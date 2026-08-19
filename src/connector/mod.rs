//! The connector seam: exactly two implementations, and no plugin mechanism.
//!
//! The spec is explicit twice over — "Connectors (exactly two)" and "so a
//! third is one file — but do not add one". So [`registry`] returns a fixed
//! pair, not a discovered set: there is no registration macro, no inventory
//! crate, no `Box<dyn Connector>` factory keyed by string. Adding a third
//! connector means writing one file and adding one line to `registry`, which
//! is cheaper than any mechanism that would make it cheaper.
//!
//! # The tolerance contract, enforced by the return types
//!
//! "one bad line never fails a file" is not a promise in prose here, it is the
//! signature: [`Connector::discover`] and [`Connector::read`] return plain
//! `Vec`s, not `Result`s. A connector has nowhere to put a fatal error, so it
//! cannot invent one. Damage is reported as data instead —
//! [`SessionMeta::parse_failures`] counts the lines that did not parse, and
//! `doctor` shows the tally. A directory that does not exist yields an empty
//! `Vec`, because a machine with no `~/.claude` is a normal machine.
//!
//! # Why `discover` takes the directories
//!
//! The spec writes the trait as `discover() -> Vec<SessionMeta>`, but it also
//! requires that "--repo resolves to the project's encoded session directories
//! (repo + worktrees) before any file opens" and that mtime "prunes files
//! outside the query window unopened". Those two are the same requirement seen
//! from the caller's side: the scope has to reach the connector before the
//! connector opens anything. So the scope is an argument rather than a
//! rediscovery inside every implementation. Pass [`Connector::roots`] and an
//! unbounded [`TimeWindow`] for the whole-machine sweep `doctor` wants.
//!
//! # Who owns a path
//!
//! Both connectors accept `*.jsonl`, so a caller that runs the whole registry
//! over one set of directories discovers every file **twice** — once as the
//! transcript it is, and once as the "0 turns, N parse failures" the
//! best-effort reader makes of the same bytes. The rule that settles it:
//! **a path belongs to the most specific connector that recognises it.**
//! [`discover_all`] enforces it, and every command that discovers sessions
//! goes through it rather than over [`registry`] itself.
//!
//! `generic_jsonl` is therefore the fallback *by construction*, not by a flag:
//! it is last in [`registry`], so it only ever reports what the specific
//! connector declined.
//!
//! # Claiming is recognition, and the window is applied after it
//!
//! A connector claims a path by **recognising** it, not by having something to
//! say about it in the caller's window. Those came apart once and it cost a
//! phantom session per out-of-window transcript: each `discover` used to apply the parsed-span filter
//! itself, so a real transcript whose turns all fell after `--until` was
//! dropped by `claude_code` — which silently released the claim, handed the
//! path to `generic_jsonl`, and let the fallback report it as "0 turns, span
//! null..null". An unbounded span overlaps every window by construction, so
//! the phantom survived the very filter that had excluded the real reading of
//! the same bytes.
//!
//! So [`discover_all`] claims by path first and applies the window **once,
//! centrally, after the claim**. A connector's `discover` returns what it
//! recognises; `window` reaches it only as the mtime prune hint the spec's
//! cheapness argument needs (see [`crate::prune`] for why that prune is
//! one-sided). The rule generalises past the bug: whether a file is in the
//! window is a property of the query, and only who *understands* the file can
//! be a property of the connector.
//!
//! # A session with no parsed timestamps, under an explicit window
//!
//! It is dropped, and this now agrees with [`crate::join`]: "unbounded" would
//! mean "overlaps everything", which is the opposite of what an absent clock
//! tells us. An empty file, or one this reader understood nothing in, is not
//! evidence that anything happened between `--since` and `--until`.
//!
//! Under an **unbounded** window it survives, unchanged — and that is what
//! keeps `doctor` whole: its whole-machine sweep asks each connector for
//! everything (`TimeWindow::default()`), where zero-turn sessions and their
//! `parse_failures`/`declined` tallies are exactly the point. `doctor` also
//! calls [`Connector::discover`] directly rather than through here, so it is
//! twice unaffected.
//!
//! # Is `generic_jsonl` reachable through `--repo`?
//!
//! Essentially no, and never for a real Claude Code transcript. `--repo`
//! resolves to this repo's encoded session directories
//! ([`crate::worktree::session_dirs`]), which are `claude_code`'s own tree, so
//! every transcript found there is claimed above it. The only paths that reach
//! it are the ones `claude_code` refuses — today the `journal.jsonl` records of
//! `docs/format-notes.md` §2.4, which are not transcripts. `doctor`'s
//! whole-machine sweep gives it nothing either: that sweep runs each connector
//! over its own [`Connector::roots`], and `generic_jsonl` declares none.
//!
//! So it runs when a caller points it at a directory of its own choosing, and
//! that is the honest description of it: an escape hatch for a tree recount was
//! pointed at, not a second opinion on the tree it already understands.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{SessionMeta, TimeWindow, Turn};

