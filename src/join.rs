//! The join: which session was driving this identity at this moment, and how
//! sure are we.
//!
//! This is the whole point of the tool, so its rules are written down rather
//! than left in the scoring arithmetic.
//!
//! # The key is topological, not textual
//!
//! `invoked_from -> worktree path -> session cwd`. A pact event says which
//! worktree the agent was invoked from; [`crate::worktree`] turns that name
//! into a path; a session states the cwd it ran in. Matching those is
//! structural — it does not depend on the agent having said its own name, on
//! log formatting, or on any heuristic about content. That is what makes a
//! match defensible in an audit.
//!
//! # Three signals, in precedence order
//!
//! 1. **Topology** — the chain above. Without it there is no match to score.
//! 2. **Time overlap** — REQUIRED, not weighted. A session whose turns do not
//!    overlap the event window is not a weak match, it is not a match: it
//!    cannot have produced the event. No overlap short-circuits to
//!    [`Refusal::NoOverlap`] before any score is computed.
//! 3. **Transcript mention** — the tiebreaker only. The PACT_AGENT name or a
//!    bead id appearing in the transcript raises confidence between sessions
//!    that already passed 1 and 2. It never creates a match on its own: an
//!    agent discussing `skeleton` is not `skeleton`.
//!
//! # Never a bare answer
//!
//! [`Match`] cannot be built without a `Confidence` and a non-empty
//! `Vec<Evidence>`, and [`Refusal`] cannot be returned without saying which of
//! the two reasons applies. There is no third outcome and no code path that
//! returns a session with no explanation attached.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Confidence, Evidence, SessionMeta, Turn};
use crate::worktree::Worktree;

/// What we are trying to place in time and space.
///
/// Built from `at`'s flags or, for `explain`, from a
/// [`crate::pact_events::Event`].
#[derive(Debug, Clone)]
pub struct Query<'a> {
    /// The fleet identity, as pact recorded it.
    pub agent: &'a str,
    /// The moment to resolve.
    pub at: DateTime<Utc>,
    /// Half-width of the match window either side of `at`.
    pub window: Duration,
    /// The event's `invoked_from`, when there is one. Its absence weakens the
    /// topological signal rather than disabling the join: the agent name often
    /// equals the worktree name in a pact fleet, and that correspondence is
    /// recorded as evidence rather than assumed silently.
    pub invoked_from: Option<&'a str>,
    /// A bead id to look for in transcripts, for the tiebreaker.
    pub bead: Option<&'a str>,
}

/// A session, with why we believe it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub session: SessionMeta,
    pub confidence: Confidence,
    /// Never empty. See the module doc.
    pub evidence: Vec<Evidence>,
}

/// The two reasons `at` is allowed to give for exit 1. Exactly two, because
/// the spec names exactly two, and an audit reader needs to know which
/// question to ask next: "no cwd match" means look at the topology, "no
/// overlap" means look at the clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// No session's cwd resolved into the repo or the named worktree — or the
    /// worktree it named is gone from disk.
    NoCwdMatch { detail: String },
    /// Sessions matched topologically, but none has a turn inside the window.
    NoOverlap { detail: String },
}

impl Refusal {
    /// The spec's two words, for the digest and for tests that pin them.
    pub fn reason(&self) -> &'static str {
        match self {
            Refusal::NoCwdMatch { .. } => "no cwd match",
            Refusal::NoOverlap { .. } => "no overlap",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Refusal::NoCwdMatch { detail } | Refusal::NoOverlap { detail } => detail,
        }
    }
}

/// Below this, `at` refuses rather than answering. A single named constant
/// because "no-confident-match" in the spec has to mean one thing everywhere.
pub const CONFIDENT: f64 = 0.5;

/// Score every candidate and return the best confident one, or the reason
/// there is none.
///
/// `turns` reads a session's transcript for the mention tiebreaker. It is a
/// callback so the join does not decide which connector owns a path and so
/// transcripts are read lazily — most candidates are eliminated by topology or
/// overlap without ever being opened.
pub fn resolve(
    query: &Query,
    worktrees: &[Worktree],
    sessions: &[SessionMeta],
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Result<Match, Refusal> {
    let _ = turns;
    todo!(
        "recount-hwk.31: score {} sessions against {} worktrees for {query:?}",
        sessions.len(),
        worktrees.len()
    )
}
