//! A tolerant reader for `.pact/events.jsonl`.
//!
//! The dependency is one-way. recount reads pact's ledger; pact never learns
//! recount exists. That means no pact crate, no shared schema type, no version
//! negotiation — just JSON lines with the five fields the join needs, and
//! every other field ignored.
//!
//! # `chain_hash` is ignored, on purpose
//!
//! Each event carries a `chain_hash` linking it to its predecessor. recount
//! does not verify it. Verifying it would make recount a second pact: it would
//! need pact's hashing rules, would have to track them across pact versions,
//! and would start failing audits for ledger-integrity reasons that are pact's
//! job to report. recount's job is testimony, not custody. A tampered ledger
//! produces wrong testimony, and that is pact's alarm to raise.
//!
//! # Line addressing is 1-based
//!
//! `explain --event-line N` means "the Nth line of the file", the number `sed
//! -n Np` and every editor agree on. Unparseable lines still consume their
//! number: line 7 is line 7 whether or not it parsed, otherwise `--event-line`
//! would silently shift under a corrupt ledger. Bad lines are counted in
//! [`Events::parse_failures`] and skipped.
//!
//! # A missing ledger is empty, not an error
//!
//! `read` on a path that does not exist returns an empty [`Events`], because
//! "pact has never run here" is a normal input, not a failure: recount is run
//! from repos that have no `.pact/` at all, and the spec's calm-degradation
//! obligation would otherwise be re-implemented as a `NotFound` match in every
//! caller. Only a file that exists and cannot be read is an `Err`. A caller
//! that needs the distinction — `doctor`'s "events present?" — asks the
//! filesystem: `events_path(repo).exists()`. Everything else can treat "no
//! ledger" and "empty ledger" the same, which is what they are.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::parse_ts;

/// One ledger entry, reduced to the fields the join uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// 1-based line number in `.pact/events.jsonl`.
    pub line: usize,
    /// When pact recorded it.
    pub at: DateTime<Utc>,
    /// The fleet identity that acted. The join's `--agent`.
    pub agent: String,
    /// The repo-relative path the event is about, when it is about one.
    pub path: Option<String>,
    /// The worktree name the agent acted from — the first link in the
    /// topological join chain `invoked_from -> worktree path -> session cwd`.
    pub invoked_from: Option<String>,
    /// Free text pact attached, shown in `explain`'s digest.
    pub detail: Option<String>,
}

/// A whole ledger, plus what it cost to read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Events {
    pub events: Vec<Event>,
    /// Lines that did not parse. Counted, never fatal.
    pub parse_failures: usize,
    /// Total lines seen, so an out-of-range `--event-line` can say how many
    /// there were.
    pub line_count: usize,
}

impl Events {
    /// The event on 1-based line `n`.
    ///
    /// Out of range, or a line that did not parse, is an error naming the line
    /// count — never a panic and never a silent neighbour.
    pub fn line(&self, n: usize) -> Result<&Event> {
        if n == 0 || n > self.line_count {
            bail!(
                "no event on line {n}: lines are 1-based and the ledger has {}",
                self.line_count
            );
        }
        // Events are pushed in file order, so the line numbers are sorted.
        self.events
            .binary_search_by_key(&n, |e| e.line)
            .map(|i| &self.events[i])
            .map_err(|_| anyhow!("line {n} of {} did not parse as an event", self.line_count))
    }
}

/// `<repo>/.pact/events.jsonl`. Existence is not checked: a repo pact has
/// never run in is a normal input, and `doctor` reports the absence.
pub fn events_path(repo: &Path) -> PathBuf {
    repo.join(".pact").join("events.jsonl")
}

/// Read a ledger. A missing file is an empty [`Events`], not an error — the
/// spec's calm degradation applies to "events but NO sessions" and to no
/// events at all.
pub fn read(path: &Path) -> Result<Events> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Events::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    // Lossy, not strict: a ledger truncated mid-codepoint must cost one line,
    // not the whole file. `str::lines` then handles CRLF and the trailing
    // newline, so `line_count` is exactly what `wc -l` and `sed -n Np` see.
    let text = String::from_utf8_lossy(&bytes);
    let mut out = Events::default();
    for (i, raw) in text.lines().enumerate() {
        // Every physical line consumes a number, parsed or not, so
        // `--event-line N` keeps pointing at the same row of the file.
        let line = i + 1;
        out.line_count = line;
        match parse_line(raw, line) {
            Ok(event) => out.events.push(event),
            Err(_) => out.parse_failures += 1,
        }
    }
    Ok(out)
}