pub mod claude_code;
pub mod generic_jsonl;

pub trait Connector {
    /// Stable identifier, stored in [`SessionMeta::connector`] and printed by
    /// `doctor`. `&'static str` because there are exactly two and they are
    /// compiled in.
    fn name(&self) -> &'static str;

    /// Where this connector would look, whether or not those places exist.
    /// `doctor` reports these verbatim, including the ones that are absent —
    /// "the root you expected is not there" is the answer people need.
    fn roots(&self) -> Vec<PathBuf>;

    /// Metadata for every session under `dirs` that this connector
    /// **recognises** — its claim on those paths (see the module doc).
    ///
    /// `dirs` are session directories (see [`crate::worktree::session_dirs`]),
    /// not repo paths. A `dirs` entry that does not exist is skipped silently.
    ///
    /// `window` is a **prune hint, not a filter**: pass it to
    /// [`crate::prune::partition`] so files it rules out on mtime are never
    /// opened, and then return everything that was. Do **not** drop a session
    /// because its parsed span misses the window — that is
    /// [`discover_all`]'s job, and doing it here releases the claim to the
    /// next connector. An unbounded `window` prunes nothing.
    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta>;

    /// Every turn in one session file, in file order, with 1-based line
    /// numbers. Unreadable file or unparseable lines: fewer turns, never an
    /// error.
    fn read(&self, path: &Path) -> Vec<Turn>;
}

/// The two connectors, in preference order. Exactly two, forever, on purpose.
pub fn registry() -> Vec<Box<dyn Connector>> {
    vec![
        Box::new(claude_code::ClaudeCode),
        Box::new(generic_jsonl::GenericJsonl),
    ]
}

/// Every session under `dirs`, discovered **once**, by the first connector
/// that claims it — then filtered to `window`, once, here.
///
/// Claiming is not a new trait method: a connector claims a path by returning
/// a [`SessionMeta`] for it. Connectors run in `connectors` order (pass
/// [`registry`], which puts `claude_code` first) and a path already claimed is
/// dropped from every later connector's results. See the module doc for the
/// rule, for why the window is applied *after* the claim rather than inside
/// each `discover`, and for what that leaves `generic_jsonl` doing.
///
/// This is the registry's job rather than each command's: `sessions`, `grep`,
/// `at` and `explain` all discover over the same directories and must all get
/// the same answer. `connectors` is a parameter for the same reason
/// [`crate::cmd::doctor`] takes one — so a test can drive a known set instead
/// of whatever this machine has in `~/.claude`.
pub fn discover_all(
    connectors: &[Box<dyn Connector>],
    dirs: &[PathBuf],
    window: &TimeWindow,
) -> Vec<SessionMeta> {
    let mut claimed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::new();
    for connector in connectors {
        for meta in connector.discover(dirs, window) {
            // The claim is recorded first and unconditionally — `insert` runs
            // before `&&` short-circuits — so a path the window excludes is
            // still spoken for and cannot fall through to the fallback.
            if claimed.insert(meta.path.clone()) && in_window(&meta, window) {
                out.push(meta);
            }
        }
    }
    out
}

/// The window test, applied once, after the claim.
///
/// An unbounded window keeps everything (`doctor`'s sweep). Under an explicit
/// one, a session with no parsed timestamp is dropped rather than kept: see
/// the module doc, and [`crate::join`], which has always said the same.
fn in_window(meta: &SessionMeta, window: &TimeWindow) -> bool {
    let span = meta.window();
    window.is_unbounded() || (!span.is_unbounded() && span.overlaps(window))
}

