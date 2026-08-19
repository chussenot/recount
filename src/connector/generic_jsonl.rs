//! Connector 2 of 2: any directory of `*.jsonl`, **best-effort**.
//!
//! The spec's whole description is one line: "any dir of *.jsonl with
//! {ts|timestamp, role, content|text}; documented best-effort." That is the
//! entire contract, and this module is deliberately not larger than it. It is
//! the escape hatch for a transcript this tool has never seen, not a format
//! framework.
//!
//! # What it accepts
//!
//! One JSON object per line. Blank lines are skipped and cost nothing.
//!
//! * **timestamp** — `ts`, else `timestamp`. `ts` wins when both are present
//!   and disagree, because it is the spelling the spec names first; there is
//!   no better tiebreaker available and picking one silently is worse than
//!   picking one out loud. The value must be an RFC 3339 string, parsed by the
//!   same [`crate::model::parse_ts`] that reads `--ts` and pact's ledger — one
//!   timestamp dialect in the whole tool. Epoch numbers, `YYYY-MM-DD hh:mm:ss`
//!   and friends are not accepted.
//! * **role** — `role`, a string. Unrecognised spellings survive as
//!   [`Role::Other`]; that is the model's job, not this module's.
//! * **text** — `content`, else `text`, same precedence and same reason. A
//!   string is taken as-is. An array is flattened: each element contributes
//!   itself if it is a string, or its `text` field if it has a string one, and
//!   is skipped otherwise; the pieces are joined with newlines. There is no
//!   dispatch on a block's `type`. Absent or unusable text is an empty string,
//!   not a failure — a turn can legitimately carry no prose.
//!
//! A key present but `null` counts as absent, so `{"ts": null, "timestamp":
//! "…"}` reads the one that is actually there. Unknown fields are ignored.
//!
//! # What it will NOT do
//!
//! * **No cwd.** Nothing in this format states a working directory, so
//!   [`SessionMeta::cwd`] is always `None` and the topological join will
//!   correctly refuse these sessions with "no cwd match". Inferring one from
//!   the directory name would be inventing evidence.
//! * **No roots.** [`Connector::roots`] returns empty: "any dir" has no
//!   natural place on the filesystem, the caller supplies the dirs. `doctor`
//!   printing nothing for this connector is the honest answer; a made-up root
//!   would read as a place recount had looked and found empty.
//! * **No block-type parser.** Rendering `tool_use`/`tool_result`/`thinking`
//!   blocks is `claude_code`'s job, against schema notes it owns. Duplicating
//!   it here is exactly the coupling "best-effort" exists to avoid.
//! * **No sniffing, no repair.** No guessing which tool wrote a file; no
//!   fixing up a record that does not already have what the spec names.
//!
//! # It does walk subdirectories (this doc used to say otherwise)
//!
//! Discovery is [`crate::connector::candidates`], shared with `claude_code`,
//! and it recurses. That is deliberate: `docs/format-notes.md` §0.1 measured
//! 604 of 871 real transcripts one level down, at
//! `<session-uuid>/subagents/agent-<hex>.jsonl`, and a one-level listing would
//! silently miss three quarters of a corpus — the exact shape of failure this
//! tool exists to prevent. Over the flat directory of `*.jsonl` that the spec
//! line describes, the recursion is a no-op and this connector behaves exactly
//! as before; pointed at a tree, it finds every `*.jsonl` in it. Missing
//! nothing is the right default for a best-effort reader, so the walk stands
//! and the promise it broke is what changed.
//! * **No timeless turns.** A line whose timestamp is missing or unparseable
//!   is declined, counted and skipped — not a turn with a guessed time. The
//!   join is a time-window overlap, so a turn with no time cannot take part in
//!   it; admitting one would only let it be silently dropped later, or worse,
//!   matched. Same for a line with no `role`.
//!
//! # `declined` and `parse_failures` — this reader says which
//!
//! Per the spec's "one bad line never fails a file", neither is an error; both
//! are tallies [`crate::cmd::doctor`] shows. The split is the one
//! [`SessionMeta`] defines, and here the line between them is exactly whether
//! `serde_json` succeeded:
//!
//! * **`parse_failures`** — the line is not JSON. Damage. A truncated last
//!   line, the normal state of a file a live process is still appending to,
//!   lands here; this reader cannot tell it from a corrupted one, and says so
//!   rather than guessing.
//! * **`declined`** — the line is perfectly good JSON that is simply not
//!   turn-shaped: a header record, a summary line, an event from some other
//!   stream sharing the directory, or a turn with no usable timestamp or role.
//!   Nothing is wrong with the file; this reader accepts one shape and that
//!   was not it. Calling that a failure would be blaming the transcript for
//!   the narrowness of the contract on the tin.
//!
//! An unreadable file yields no turns and neither tally: there were no lines
//! to judge. Invalid UTF-8 is lossily replaced rather than discarding the
//! whole transcript over one bad byte.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::connector::{candidates, Connector};
use crate::model::{parse_ts, Role, SessionMeta, TimeWindow, Turn};
use crate::prune;

