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

use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
        todo!(
            "recount-hwk.27: 1-based lookup of line {n} among {} lines",
            self.line_count
        )
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
    todo!("recount-hwk.27: tolerant line-by-line parse of {path:?}, chain_hash ignored")
}
