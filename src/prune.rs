//! mtime windowing: the files a query provably need not open.
//!
//! recount owns no index — "session files are the only truth" — so scans are
//! made cheap structurally instead. This module is half of that: "sessions are
//! append-only, so mtime prunes files outside the query window unopened".
//!
//! # The argument is one-sided, and that matters
//!
//! An append-only file's mtime is the time of its *last* write, so it bounds
//! the last turn and says nothing about the first. Therefore:
//!
//! * a file whose mtime is **before** `window.since` cannot contain a turn
//!   inside the window — every turn it holds is older than its mtime. It is
//!   skipped, unopened. The lower bound prunes.
//! * a file whose mtime is **after** `window.until` may still contain turns
//!   from before `until` — a session that started this morning and was
//!   appended to a minute ago. It must be opened. The upper bound does not
//!   prune.
//!
//! Getting this backwards would silently lose evidence, which is why it is
//! written down rather than left to the reader of the `if`.
//!
//! An unbounded window prunes nothing. A file whose mtime cannot be read is
//! opened, not skipped: uncertainty resolves toward looking.
//!
//! # The invariant was measured, not assumed
//!
//! `docs/format-notes.md` §6.2 checked `mtime − last_timestamp` over 842 real
//! transcripts: **min 0.0 s, median 0.1 s, and not one file with an mtime
//! earlier than its last timestamp**. That is what licenses the lower-bound
//! prune.
//!
//! The same measurement is why the upper bound is refused. mtime overshoots
//! the last turn by up to **22.72 days** (p99 ~30 h), because timestamp-less
//! metadata lines — `last-prompt`, `ai-title`, `agent-name` — are rewritten
//! when a session is resumed or renamed in the UI. Overshoot is harmless in
//! the pruning direction (it only makes us open a file we could have skipped,
//! so the prune is conservative, never precise) and fatal in the other: a file
//! touched today can hold nothing but last month's turns. Never report mtime
//! to a user as "last activity" either.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::model::TimeWindow;

/// Candidates split by the mtime test. Both halves are kept: `skipped` is not
/// waste, it is what `doctor` and the bench report to show the pruning worked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Partition {
    /// Files that must be read.
    pub opened: Vec<PathBuf>,
    /// Files mtime ruled out. Never opened.
    pub skipped: Vec<PathBuf>,
}

/// Partition `candidates` by whether `window` can rule them out on mtime alone.
///
/// Order within each half follows `candidates`. Never opens a file, and never
/// fails: a candidate it cannot stat goes to `opened`, where the reader that
/// does open it will report the real error in context.
pub fn partition(candidates: &[PathBuf], window: &TimeWindow) -> Partition {
    // Only `since` prunes (see the module doc), so without one there is
    // nothing to decide and not a single stat call to make.
    let Some(since) = window.since else {
        return Partition {
            opened: candidates.to_vec(),
            skipped: Vec::new(),
        };
    };
    let since = SystemTime::from(since);

    let mut part = Partition::default();
    for path in candidates {
        let ruled_out = fs::metadata(path)
            .and_then(|m| m.modified())
            .is_ok_and(|mtime| mtime < since);
        if ruled_out {
            part.skipped.push(path.clone());
        } else {
            part.opened.push(path.clone());
        }
    }
    part
}

#[cfg(test)]
mod tests {
    use std::fs::{File, FileTimes};
    use std::path::Path;

    use chrono::{DateTime, Utc};

    use super::*;
    use crate::model::parse_ts;

    fn ts(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    /// mtimes are set explicitly — a test that raced the wall clock would be
    /// testing the machine, not the rule.
    fn file_at(dir: &Path, name: &str, mtime: DateTime<Utc>) -> PathBuf {
        let path = dir.join(name);
        let f = File::create(&path).unwrap();
        f.set_times(FileTimes::new().set_modified(SystemTime::from(mtime)))
            .unwrap();
        path
    }

    fn window(since: Option<&str>, until: Option<&str>) -> TimeWindow {
        TimeWindow {
            since: since.map(ts),
            until: until.map(ts),
        }
    }

    /// The whole bead in one assertion: `since` prunes, `until` does not.
    #[test]
    fn lower_bound_prunes_and_upper_bound_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let before = file_at(dir.path(), "before.jsonl", ts("2026-08-01T00:00:00Z"));
        let inside = file_at(dir.path(), "inside.jsonl", ts("2026-08-10T00:00:00Z"));
        // Appended to after the window closed — but an append-only file
        // touched late can still be full of turns from inside the window.
        let after = file_at(dir.path(), "after.jsonl", ts("2026-08-19T00:00:00Z"));

        let part = partition(
            &[before.clone(), inside.clone(), after.clone()],
            &window(Some("2026-08-05T00:00:00Z"), Some("2026-08-12T00:00:00Z")),
        );

        assert_eq!(
            part.skipped,
            vec![before],
            "only the pre-window file prunes"
        );
        assert_eq!(part.opened, vec![inside, after], "order follows candidates");
    }

    /// A file exactly at `since` is inside the window, not before it.
    #[test]
    fn mtime_equal_to_since_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let edge = file_at(dir.path(), "edge.jsonl", ts("2026-08-05T00:00:00Z"));

        let part = partition(
            std::slice::from_ref(&edge),
            &window(Some("2026-08-05T00:00:00Z"), None),
        );

        assert_eq!(part.opened, vec![edge]);
        assert!(part.skipped.is_empty());
    }

    /// No `since` — unbounded, or bounded only above — prunes nothing.
    #[test]
    fn no_lower_bound_opens_everything() {
        let dir = tempfile::tempdir().unwrap();
        let ancient = file_at(dir.path(), "ancient.jsonl", ts("1999-01-01T00:00:00Z"));
        let cands = [ancient];

        for w in [
            TimeWindow::default(),
            window(None, Some("2026-08-12T00:00:00Z")),
        ] {
            assert!(w.is_unbounded() || w.until.is_some());
            let part = partition(&cands, &w);
            assert_eq!(part.opened, cands, "no since means no prune ({w:?})");
            assert!(part.skipped.is_empty());
        }
    }

    /// The acceptance criterion: proof by *opens*, not by timing.
    ///
    /// The pruned file is mode 000, so any `File::open` inside `partition`
    /// errors; the unstattable candidate does not exist at all, so an
    /// implementation that opened it would have to invent an answer. Nothing
    /// here is a stopwatch, and nothing here passes on a warm cache.
    #[cfg(unix)]
    #[test]
    fn pruned_files_are_never_opened_and_missing_ones_never_fail() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let unopenable = file_at(dir.path(), "unopenable.jsonl", ts("2026-08-01T00:00:00Z"));
        fs::set_permissions(&unopenable, fs::Permissions::from_mode(0o000)).unwrap();
        // Stat still works; open must not. (Root defeats mode 000, so under
        // root this asserts the partition and not the no-open.)
        assert!(fs::metadata(&unopenable).is_ok());

        let missing = dir.path().join("deleted-mid-scan.jsonl");

        let part = partition(
            &[unopenable.clone(), missing.clone()],
            &window(Some("2026-08-05T00:00:00Z"), None),
        );

        assert_eq!(
            part.skipped,
            vec![unopenable],
            "pruned without being opened"
        );
        assert_eq!(
            part.opened,
            vec![missing],
            "unstattable resolves to opening"
        );
    }
}