const NAME: &str = "generic_jsonl";

pub struct GenericJsonl;

impl Connector for GenericJsonl {
    fn name(&self) -> &'static str {
        NAME
    }

    /// Empty, on purpose — see the module doc.
    fn roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta> {
        // `window` is the mtime prune and nothing else; the span filter lives
        // once, in `connector::discover_all`. See its module doc — a connector
        // that filters here silently releases its claim on the path.
        prune::partition(&candidates(dirs), window)
            .opened
            .iter()
            .map(|path| scan(path))
            .collect()
    }

    fn read(&self, path: &Path) -> Vec<Turn> {
        read_turns(path).0
    }
}

/// One file's metadata: the same parse as [`Connector::read`], keeping the ends
/// and the tally instead of the turns.
fn scan(path: &Path) -> SessionMeta {
    let (turns, parse_failures, declined) = read_turns(path);
    // min/max rather than first/last: these files are append-only in practice,
    // but a span that lies is a silently wrong join, and the correct version is
    // the same length.
    SessionMeta {
        path: path.to_path_buf(),
        cwd: None,
        first_ts: turns.iter().map(|t| t.ts).min(),
        last_ts: turns.iter().map(|t| t.ts).max(),
        turn_count: turns.len(),
        parse_failures,
        declined,
        connector: NAME.to_string(),
    }
}

/// Every turn in `path`, in file order, plus `(damaged, declined)` for the
/// lines that were not one — see the module doc for which is which. Never
/// fails: an unreadable file is `(vec![], 0, 0)`.
fn read_turns(path: &Path) -> (Vec<Turn>, usize, usize) {
    let Ok(bytes) = fs::read(path) else {
        return (Vec::new(), 0, 0);
    };
    let mut turns = Vec::new();
    let (mut damaged, mut declined) = (0, 0);
    for (i, line) in String::from_utf8_lossy(&bytes).lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // The one question that separates the two tallies: did it parse?
        match serde_json::from_str::<Value>(line) {
            Err(_) => damaged += 1,
            Ok(value) => match turn_from_value(&value, i + 1) {
                Some(turn) => turns.push(turn),
                None => declined += 1,
            },
        }
    }
    (turns, damaged, declined)
}

/// `None` for JSON that is not turn-shaped — no usable timestamp, or no role.
fn turn_from_value(value: &Value, number: usize) -> Option<Turn> {
    Some(Turn {
        ts: parse_ts(either(value, "ts", "timestamp")?.as_str()?).ok()?,
        role: Role::from(value.get("role")?.as_str()?.to_string()),
        text: either(value, "content", "text")
            .map(flatten)
            .unwrap_or_default(),
        line: number,
    })
}

/// `first` if it is present and not null, else `second`.
fn either<'a>(value: &'a Value, first: &str, second: &str) -> Option<&'a Value> {
    value
        .get(first)
        .filter(|v| !v.is_null())
        .or_else(|| value.get(second).filter(|v| !v.is_null()))
}