/// Every `*.jsonl` anywhere under each of `dirs`, sorted, deduplicated, and
/// opening none of them.
///
/// Shared by both connectors and by `prune`'s callers: this is the "before any
/// file opens" boundary, and it is a free function rather than a trait method
/// so it cannot accidentally differ between the two implementations.
///
/// # Why this recurses
///
/// `docs/format-notes.md` §0 measured two shapes in the same tree, and the
/// nested one is the majority: 238 top-level `<encoded>/<session-uuid>.jsonl`
/// against 604 `<encoded>/<session-uuid>/subagents/agent-<id>.jsonl` (plus 29
/// `…/subagents/workflows/wf_<id>/journal.jsonl`). A single `read_dir` level
/// would silently miss three quarters of the corpus — and the subagent files
/// are precisely the fleet testimony this tool exists to find. Filtering the
/// shapes back apart is the connectors' job; this function's job is to miss
/// nothing. (`generic_jsonl`'s doc says it does not walk subdirectories: over
/// the flat directory of `*.jsonl` that connector describes, the recursion is
/// a no-op, so the two statements only differ on a tree it was never pointed
/// at.)
///
/// Only directory entries are consulted — no file is opened and no content is
/// read — because the mtime prune (`crate::prune`) runs on this list and the
/// spec's cheapness argument depends on the pruned files never being touched.
///
/// A `dirs` entry that does not exist, or that cannot be read, contributes
/// nothing and is not an error: a machine with no `~/.claude` is a normal
/// machine.
pub fn candidates(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        collect_jsonl(dir, &mut out);
    }
    // Sorted for determinism (readdir order is filesystem order, which differs
    // between machines and between runs); deduplicated because the path
    // encoding is not injective (format-notes §1.3), so two worktrees can
    // legitimately hand us the same session directory twice.
    out.sort();
    out.dedup();
    out
}