/// The subset of the ledger row recount reads. Every other field pact writes
/// — `kind`, `scope`, `ttl_secs`, `head`, `pact_version`, `protocol_hash`,
/// `context_key`, `chain_hash`, and whatever 0.10.8 adds — is dropped here by
/// serde's default of ignoring unknown fields. That default IS the tolerance
/// contract: a new pact field must never turn into a parse failure.
#[derive(Deserialize)]
struct Row {
    at: String,
    agent: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    invoked_from: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

fn parse_line(raw: &str, line: usize) -> Result<Event> {
    let row: Row = serde_json::from_str(raw)?;
    Ok(Event {
        line,
        at: parse_ts(&row.at)?,
        agent: row.agent,
        path: row.path,
        invoked_from: row.invoked_from,
        detail: row.detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This repo's own ledger. It grows while the fleet runs, so every
    /// assertion here is a property, never a count.
    fn real_ledger() -> Events {
        read(&events_path(Path::new(env!("CARGO_MANIFEST_DIR")))).unwrap()
    }

    #[test]
    fn reads_this_repos_own_ledger() {
        let ev = real_ledger();
        assert!(ev.line_count > 0, "this repo has a .pact/events.jsonl");
        assert_eq!(ev.parse_failures, 0, "pact writes lines recount can read");
        assert_eq!(ev.events.len(), ev.line_count);
        // Dense and 1-based: line i of the file is events[i - 1].
        for (i, e) in ev.events.iter().enumerate() {
            assert_eq!(e.line, i + 1);
            assert!(!e.agent.is_empty());
        }
        // The rows pact 0.10.7 actually writes: `context` events carry a null
        // path and detail, lease events carry both, and every row names the
        // worktree it came from. All of them survive the unknown fields
        // (kind, scope, ttl_secs, head, pact_version, chain_hash, ...).
        assert!(ev.events.iter().any(|e| e.path.is_none()));
        assert!(ev
            .events
            .iter()
            .any(|e| e.path.is_some() && e.detail.is_some()));
        assert!(ev.events.iter().all(|e| e.invoked_from.is_some()));
        // 1-based addressing, end to end.
        assert_eq!(ev.line(1).unwrap(), &ev.events[0]);
        assert_eq!(ev.line(ev.line_count).unwrap().line, ev.line_count);
        assert!(ev.line(0).is_err(), "there is no line zero");
        let err = ev.line(ev.line_count + 1).unwrap_err().to_string();
        assert!(
            err.contains(&ev.line_count.to_string()),
            "out of range must name the line count, got {err:?}"
        );
    }

    #[test]
    fn a_truncated_tail_costs_exactly_one_line() {
        let whole = std::fs::read(events_path(Path::new(env!("CARGO_MANIFEST_DIR")))).unwrap();
        let body = whole.strip_suffix(b"\n").unwrap_or(&whole);
        // Chop the last 20 bytes: the final row is now half a JSON object,
        // possibly half a UTF-8 codepoint too. Nothing before it is touched.
        let cut = &body[..body.len() - 20];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, cut).unwrap();

        let ev = read(&path).unwrap();
        let whole_ev = real_ledger();
        assert_eq!(ev.parse_failures, 1, "one bad tail, not a failed file");
        assert_eq!(ev.line_count, ev.events.len() + 1);
        assert_eq!(ev.events, whole_ev.events[..ev.events.len()]);
        assert!(ev
            .line(ev.line_count)
            .unwrap_err()
            .to_string()
            .contains("did not parse"));
    }

    #[test]
    fn bad_lines_still_consume_their_line_number() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"at":"2026-08-19T07:03:12.694912822+00:00","agent":"a","kind":"context","path":null,"detail":null,"chain_hash":"deadbeef","invoked_from":"main","unheard_of_field_from_pact_0_11":42}"#,
                "\n",
                "{ this is not json\n",
                "\n",
                r#"{"at":"nonsense","agent":"a","invoked_from":"main"}"#,
                "\n",
                r#"{"at":"2026-08-19T07:04:00Z","agent":"b","path":"src/x.rs","invoked_from":"wt","detail":"d"}"#,
                "\n",
            ),
        )
        .unwrap();

        let ev = read(&path).unwrap();
        assert_eq!(ev.line_count, 5);
        assert_eq!(ev.parse_failures, 3, "bad json, blank line, bad timestamp");
        assert_eq!(
            ev.events.iter().map(|e| e.line).collect::<Vec<_>>(),
            vec![1, 5],
            "line 5 stays line 5 even though 2, 3 and 4 are junk"
        );
        assert_eq!(ev.line(1).unwrap().agent, "a");
        assert_eq!(ev.line(1).unwrap().path, None);
        assert_eq!(ev.line(5).unwrap().detail.as_deref(), Some("d"));
        for junk in [2, 3, 4] {
            assert!(ev
                .line(junk)
                .unwrap_err()
                .to_string()
                .contains("did not parse"));
        }
        assert!(ev.line(6).unwrap_err().to_string().contains("5"));
    }

    #[test]
    fn a_missing_ledger_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read(&events_path(dir.path())).unwrap(),
            Events::default(),
            "a repo pact never ran in reads as an empty ledger"
        );
        assert_eq!(
            events_path(Path::new("/repo")),
            Path::new("/repo/.pact/events.jsonl")
        );
        // A directory where a file should be is still an error: that is a
        // broken .pact, not an absent one.
        assert!(read(dir.path()).is_err());
    }
}