/// A string, or an array of strings and `{text: …}`-ish blocks, joined by
/// newlines. Anything else flattens to nothing.
fn flatten(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.as_str().or_else(|| b.get("text")?.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `discover` is exercised at the bottom of this module. It was not, when
    // this connector landed: it routes through `connector::candidates` and
    // `prune::partition`, both `todo!()` in that worktree at the time. Both
    // are real now (recount-hwk.24, .32), so recount-hwk.33 — the first
    // caller — proves the path.

    /// Write `body` at `path`, creating parents, with an explicit mtime — a
    /// test that raced the wall clock would be testing the machine, not the
    /// rule.
    fn write_at(path: &Path, body: &str, mtime: &str) -> PathBuf {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_times(
            fs::FileTimes::new()
                .set_modified(std::time::SystemTime::from(parse_ts(mtime).unwrap())),
        )
        .unwrap();
        path.to_path_buf()
    }

    fn file(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        fs::write(&path, body).unwrap();
        (dir, path)
    }

    fn texts(turns: &[Turn]) -> Vec<&str> {
        turns.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn takes_either_spelling_of_either_pair() {
        let (_d, path) = file(concat!(
            r#"{"ts":"2026-08-19T07:00:00Z","role":"user","content":"one"}"#,
            "\n",
            r#"{"timestamp":"2026-08-19T07:01:00Z","role":"assistant","text":"two"}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:02:00Z","role":"user","text":"three"}"#,
            "\n",
            r#"{"timestamp":"2026-08-19T07:03:00Z","role":"assistant","content":"four"}"#,
            "\n",
        ));
        let turns = GenericJsonl.read(&path);
        assert_eq!(texts(&turns), ["one", "two", "three", "four"]);
        assert_eq!(
            turns.iter().map(|t| t.line).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(turns[0].role, Role::User);
        assert_eq!(turns[1].role, Role::Assistant);
        assert_eq!(turns[3].ts, parse_ts("2026-08-19T07:03:00Z").unwrap());
    }

    /// The documented tiebreaker: `ts` beats `timestamp`, `content` beats
    /// `text`. A null key does not win by merely being present.
    #[test]
    fn first_spelling_wins_unless_it_is_null() {
        let (_d, path) = file(concat!(
            r#"{"ts":"2026-08-19T07:00:00Z","timestamp":"2020-01-01T00:00:00Z","#,
            r#""role":"user","content":"kept","text":"dropped"}"#,
            "\n",
            r#"{"ts":null,"timestamp":"2026-08-19T07:01:00Z","#,
            r#""role":"user","content":null,"text":"fallback"}"#,
            "\n",
        ));
        let turns = GenericJsonl.read(&path);
        assert_eq!(texts(&turns), ["kept", "fallback"]);
        assert_eq!(turns[0].ts, parse_ts("2026-08-19T07:00:00Z").unwrap());
        assert_eq!(turns[1].ts, parse_ts("2026-08-19T07:01:00Z").unwrap());
    }

    #[test]
    fn flattens_string_and_block_array_content_and_ignores_unknown_fields() {
        let (_d, path) = file(concat!(
            r#"{"ts":"2026-08-19T07:00:00Z","role":"weird","whatever":{"a":1},"#,
            r#""content":[{"type":"text","text":"a"},{"type":"tool_use","id":"x"},"b",7]}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:01:00Z","role":"assistant","content":[]}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:02:00Z","role":"assistant"}"#,
            "\n",
        ));
        let turns = GenericJsonl.read(&path);
        assert_eq!(texts(&turns), ["a\nb", "", ""]);
        assert_eq!(turns[0].role, Role::Other("weird".into()));
    }

    #[test]
    fn a_garbage_line_mid_file_is_counted_not_fatal() {
        let (_d, path) = file(concat!(
            r#"{"ts":"2026-08-19T07:00:00Z","role":"user","content":"before"}"#,
            "\n",
            "this is not json at all\n",
            "\n",
            r#"{"nope":true}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:02:00Z","role":"user","content":"after"}"#,
            "\n",
        ));
        let turns = GenericJsonl.read(&path);
        assert_eq!(texts(&turns), ["before", "after"]);
        // Line numbers are the file's, not the turns': 1 and 5.
        assert_eq!(turns[1].line, 5);

        let meta = scan(&path);
        assert_eq!(meta.turn_count, 2);
        // Two lines were not turns, and they are not the same kind of not.
        assert_eq!(meta.parse_failures, 1, "only `this is not json at all`");
        assert_eq!(
            meta.declined, 1,
            "the `nope` line is fine JSON, just not a turn"
        );
        assert_eq!(meta.cwd, None);
        assert_eq!(meta.connector, "generic_jsonl");
        assert_eq!(meta.first_ts, parse_ts("2026-08-19T07:00:00Z").ok());
        assert_eq!(meta.last_ts, parse_ts("2026-08-19T07:02:00Z").ok());
    }

    /// The normal state of a file a live process is still appending to.
    #[test]
    fn a_truncated_last_line_is_counted_not_fatal() {
        let (_d, path) = file(concat!(
            r#"{"ts":"2026-08-19T07:00:00Z","role":"user","content":"whole"}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:01:00Z","role":"assist"#,
        ));
        let meta = scan(&path);
        assert_eq!(meta.turn_count, 1);
        // Indistinguishable from real damage through this reader, and reported
        // as such rather than guessed at (format-notes §7.4).
        assert_eq!(meta.parse_failures, 1);
        assert_eq!(meta.declined, 0);
        assert_eq!(meta.last_ts, parse_ts("2026-08-19T07:00:00Z").ok());
    }

    #[test]
    fn an_empty_file_is_an_empty_session_not_a_failure() {
        let (_d, path) = file("");
        assert!(GenericJsonl.read(&path).is_empty());
        let meta = scan(&path);
        assert_eq!(meta.turn_count, 0);
        assert_eq!(meta.parse_failures, 0);
        assert_eq!(meta.first_ts, None);
        assert_eq!(meta.last_ts, None);
        assert!(meta.window().is_unbounded());
    }

    /// A missing or non-RFC-3339 timestamp, and a missing role, are declines:
    /// a turn with no time cannot join, but there is nothing wrong with the
    /// file — this reader takes one shape and that was not it.
    #[test]
    fn timeless_and_roleless_lines_are_declined_not_damaged() {
        let (_d, path) = file(concat!(
            r#"{"role":"user","content":"no time"}"#,
            "\n",
            r#"{"ts":1755586992,"role":"user","content":"epoch is not rfc3339"}"#,
            "\n",
            r#"{"ts":"2026-08-19 07:00:00","role":"user","content":"nor is this"}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:00:00Z","content":"no role"}"#,
            "\n",
            r#"{"ts":"2026-08-19T07:00:00Z","role":7,"content":"role is not a string"}"#,
            "\n",
        ));
        let meta = scan(&path);
        assert_eq!(meta.turn_count, 0);
        assert_eq!(meta.declined, 5);
        assert_eq!(meta.parse_failures, 0, "every one of them parsed");
    }

    #[test]
    fn a_missing_file_is_empty_and_never_panics() {
        let dir = tempfile::tempdir().unwrap();
        let meta = scan(&dir.path().join("nope.jsonl"));
        assert_eq!(meta.turn_count, 0);
        assert_eq!(meta.parse_failures, 0);
        assert!(GenericJsonl.read(&dir.path().join("nope.jsonl")).is_empty());
    }

    #[test]
    fn has_no_roots_because_any_dir_has_no_root() {
        assert!(GenericJsonl.roots().is_empty());
        assert_eq!(GenericJsonl.name(), "generic_jsonl");
    }

    /// The one path wave 1 could not test: `discover` end to end, over the
    /// real `candidates` walk and the real `prune::partition`.
    ///
    /// Three files, three different fates, one call:
    ///
    /// * `old.jsonl` — mtime before `since`. **Pruned unopened** by
    ///   `prune::partition`; it is mode 000, so an implementation that opened
    ///   it would read nothing and could not report its turn.
    /// * `nest/deep/new.jsonl` — one level down, and then another. Found only
    ///   because `candidates` recurses (see the module doc), and kept.
    /// * `touched.jsonl` — mtime inside the window, turns entirely before it.
    ///   mtime cannot rule it out (the prune is one-sided, by design), so it
    ///   is opened, and **still claimed**: `discover` no longer filters on the
    ///   parsed span, because a connector that does silently releases the path
    ///   to the next one. `discover_all` drops it, and that is asserted too.
    #[test]
    fn discover_recurses_prunes_on_mtime_and_then_on_the_real_span() {
        let dir = tempfile::tempdir().unwrap();
        let turn = |ts: &str| format!(r#"{{"ts":"{ts}","role":"user","content":"hi"}}"#);

        let old = write_at(
            &dir.path().join("old.jsonl"),
            &turn("2026-08-10T00:00:00Z"),
            "2026-08-01T00:00:00Z",
        );
        let nested = write_at(
            &dir.path().join("nest/deep/new.jsonl"),
            &turn("2026-08-10T00:00:00Z"),
            "2026-08-10T00:00:00Z",
        );
        let touched = write_at(
            &dir.path().join("touched.jsonl"),
            &turn("2026-07-01T00:00:00Z"),
            "2026-08-12T00:00:00Z",
        );
        // Only reachable by opening it — and it must never be opened.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&old, fs::Permissions::from_mode(0o000)).unwrap();
        }

        let dirs = vec![dir.path().to_path_buf()];
        let window = TimeWindow {
            since: parse_ts("2026-08-05T00:00:00Z").ok(),
            until: None,
        };
        let found = GenericJsonl.discover(&dirs, &window);
        assert_eq!(
            found.iter().map(|m| &m.path).collect::<Vec<_>>(),
            vec![&nested, &touched],
            "old.jsonl prunes on mtime; touched.jsonl is opened and still claimed"
        );
        assert_eq!(found[0].turn_count, 1);
        assert_eq!(found[0].connector, "generic_jsonl");
        // The span rules `touched.jsonl` out exactly once, in `discover_all`.
        // This connector alone, not the registry: `claude_code` is first and
        // would claim these files itself (they are `*.jsonl` and not
        // journals), which is the whole point of the ordering.
        let only_this: Vec<Box<dyn Connector>> = vec![Box::new(GenericJsonl)];
        assert_eq!(
            crate::connector::discover_all(&only_this, &dirs, &window)
                .iter()
                .map(|m| &m.path)
                .collect::<Vec<_>>(),
            vec![&nested],
            "the window is applied centrally, after the claim"
        );

        // Unbounded: nothing prunes, everything the walk found comes back —
        // including the nested one and the unreadable one (no turns, no
        // failures, per the tolerance contract).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&old, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let all = GenericJsonl.discover(&dirs, &TimeWindow::default());
        let mut paths: Vec<_> = all.iter().map(|m| m.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec![nested, old, touched]);
    }
}