/// Depth-first over directory entries.
///
/// Symlinks are never descended into, so the walk cannot leave the root or
/// loop. A symlink *to* a `.jsonl` is still a candidate: following that one
/// reads a file, it does not walk a tree.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `DirEntry::file_type` does not follow symlinks — that is what makes
        // the rule above hold, and on Linux it is usually the `d_type` readdir
        // already returned rather than a second syscall.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    use crate::connector::claude_code::ClaudeCode;

    /// Create an empty file at `rel` under `dir`, parents included.
    fn touch(dir: &TempDir, rel: &str) -> PathBuf {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn registry_is_exactly_the_two() {
        let names: Vec<&str> = registry().iter().map(|c| c.name()).collect();
        assert_eq!(names, ["claude_code", "generic_jsonl"]);
    }

    /// The wave-2 integration bug: run the registry over one set of dirs and
    /// every transcript is discovered twice, the second time as garbage.
    #[test]
    fn discover_all_gives_each_path_to_the_first_connector_that_claims_it() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("-tmp-repo");
        let transcript = dir.join("s.jsonl");
        let journal = dir.join("s/subagents/workflows/wf_1/journal.jsonl");
        fs::create_dir_all(journal.parent().unwrap()).unwrap();
        fs::write(
            &transcript,
            "{\"type\":\"user\",\"timestamp\":\"2026-08-19T07:00:00.000Z\",\
             \"cwd\":\"/tmp/repo\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        )
        .unwrap();
        fs::write(&journal, "{\"type\":\"started\",\"agentId\":\"wf\"}\n").unwrap();

        let dirs = vec![dir];
        let window = TimeWindow::default();

        // What every caller used to do, and the defect itself: `s.jsonl` comes
        // back twice, the second time from the fallback which cannot read it.
        let ungoverned: Vec<SessionMeta> = registry()
            .iter()
            .flat_map(|c| c.discover(&dirs, &window))
            .collect();
        assert_eq!(ungoverned.len(), 3);

        let once = discover_all(&registry(), &dirs, &window);
        assert_eq!(once.len(), 2, "one entry per path");
        let by_path = |p: &PathBuf| once.iter().find(|m| &m.path == p).expect("discovered");
        let s = by_path(&transcript);
        assert_eq!(s.connector, "claude_code", "the specific connector wins");
        assert_eq!(s.turn_count, 1);
        assert_eq!(s.parse_failures, 0);
        // And the fallback still gets what the specific connector declined
        // (§2.4: a journal is not a transcript) — which is the only way it is
        // reachable through `--repo` at all.
        assert_eq!(by_path(&journal).connector, "generic_jsonl");
    }

    /// The W5-barrier defect: a real `claude_code` transcript whose turns all
    /// fall after `--until` must be **absent**, not reappear under the
    /// fallback as "0 turns, span null..null".
    ///
    /// It used to, because `claude_code::discover` applied the window itself
    /// and thereby released its claim; `generic_jsonl` then picked the path up,
    /// parsed nothing out of it, and reported an unbounded span — which
    /// overlaps every window by construction, so it survived the very filter
    /// that had excluded it.
    #[test]
    fn a_recognised_file_out_of_window_is_absent_not_reclaimed_by_the_fallback() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("-tmp-repo");
        let early = dir.join("early.jsonl");
        let late = dir.join("late.jsonl");
        fs::create_dir_all(&dir).unwrap();
        let turn = |ts: &str| {
            format!(
                "{{\"type\":\"user\",\"timestamp\":\"{ts}\",\"cwd\":\"/tmp/repo\",\
                 \"message\":{{\"role\":\"user\",\"content\":\"hi\"}}}}\n"
            )
        };
        fs::write(&early, turn("2026-08-19T07:00:00.000Z")).unwrap();
        fs::write(&late, turn("2026-08-19T09:00:00.000Z")).unwrap();

        let dirs = vec![dir];
        let until = TimeWindow {
            since: None,
            until: crate::model::parse_ts("2026-08-19T08:20:00Z").ok(),
        };

        // `claude_code` claims both — recognition, not windowing — so the
        // fallback is never offered `late.jsonl`.
        let claimed = ClaudeCode.discover(&dirs, &until);
        assert_eq!(claimed.len(), 2, "both are recognised");

        let found = discover_all(&registry(), &dirs, &until);
        assert_eq!(
            found.iter().map(|m| &m.path).collect::<Vec<_>>(),
            vec![&early],
            "the out-of-window transcript is gone, not relabelled"
        );
        assert!(
            !found.iter().any(|m| m.connector == "generic_jsonl"),
            "the fallback must not claim what `claude_code` understood"
        );
        assert!(
            found.iter().all(|m| m.window().since.is_some()),
            "no null spans survive an explicit window"
        );

        // Unbounded, both come back — `doctor`'s sweep still sees everything.
        assert_eq!(
            discover_all(&registry(), &dirs, &TimeWindow::default()).len(),
            2
        );
    }

    /// A session with no parsed timestamp at all: kept when nothing was asked,
    /// dropped by an explicit window. Same rule as [`crate::join`] — unbounded
    /// would mean "always", which is the opposite of what a missing clock says.
    #[test]
    fn an_unbounded_span_survives_no_window_and_no_explicit_one() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("-tmp-repo");
        let empty = dir.join("empty.jsonl");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&empty, "").unwrap();
        let dirs = vec![dir];

        assert_eq!(
            discover_all(&registry(), &dirs, &TimeWindow::default()).len(),
            1,
            "doctor's sweep wants the zero-turn session and its tallies"
        );
        for w in [
            TimeWindow {
                // Deliberately older than the file's mtime, so the mtime
                // prune cannot be the reason it is absent.
                since: crate::model::parse_ts("1999-01-01T00:00:00Z").ok(),
                until: None,
            },
            TimeWindow {
                since: None,
                until: crate::model::parse_ts("2026-08-19T08:20:00Z").ok(),
            },
        ] {
            assert!(
                discover_all(&registry(), &dirs, &w).is_empty(),
                "an absent clock is not evidence of activity in {w:?}"
            );
        }
    }

    /// Both layouts from `docs/format-notes.md`, plus the things that are not
    /// sessions, plus a directory that does not exist.
    #[test]
    fn candidates_finds_both_layouts_and_nothing_else() {
        let d = tempfile::tempdir().unwrap();
        let top = touch(&d, "proj/0000-session.jsonl");
        let nested = touch(&d, "proj/1111-session/subagents/agent-ab.jsonl");
        let journal = touch(
            &d,
            "proj/1111-session/subagents/workflows/wf_1/journal.jsonl",
        );
        touch(&d, "proj/1111-session/subagents/agent-ab.meta.json");
        touch(&d, "proj/notes.txt");
        touch(&d, "proj/jsonl");
        fs::create_dir_all(d.path().join("empty")).unwrap();

        let dirs = vec![
            d.path().join("proj"),
            d.path().join("empty"),
            d.path().join("does-not-exist"),
            // Duplicated on purpose: the encoding is not injective.
            d.path().join("proj"),
        ];
        assert_eq!(candidates(&dirs), vec![top, nested, journal]);
    }

    #[cfg(unix)]
    #[test]
    fn candidates_does_not_follow_symlinked_directories_out_of_the_root() {
        let d = tempfile::tempdir().unwrap();
        let inside = touch(&d, "root/real.jsonl");
        let outside = touch(&d, "elsewhere/escaped.jsonl");
        let root = d.path().join("root");
        std::os::unix::fs::symlink(d.path().join("elsewhere"), root.join("link")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked.jsonl")).unwrap();

        // `escaped.jsonl` is absent: the symlinked directory was not entered.
        assert_eq!(
            candidates(std::slice::from_ref(&root)),
            vec![root.join("linked.jsonl"), inside]
        );
    }
}
