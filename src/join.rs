//! The join: which session was driving this identity at this moment, and how
//! sure are we.
//!
//! This is the whole point of the tool, so its rules are written down rather
//! than left in the scoring arithmetic.
//!
//! # Two tiers, and only one of them is scored
//!
//! pact 0.11.0 stamps `harness_session` on the events it writes, plus
//! `harness_subagent` when a subagent wrote them. Either way the event NAMES a
//! transcript: with a subagent id it is that subagent's file, and with a
//! session alone it is the top-level session's own file, sitting beside the
//! `subagents/` directory and exactly as named. Resolving either is a path join
//! and one `is_file()` — no candidate set, no score, no tie to break — so it is
//! not a fifth rung on the ladder below but a tier ABOVE the whole ladder:
//! [`Tier::Keyed`], evidenced by the filename that proved it. Everything from
//! here to [`CONFIDENT`] is the tier BELOW: calibrated inference, unchanged by
//! this, and reached only by an event carrying no chain at all. Neither tier is
//! tuned in terms of the other — one is an identity, the other is a guess.
//!
//! Which tier answered is on every result, and the [`Signal::Keyed`] line says
//! which of three things happened — same tier for the first two, but three
//! different answers to "who acted", so three different sentences:
//!
//! 1. **a subagent id, resolved** — the event named a SUBAGENT transcript, and
//!    the actor is that subagent;
//! 2. **a session with no subagent id, resolved** — the event named a TOP-LEVEL
//!    session transcript, and the actor is the agent that owns the session and
//!    is nobody's subagent;
//! 3. **no chain at all** — a pre-contract row, written before pact stamped
//!    either id. Nothing named a transcript, nothing is missing, and the ladder
//!    below answers. Such a row is OLD, not defective; saying a subagent id was
//!    expected of it would be reporting a field it never had.
//!
//! A chain that names a file which is not on disk is none of the three: it is a
//! refusal ([`Refusal::NoKeyedTranscript`]) and never a quiet demotion to
//! guessing.
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
//! 2. **Time overlap** — REQUIRED, not weighted, and measured on TURNS. A
//!    session whose turns do not fall inside the event window is not a weak
//!    match, it is not a match: it cannot have produced the event. No overlap
//!    short-circuits to [`Refusal::NoOverlap`] before any score is computed.
//!
//!    The primitive is the turn, not the span. A session's
//!    `first_ts..last_ts` straddling the window is NOT overlap: a thirteen-day
//!    session straddles every window anyone will ever query, and awarding the
//!    signal for it hands 0.20 — and with it the bar — to every long-running
//!    session in the repo for every moment in it. The gate is
//!    [`Match::in_window`], the turns actually inside, and it is non-empty on
//!    every topological answer by construction.
//! 3. **Transcript mention** — the tiebreaker. The PACT_AGENT name or a bead
//!    id appearing in the transcript raises confidence between sessions that
//!    already passed 1 and 2. It never creates a match on its own: an agent
//!    discussing `skeleton` is not `skeleton`, and a session the chain
//!    excluded is never scored, however often it says the name.
//!
//! # The chain can filter without discriminating (topology A and topology B)
//!
//! `cwd` is the cwd of the session *process*, and a subagent is not a process
//! (`docs/format-notes.md` §4.3). So a fleet comes in two shapes:
//!
//! * **A, process-per-agent** — each agent is its own `claude` started in
//!   `wt/<name>`; the cwd IS the worktree and the chain resolves exactly as
//!   the spec describes.
//! * **B, subagent-per-agent** — every agent is a subagent of one session, and
//!   every transcript records the parent's cwd. Measured on this repo: three
//!   wave-0 agents worked in three different worktrees, and all 206
//!   cwd-bearing lines of their transcripts say the repo root. The chain then
//!   returns *every* session in the repo for *every* worktree: it still
//!   filters, but it cannot tell the candidates apart.
//!
//! A confident answer produced by a key that could not discriminate is exactly
//! the arkanoid-v1 misdiagnosis this tool was written against — committed by
//! the tool built to prevent it. So [`resolve`] detects the degenerate case
//! ([`Topology`]): when no surviving candidate's cwd names the worktree
//! `invoked_from` named, or when several do and the chain cannot separate
//! them, topology scores as a filter rather than as a discriminator, the
//! mention signal becomes the decisive one, and the resulting confidence is
//! lower. Every answer says which regime it was decided in.
//!
//! # Never a bare answer
//!
//! [`Match`] cannot be built without a [`Tier`] and a non-empty
//! `Vec<Evidence>`, and [`Refusal`] cannot be returned without saying which
//! reason applies. There is no third outcome and no code path that returns a
//! session with no explanation attached.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::connector::claude_code::{self, Keyed};
use crate::model::{Confidence, Evidence, SessionMeta, Signal, Tier, TimeWindow, Turn};
use crate::worktree::{for_cwd, Worktree};

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
    /// The event's `harness_session`: the Claude Code session uuid, which is
    /// the directory a subagent transcript lives under. Half of the exact key.
    pub harness_session: Option<&'a str>,
    /// The event's `harness_subagent`: the id of the transcript the event was
    /// written from. **This field alone gates the keyed tier** — see
    /// [`resolve`]. Its absence is a normal state, not a degraded one.
    pub harness_subagent: Option<&'a str>,
}

/// A session, with which tier answered and why we believe it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub session: SessionMeta,
    /// `keyed`, or `topological` with the score. Flattened, so the wire shape
    /// is the two fields `{"tier": …, "confidence": …}` documented on
    /// [`Tier`] — a keyed answer writes an explicit `null` where a number used
    /// to be, rather than a number nobody computed.
    #[serde(flatten)]
    pub tier: Tier,
    /// Never empty. See the module doc.
    pub evidence: Vec<Evidence>,
    /// The matched session's turns INSIDE the query window — the overlap
    /// signal's own evidence, computed once, here, and handed to whoever
    /// prints it.
    ///
    /// **This field is why the scorer and the renderers cannot drift apart.**
    /// `at` and `testify` used to re-derive "the turns in the window" from the
    /// transcript themselves, which is how the tool came to award a time
    /// overlap and then print "no single turn falls inside it" two lines
    /// below: two conditions, two code paths, one of them wrong. There is now
    /// one condition and one path. On a [`Tier::Topological`] answer this is
    /// non-empty *by construction* — it IS the gate ([`topological`]) — so a
    /// renderer that finds it empty is looking at a keyed answer, which never
    /// consulted the clock and does not depend on it.
    pub in_window: Vec<Turn>,
}

/// The reasons `at` is allowed to give for exit 1. One per question the reader
/// has to ask next, which is why they are not one variant: "no cwd match" means
/// look at the topology, "no overlap" means look at the clock, "no keyed
/// transcript" means look at the transcript store — or at which machine you are
/// standing on.
///
/// The spec named the first two. The third arrived with the keyed tier and
/// belongs to it exclusively — it is reachable only for an event carrying a
/// `harness_subagent`. Collapsing it into [`Refusal::NoCwdMatch`] would send an
/// auditor to look at cwds when no cwd was ever consulted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// A list of refusals is a list of negatives, so of course they share a prefix.
// The alternative the lint suggests is renaming the spec's own two words to
// suit a naming heuristic.
#[expect(
    clippy::enum_variant_names,
    reason = "every refusal is a `no <thing>`; that is the domain, not a naming slip"
)]
pub enum Refusal {
    /// No session's cwd resolved into the repo or the named worktree — or the
    /// worktree it named is gone from disk.
    NoCwdMatch { detail: String },
    /// Sessions matched topologically, but none has a turn inside the window.
    NoOverlap { detail: String },
    /// The event named its transcript and the transcript is not there: absent
    /// from disk, or the id is not an id. Keyed events only.
    NoKeyedTranscript { detail: String },
}

impl Refusal {
    /// The words the digest prints, and that tests pin.
    pub fn reason(&self) -> &'static str {
        match self {
            Refusal::NoCwdMatch { .. } => "no cwd match",
            Refusal::NoOverlap { .. } => "no overlap",
            Refusal::NoKeyedTranscript { .. } => "no keyed transcript",
        }
    }

    pub fn detail(&self) -> &str {
        match self {
            Refusal::NoCwdMatch { detail }
            | Refusal::NoOverlap { detail }
            | Refusal::NoKeyedTranscript { detail } => detail,
        }
    }
}

/// Below this, `at` refuses rather than answering. A single named constant
/// because "no-confident-match" in the spec has to mean one thing everywhere.
///
/// Apply it with [`Tier::clears`], never by reaching for a number: a keyed
/// answer has no score, clears every bar, and a caller that re-derives the
/// comparison drops an identity for failing a test it was never on. The keyed
/// tier moved neither this constant nor the four rungs beneath it.
pub const CONFIDENT: f64 = 0.5;

/// What each signal is worth. Four fixed numbers, not a weight table: the spec
/// names three signals, so there are three, and a configurable scorer is a
/// scorer nobody can reproduce in an audit six months later.
///
/// The arithmetic is a sum, and the numbers are chosen so that the *regime* is
/// legible from the score alone:
///
/// | outcome                                            | score |
/// |----------------------------------------------------|------:|
/// | chain singled the session out, + mention (topology A) | 0.95 |
/// | chain singled it out, no mention                     | 0.70 |
/// | chain only filtered, + mention (topology B)          | 0.65 |
/// | chain only filtered, no mention                      | 0.40 |
///
/// The bottom row is below [`CONFIDENT`] on purpose: under topology B a
/// repo-level cwd plus a clock overlap is true of every agent in the fleet,
/// and answering from it would be the misdiagnosis in the module doc. The
/// third row clears the bar but is visibly not the second: a mention is
/// testimony, not topology.
const TOPOLOGY_DISCRIMINATING: f64 = 0.5;
const TOPOLOGY_FILTER: f64 = 0.2;
const OVERLAP: f64 = 0.2;
const MENTION: f64 = 0.25;

/// Which regime the chain turned out to be in, decided from the data rather
/// than configured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Topology {
    /// Exactly one candidate's cwd names the worktree `invoked_from` named:
    /// the chain picked a session out of the field. This is the spec's
    /// process-per-agent case, and the only one where topology discriminates.
    ProcessPerAgent,
    /// Several candidates name it and the chain cannot separate them.
    Ambiguous(usize),
    /// No candidate names it — every candidate matched at an ancestor (repo)
    /// level only. The subagent-per-agent case.
    SubagentPerAgent,
    /// Nothing named a worktree: no `invoked_from` and no worktree called
    /// after the agent. The chain filtered to "sessions in this repo".
    Unnamed,
}

impl Topology {
    fn discriminates(self) -> bool {
        self == Topology::ProcessPerAgent
    }

    /// The sentence that goes in the evidence, so a human reading the output
    /// knows which regime produced the number.
    fn observed(self) -> &'static str {
        match self {
            Topology::ProcessPerAgent => {
                "topology A (process-per-agent): the session cwd names the worktree, \
                 so the chain discriminates"
            }
            Topology::Ambiguous(_) => {
                "topology A (process-per-agent) but ambiguous: several candidate sessions \
                 name the same worktree, so the chain filtered without discriminating and \
                 the transcript mention decides"
            }
            Topology::SubagentPerAgent => {
                "topology B (subagent-per-agent): no candidate cwd names the worktree, \
                 every candidate sits at the shared parent cwd, so the chain filtered \
                 without discriminating and the transcript mention decides"
            }
            Topology::Unnamed => {
                "topology unnamed: nothing tied the agent to a worktree, so the chain \
                 filtered to the repo only and the transcript mention decides"
            }
        }
    }
}

/// A candidate that cleared BOTH gates, carrying everything its one transcript
/// read produced: the turns inside the window (which is what clearing the
/// overlap gate means, and what the digest prints) and the mention, if the
/// tiebreaker found one.
struct Survivor<'a> {
    c: Candidate<'a>,
    in_window: Vec<Turn>,
    mention: Option<String>,
}

/// One session that survived the chain, and how well it survived it.
struct Candidate<'a> {
    session: &'a SessionMeta,
    /// The worktree its cwd resolves to — the named one, or an ancestor of it.
    resolved: &'a Worktree,
    /// Its cwd names the queried worktree itself, not merely an ancestor.
    exact: bool,
}

/// The two tiers, in order: keyed if the event named a transcript, the scored
/// ladder if it named none.
///
/// **The gate is the chain, and `harness_session` alone is enough of one.** A
/// subagent id names `<session>/subagents/agent-<id>.jsonl`; a session with no
/// subagent id names `<session>.jsonl` beside that directory, which is just as
/// much a name and just as much on disk. That row was written by a TOP-LEVEL
/// agent — one that has a session and is not a subagent of anything — and
/// keying it is the same join and the same single stat. Measured on this repo's
/// live ledger: 216 rows, 39 carry any chain, 13 of those are session-only (12
/// orchestrator, 1 precheck), and those 13 are what Phase 3's self-audit runs
/// against. The gate is still not `harness.is_some()` or `model.is_some()`:
/// those name no file, and a gate on them would refuse where there is nothing
/// to go and look for.
///
/// **This does not moot `recount-gwx`.** A v0.1.0-era row carries no
/// `harness_session` either — line 60 of this repo's own ledger is an
/// `orchestrator` `acquired` with both ids null — so pre-contract rows have
/// nothing to key by, are untouched by this, and still reach the ladder with
/// the path-order tie-break `gwx` is about waiting for them.
///
/// `turns` reads a session's transcript. It is a callback so the join does not
/// decide which connector owns a path, and so transcripts are read lazily —
/// most candidates are eliminated by topology, or by a span that cannot hold a
/// turn in the window, without ever being opened. One read now serves three
/// purposes for a candidate that survives: the overlap gate, the mention
/// tiebreaker, and [`Match::in_window`] for whoever prints the answer.
///
/// The keyed tier reads only the WINNER's transcript, and only to fill
/// [`Match::in_window`] — the read `at` and `testify` were each doing
/// themselves a moment later. Nothing in a transcript can raise or lower an
/// identity; the keyed tier still has no gate, no score and no tie to break.
pub fn resolve(
    query: &Query,
    worktrees: &[Worktree],
    sessions: &[SessionMeta],
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Result<Match, Refusal> {
    let window = TimeWindow::around(query.at, query.window);
    let keyed_in_window = |m: Match| {
        let in_window = inside(&window, &turns(&m.session.path));
        Match { in_window, ..m }
    };
    match (query.harness_subagent, query.harness_session) {
        // A subagent id wins where both are present: it names the narrower
        // file, and the parent session is not who wrote the event.
        (Some(subagent), _) => {
            keyed(query, Some(subagent), worktrees, sessions).map(keyed_in_window)
        }
        (None, Some(_)) => keyed(query, None, worktrees, sessions).map(keyed_in_window),
        (None, None) => topological(query, worktrees, sessions, turns, &window),
    }
}

/// The one definition of "inside the window" in the codebase. Everything that
/// asks the question — the overlap gate, the keyed answer's transcript, both
/// digests — asks it here.
fn inside(window: &TimeWindow, turns: &[Turn]) -> Vec<Turn> {
    turns
        .iter()
        .filter(|t| window.contains(t.ts))
        .cloned()
        .collect()
}

/// The keyed tier: the event names the transcript, so resolution is a path
/// join and one `stat` per session directory this repo has.
///
/// Not a search. [`claude_code::keyed`] validates the two ids as opaque
/// `[A-Za-z0-9-]` tokens (so a hostile ledger cannot walk out of the project
/// directory), joins the path and stats it. Several directories are tried
/// because a repo and its worktrees encode to different ones and the event does
/// not say which — a handful of stats, not a walk, first hit wins.
///
/// **The ceiling, and why it is not a hole.** 371 of 634 subagent transcripts on
/// the machine this was measured on live one level down, in
/// `subagents/workflows/wf_<id>/`, which this tier does not look in. It does not
/// have to: no pact event carries a `wf_` id, because the only recipe by which
/// an agent learns its own id (`docs/format-notes.md` §10.3) globs the flat
/// directory, so any id that reached an event came from a flat file by
/// construction. Those 371 belong to the topological ladder and always did. Do
/// not "fix" this by scanning `workflows/`: a scan is what this tier exists not
/// to do, and it would hand the tie-break bug an identity it cannot touch.
fn keyed(
    query: &Query,
    subagent: Option<&str>,
    worktrees: &[Worktree],
    sessions: &[SessionMeta],
) -> Result<Match, Refusal> {
    // Half a key is a malformed identity, not a missing file: the path is
    // <project dir>/<harness_session>/subagents/agent-<harness_subagent>.jsonl,
    // so an absent session id names nothing, and `claude_code::keyed` already
    // rejects an empty token. One code path says it once.
    let session = query.harness_session.unwrap_or_default();

    let mut looked: Vec<PathBuf> = Vec::new();
    for w in worktrees {
        match claude_code::keyed(&w.session_dir, session, subagent) {
            Keyed::Found(path) => return keyed_match(path, session, subagent, sessions),
            Keyed::Absent(path) => looked.push(path),
            // Decided by the ids alone, so the first directory settles it for
            // every directory: this is a bad id, not a missing file, and
            // nothing was stat'd.
            Keyed::Malformed(why) => {
                return Err(Refusal::NoKeyedTranscript {
                    detail: format!(
                        "the event's keyed identity is not usable as one: {why}. No path was \
                         joined and nothing was stat'd, so this is not a transcript that has \
                         gone missing — it is a defect in the event that names it, and the \
                         thing to look at is the ledger, not the disk"
                    ),
                })
            }
        }
    }

    // Both causes below hold for either kind of file — a file that is not here
    // is either gone or was never here. Which to bet on differs, and only the
    // top-level case needs telling: the worktree-tidying half of the first
    // cause is a subagent's story, so it gets a clause of its own rather than a
    // sentence that has quietly become wrong for it.
    let tidying = match subagent {
        Some(_) => "",
        None => {
            " Weigh that first cause differently here, though: a top-level session ran in \
                 a directory that OUTLIVES THE RUN, not in a worktree torn down at the end of \
                 it, so retention or a deletion is the likely half and tidying is not."
        }
    };
    Err(Refusal::NoKeyedTranscript {
        detail: format!(
            "the event names {}, and it is not on disk: looked at {}. Two things do this and \
             they are not the same news. Either the transcript was PRUNED — Claude Code keeps \
             30 days, and a fleet that tidies its worktrees as it goes takes their encoded \
             session directories with them — in which case look elsewhere on this machine and \
             at your backups. Or this LEDGER CAME FROM ANOTHER MACHINE, in which case the \
             transcript was never here, no amount of looking will find it, and this question \
             cannot be answered on this host at all.{tidying}",
            named(session, subagent),
            paths(&looked)
        ),
    })
}

/// The transcript is on disk. Turn it into an answer, or say why the sessions
/// the join was offered do not contain it.
fn keyed_match(
    path: PathBuf,
    session: &str,
    subagent: Option<&str>,
    sessions: &[SessionMeta],
) -> Result<Match, Refusal> {
    // Practically unreachable: a transcript being written when the event fired
    // has an mtime at or after it, so the prune cannot drop it (`crate::prune`).
    // Not impossible though, and an identity silently downgraded to a guess is
    // the one outcome this tier exists to prevent.
    let Some(meta) = sessions.iter().find(|s| s.path == path) else {
        return Err(Refusal::NoKeyedTranscript {
            detail: format!(
                "the event names {}, which IS on disk at {}, but it is \
                 not among the {} \
                 session(s) the join was offered — no connector read it. The file is there; \
                 what is missing is a reader for it, so look at the file, not for it",
                named(session, subagent),
                path.display(),
                sessions.len()
            ),
        });
    };
    // Same tier, two sentences. A reader has to be able to tell "a subagent
    // wrote this" from "the agent that owns the session wrote this": both are
    // identities, but they answer different questions about who acted.
    let detail = match subagent {
        Some(id) => format!(
            "the event named its own transcript: harness_subagent `{id}` under \
             harness_session `{session}` is {}, found at {} by one path join and one \
             stat. No candidate set, no score, no tie to break — this is an identity, and \
             the topological ladder was not consulted",
            file_name(meta),
            path.display()
        ),
        None => format!(
            "the event named a TOP-LEVEL session transcript, not a subagent's: \
             harness_session `{session}` with no harness_subagent is {}, the session's own \
             file beside the subagents/ directory, found at {} by one path join and one \
             stat. Whoever wrote this event owns that session and is nobody's subagent. No \
             candidate set, no score, no tie to break — this is an identity, and the \
             topological ladder was not consulted",
            file_name(meta),
            path.display()
        ),
    };
    Ok(Match {
        session: meta.clone(),
        tier: Tier::Keyed,
        evidence: vec![Evidence::new(Signal::Keyed, detail)],
        // Filled by `resolve`, which owns the window and the reader. A keyed
        // identity does not depend on it: it is what the session was SAYING
        // around the moment, not what proves it was this session.
        in_window: Vec::new(),
    })
}

/// What the event named, in the words the refusals have to use. The two kinds
/// of keyed identity name different files, so a sentence that says only
/// "agent-<id>.jsonl" is wrong for half of them.
fn named(session: &str, subagent: Option<&str>) -> String {
    match subagent {
        Some(id) => format!("subagent transcript agent-{id}.jsonl under harness_session {session}"),
        None => format!("the top-level session transcript {session}.jsonl"),
    }
}

/// Why the keyed tier did not answer, on every answer the ladder gives.
///
/// One sentence, because there is exactly one way to get here: an event with
/// EITHER id resolves keyed or refuses, so reaching the ladder means the event
/// carried no chain at all. It must not say "no `harness_subagent`" any more —
/// that was true when a session with no subagent id fell through here, and
/// since it no longer does, saying it would report a missing field to the
/// reader of a row that never had one to miss.
const NO_CHAIN: &str =
    "no harness chain on this event at all — neither `harness_session` nor `harness_subagent` \
     — so nothing named a transcript to key it to. This is a PRE-CONTRACT row, written before \
     pact stamped the attribution chain: it is OLD, not defective, no id is missing from it, \
     and there is nothing to go and look for. The answer below is INFERRED by the topological \
     ladder, not keyed";

/// Score every candidate and return the best one, or the reason there is none.
///
/// The returned [`Match`] is the best candidate, not necessarily a confident
/// one: the threshold is [`CONFIDENT`] and applying it is `at`'s job, because
/// a low-confidence match still has to be *printed* — its evidence is what
/// tells the auditor whether to go and look at the clock or at the topology.
///
/// Unchanged by the keyed tier above it: the four rungs, their arithmetic and
/// [`CONFIDENT`] are exactly what they were.
fn topological(
    query: &Query,
    worktrees: &[Worktree],
    sessions: &[SessionMeta],
    turns: &dyn Fn(&Path) -> Vec<Turn>,
    window: &TimeWindow,
) -> Result<Match, Refusal> {
    // 1. invoked_from -> worktree.
    let (named, chain) = name_worktree(query, worktrees)?;

    // 2. worktree path -> session cwd. A cwd that resolves to the named
    //    worktree is an exact link; one that resolves to an ancestor of it is
    //    the repo-level match topology B leaves behind — kept, because it is
    //    still a correct filter, and flagged, because it is not a
    //    discriminator. A cwd in a sibling worktree is excluded outright.
    let mut candidates = Vec::new();
    let (mut no_cwd, mut elsewhere) = (0usize, 0usize);
    for session in sessions {
        let Some(cwd) = session.cwd.as_deref() else {
            no_cwd += 1;
            continue;
        };
        let Some(resolved) = for_cwd(worktrees, cwd) else {
            elsewhere += 1;
            continue;
        };
        let exact = named.is_some_and(|n| n.path == resolved.path);
        let inside = named.is_none_or(|n| exact || n.path.starts_with(&resolved.path));
        if inside {
            candidates.push(Candidate {
                session,
                resolved,
                exact,
            });
        } else {
            elsewhere += 1;
        }
    }
    if candidates.is_empty() {
        return Err(Refusal::NoCwdMatch {
            detail: format!(
                "{chain}, but none of the {} session(s) offered resolves into it \
                 ({elsewhere} resolve elsewhere, {no_cwd} state no cwd at all)",
                sessions.len()
            ),
        });
    }

    // 3. Time overlap, the one hard gate, and it is a gate on TURNS. A span
    //    that straddles the window proves nothing — a session that ran for
    //    thirteen days straddles every window there is — so the question asked
    //    of each candidate is whether any of its turns is actually inside.
    //    A session with no timestamp at all (an empty or wholly unparseable
    //    file) has no turn anywhere and so has none here: unbounded would mean
    //    "always", the opposite of what an absent clock tells us.
    //
    //    The span is still consulted, as a cheap NECESSARY condition: every
    //    turn lies within `first_ts..last_ts` by construction, so a span that
    //    misses the window cannot hold a turn inside it and that file is never
    //    opened. That is an optimisation on the read, not a second definition
    //    of overlap — the decision below is the turn list in both branches.
    let mut overlapping: Vec<Survivor> = Vec::new();
    let mut missed: Vec<(Candidate, bool)> = Vec::new();
    for c in candidates {
        let span = c.session.window();
        let could = !span.is_unbounded() && span.overlaps(window);
        let read = if could {
            turns(&c.session.path)
        } else {
            Vec::new()
        };
        // One read, three answers: the gate, the tiebreaker, and the turns the
        // digest prints. The transcript is dropped at the end of this
        // iteration — only the window's worth of it is kept.
        let in_window = inside(window, &read);
        if in_window.is_empty() {
            missed.push((c, could));
            continue;
        }
        let mention = mention(query, &read);
        overlapping.push(Survivor {
            c,
            in_window,
            mention,
        });
    }
    if overlapping.is_empty() {
        return Err(Refusal::NoOverlap {
            detail: format!(
                "{} session(s) matched {chain}, but none has a turn in {}: {}",
                missed.len(),
                fmt_window(window),
                missed
                    .iter()
                    .map(|(c, straddled)| format!(
                        "{} covers {}{}",
                        file_name(c.session),
                        fmt_window(&c.session.window()),
                        // The distinction the auditor needs: a span that
                        // never reached the window is a different fact from
                        // one that straddles it and was silent through it,
                        // and only the first is fixed by widening --window.
                        if *straddled {
                            " — its span straddles the window and not one of its turns falls inside it"
                        } else {
                            ""
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }

    // 4. Did the chain actually single anyone out? One exact link and the
    //    answer is topological; anything else and topology has only filtered,
    //    whatever the spec's happy path assumes.
    let exact = overlapping.iter().filter(|s| s.c.exact).count();
    let topology = match (named, exact) {
        (None, _) => Topology::Unnamed,
        (Some(_), 0) => Topology::SubagentPerAgent,
        (Some(_), 1) => Topology::ProcessPerAgent,
        (Some(_), n) => Topology::Ambiguous(n),
    };

    // 5. Score. Ties keep the first candidate offered, so the same inputs
    //    always give the same answer — and recount-gwx is still open on
    //    exactly that: candidates arrive in path order, which puts a
    //    subagent transcript ahead of its parent, so an orchestrator event
    //    systematically loses its own session. NEITHER HALF of the keyed tier
    //    fixes this, and that includes the parent-keyed half added for exactly
    //    that shape of row. Pre-contract rows carry no session id and are
    //    untouched by it: every event gwx was measured on comes from the
    //    v0.1.0 run — line 60 of this repo's ledger is one of them, an
    //    orchestrator `acquired` with harness_session AND harness_subagent
    //    both null — so there is nothing to key them by, they arrive here
    //    unchanged, and they meet the path-order tie-break intact. What
    //    parent-keying bought is this run's 13 chained session-only rows, not
    //    gwx's 56. recount-gwx stays open.
    let mut best: Option<(f64, Match)> = None;
    for s in overlapping {
        let (score, scored) = score(query, s, topology, window);
        if best.as_ref().is_none_or(|(b, _)| score > *b) {
            best = Some((score, scored));
        }
    }
    Ok(best.expect("overlapping is non-empty").1)
}

/// `invoked_from -> worktree`, the first link, plus the phrase describing it
/// that every downstream message reuses.
fn name_worktree<'a>(
    query: &Query,
    worktrees: &'a [Worktree],
) -> Result<(Option<&'a Worktree>, String), Refusal> {
    let (named, chain) = match query.invoked_from {
        Some(name) => match worktrees.iter().find(|w| w.name == name) {
            Some(w) => (
                Some(w),
                format!("invoked_from `{name}` -> {}", w.path.display()),
            ),
            None => {
                return Err(Refusal::NoCwdMatch {
                    detail: format!(
                        "invoked_from `{name}` names no worktree of this repo (known: {})",
                        names(worktrees)
                    ),
                })
            }
        },
        // The absence of invoked_from weakens the chain rather than disabling
        // it: in a pact fleet the worktree is usually named after the agent.
        // That correspondence is stated as evidence, never assumed silently.
        None => match worktrees.iter().find(|w| w.name == query.agent) {
            Some(w) => (
                Some(w),
                format!(
                    "no invoked_from; agent `{}` names worktree {}",
                    query.agent,
                    w.path.display()
                ),
            ),
            None => (
                None,
                format!(
                    "no invoked_from and no worktree named after agent `{}` (known: {}); \
                     candidates are every session in the repo",
                    query.agent,
                    names(worktrees)
                ),
            ),
        },
    };
    // A worktree git still lists but nobody can look at is not a match, it is
    // the thing the auditor needs told: the spec's pruned-worktree refusal.
    if let Some(w) = named {
        if !w.alive {
            return Err(Refusal::NoCwdMatch {
                detail: format!(
                    "worktree `{}` ({}) is gone from disk — pruned or deleted, \
                     so no session can be placed in it",
                    w.name,
                    w.path.display()
                ),
            });
        }
    }
    Ok((named, chain))
}

/// One candidate's score and the evidence that produced it. Both come out of
/// the same loop because a score without its reason is the bare answer the
/// spec forbids.
fn score(query: &Query, s: Survivor, topology: Topology, window: &TimeWindow) -> (f64, Match) {
    let Survivor {
        c,
        in_window,
        mention,
    } = s;
    let mut evidence = Vec::new();
    let mut total = 0.0;

    let topological = topology.discriminates() && c.exact;
    total += if topological {
        TOPOLOGY_DISCRIMINATING
    } else {
        TOPOLOGY_FILTER
    };
    let link = if c.exact {
        format!(
            "session cwd {} is worktree `{}`",
            display_cwd(c.session),
            c.resolved.name
        )
    } else {
        format!(
            "session cwd {} resolves to worktree `{}`, an ancestor: repo-level match only",
            display_cwd(c.session),
            c.resolved.name
        )
    };
    evidence.push(Evidence::new(
        Signal::Topology,
        format!("{link} — {}", topology.observed()),
    ));

    // The signal, and the count that IS the signal. The span is reported second
    // and as context only: it is what the old scorer awarded on, it is true of
    // every long session, and it is not what earned the 0.20 here.
    total += OVERLAP;
    evidence.push(Evidence::new(
        Signal::TimeOverlap,
        format!(
            "{} turn(s) of this session fall inside the queried {} (the session covers {})",
            in_window.len(),
            fmt_window(window),
            fmt_window(&c.session.window()),
        ),
    ));

    match &mention {
        Some(found) => {
            total += MENTION;
            evidence.push(Evidence::new(Signal::TranscriptMention, found.clone()));
        }
        // Silence is evidence too when the mention is what was deciding. Say
        // so, or the low confidence looks like a rounding error.
        None if !topological => evidence.push(Evidence::new(
            Signal::TranscriptMention,
            format!(
                "no mention of {} in the transcript, and topology could not \
                 discriminate: nothing distinguishes this session from its siblings",
                needles(query)
            ),
        )),
        None => {}
    }

    // Last, and on every scored answer: which tier this is and why it is not
    // the higher one. A reader who cannot tell an inferred answer from one that
    // had an exact key available is back to reading a bare number.
    evidence.push(Evidence::new(Signal::Keyed, NO_CHAIN));

    (
        total,
        Match {
            session: c.session.clone(),
            tier: Tier::Topological(Confidence::new(total)),
            evidence,
            in_window,
        },
    )
}

/// The PACT_AGENT name or the bead id, in the session's own words.
fn mention(query: &Query, turns: &[Turn]) -> Option<String> {
    for turn in turns {
        for needle in [Some(query.agent), query.bead].into_iter().flatten() {
            if let Some(at) = find_word(&turn.text, needle) {
                // `at` indexes the lowercased text. Lowercasing is
                // length-preserving for everything but a handful of non-ASCII
                // characters; where it is not, the snippet comes from the
                // lowercased copy rather than being dropped or sliced wrong.
                let lower = turn.text.to_lowercase();
                let src = if lower.len() == turn.text.len() {
                    turn.text.as_str()
                } else {
                    lower.as_str()
                };
                return Some(format!(
                    "`{needle}` at line {} ({}): {}",
                    turn.line,
                    turn.role,
                    snippet(src, at, needle.to_lowercase().len())
                ));
            }
        }
    }
    None
}

/// Case-insensitive, on word boundaries. `join` is mentioned by `wt/join` and
/// by `PACT_AGENT=join`, but not by `joined` — a substring match here would be
/// the loose inference the whole tool exists to argue against, and under
/// topology B this signal is the deciding one.
///
/// The returned offset is into the *lowercased* text; see [`mention`].
fn find_word(text: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let hay = text.to_lowercase();
    let needle = needle.to_lowercase();
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    hay.match_indices(&needle)
        .find(|(i, m)| {
            !hay[..*i].chars().next_back().is_some_and(word)
                && !hay[*i + m.len()..].chars().next().is_some_and(word)
        })
        .map(|(i, _)| i)
}

/// About thirty characters either side of the hit, on one line: enough context
/// for a human to judge the mention without pasting a turn into the digest.
fn snippet(text: &str, at: usize, len: usize) -> String {
    let lo = (0..=at)
        .rev()
        .find(|i| at - i >= 30 && text.is_char_boundary(*i))
        .unwrap_or(0);
    let end = at + len;
    let hi = (end..=text.len())
        .find(|i| i - end >= 30 && text.is_char_boundary(*i))
        .unwrap_or(text.len());
    let body = text[lo..hi]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}{body}{}",
        if lo > 0 { "…" } else { "" },
        if hi < text.len() { "…" } else { "" }
    )
}

fn needles(query: &Query) -> String {
    match query.bead {
        Some(bead) => format!("`{}` or `{bead}`", query.agent),
        None => format!("`{}`", query.agent),
    }
}

/// The paths the keyed lookup stat'd, for the refusal that must name them.
fn paths(looked: &[PathBuf]) -> String {
    match looked.is_empty() {
        true => "nowhere: this repo has no worktree to encode a session directory from".into(),
        false => looked
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn names(worktrees: &[Worktree]) -> String {
    if worktrees.is_empty() {
        return "none".to_string();
    }
    worktrees
        .iter()
        .map(|w| format!("`{}`", w.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn file_name(session: &SessionMeta) -> String {
    session
        .path
        .file_name()
        .unwrap_or(session.path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn display_cwd(session: &SessionMeta) -> String {
    session
        .cwd
        .as_ref()
        .map_or_else(|| "(none)".to_string(), |c| c.display().to_string())
}

fn fmt_window(w: &TimeWindow) -> String {
    let edge = |t: Option<DateTime<Utc>>, unbounded: &str| {
        t.map_or_else(
            || unbounded.to_string(),
            |t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        )
    };
    format!("{}..{}", edge(w.since, "-∞"), edge(w.until, "+∞"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use crate::model::{parse_ts, Role};

    const REPO: &str = "/r";
    /// A well-formed pair of harness ids, in the shapes pact 0.11.0 stamps.
    const SESSION: &str = "11111111-2222-3333-4444-555555555555";
    const SUB: &str = "aaadf026f08b25971";

    fn ts(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    /// The fleet's worktrees: the repo root plus `wt/<name>` for each agent.
    fn fleet(names: &[&str]) -> Vec<Worktree> {
        let mut out = vec![wt(REPO, "main", true)];
        out.extend(names.iter().map(|n| wt(&format!("{REPO}/wt/{n}"), n, true)));
        out
    }

    fn wt(path: &str, name: &str, alive: bool) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive,
            session_dir: PathBuf::from("/home/u/.claude/projects").join(name),
        }
    }

    fn session(file: &str, cwd: Option<&str>, first: &str, last: &str) -> SessionMeta {
        SessionMeta {
            path: PathBuf::from(format!("/home/u/.claude/projects/p/{file}.jsonl")),
            cwd: cwd.map(PathBuf::from),
            first_ts: Some(ts(first)),
            last_ts: Some(ts(last)),
            turn_count: 2,
            parse_failures: 0,
            declined: 0,
            connector: "claude_code".into(),
        }
    }

    fn query<'a>(agent: &'a str, invoked_from: Option<&'a str>) -> Query<'a> {
        Query {
            agent,
            at: ts("2026-08-19T07:30:00Z"),
            window: Duration::minutes(5),
            invoked_from,
            bead: Some("recount-hwk.31"),
            harness_session: None,
            harness_subagent: None,
        }
    }

    /// Transcripts by file stem. Every session gets ONE turn, at the queried
    /// moment, because the overlap gate is a gate on turns: a fixture with no
    /// turn in the window is a fixture that refuses, whatever its span says.
    /// Anything not listed says nothing, which is how a session with no
    /// mention is spelled.
    fn transcripts(by_stem: &[(&str, &str)]) -> impl Fn(&Path) -> Vec<Turn> {
        let map: HashMap<String, String> = by_stem
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |p: &Path| {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            vec![turn_at(
                "2026-08-19T07:30:00Z",
                map.get(stem.as_ref()).map_or("", String::as_str),
            )]
        }
    }

    fn turn_at(at: &str, text: &str) -> Turn {
        Turn {
            ts: ts(at),
            role: Role::User,
            text: text.to_string(),
            line: 1,
        }
    }

    /// A session that is present in the window and names nobody: one turn at
    /// the queried moment, saying nothing. NOT an empty file — see
    /// [`no_turns`], which is a different fixture asserting a different thing.
    fn silent(_: &Path) -> Vec<Turn> {
        vec![turn_at("2026-08-19T07:30:00Z", "")]
    }

    /// A file with nothing in it, or nothing this connector could read.
    fn no_turns(_: &Path) -> Vec<Turn> {
        Vec::new()
    }

    /// What `resolve` takes, named so an array of them is not "a very complex
    /// type".
    type Reader = dyn Fn(&Path) -> Vec<Turn>;

    /// The score behind a topological answer, for the tests that pin the
    /// arithmetic. Panics on a keyed one — which is the point: there is no
    /// number to compare a keyed answer against.
    fn score(m: &Match) -> f64 {
        m.tier
            .confidence()
            .expect("a topological answer carries a score")
            .value()
    }

    fn signals(m: &Match) -> Vec<Signal> {
        m.evidence.iter().map(|e| e.signal).collect()
    }

    /// Signal 1 alone: process-per-agent, where the spec's chain works. The
    /// cwd names the worktree, nothing mentions anyone, and that is enough.
    #[test]
    fn topology_alone_answers_when_the_cwd_names_the_worktree() {
        let wts = fleet(&["join", "skeleton"]);
        let sessions = vec![
            session(
                "a",
                Some("/r/wt/join/src"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "b",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            // A sibling worktree: excluded by topology, not scored low.
            session(
                "c",
                Some("/r/wt/skeleton"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
        ];

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap();
        assert_eq!(m.session.path, sessions[0].path);
        assert_eq!(score(&m), TOPOLOGY_DISCRIMINATING + OVERLAP);
        assert!(score(&m) >= CONFIDENT);
        assert_eq!(
            signals(&m),
            [Signal::Topology, Signal::TimeOverlap, Signal::Keyed]
        );
        // Even the cleanest topological answer says it is not the keyed one.
        assert!(m.evidence[2].detail.starts_with("no harness chain"));
        assert!(m.evidence[0]
            .detail
            .contains("topology A (process-per-agent)"));
        assert!(m.evidence[0].detail.contains("is worktree `join`"));
    }

    /// The bead: a session whose SPAN straddles the window but whose turns are
    /// all outside it does not overlap. A thirteen-day session straddles every
    /// window anyone will ever query, and on the old span test it collected
    /// 0.20 — and with it the bar — for every moment in those thirteen days.
    ///
    /// Same fleet, same chain, same span: the only thing that moves is whether
    /// a turn is inside, and it decides between an answer and a refusal.
    #[test]
    fn a_span_that_straddles_the_window_is_not_overlap_a_turn_inside_it_is() {
        let wts = fleet(&["join"]);
        // 07:00..08:00 straddles the queried 07:25..07:35 completely.
        let sessions = vec![session(
            "long",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];
        let q = query("join", Some("join"));

        // Turns either side of the window and none in it: not a weak match, a
        // refusal, and the reason says which of the two shapes it is so the
        // reader is not sent to widen a window that is already wide enough.
        let parked = |_: &Path| {
            vec![
                turn_at("2026-08-19T07:05:00Z", "before"),
                turn_at("2026-08-19T07:55:00Z", "after"),
            ]
        };
        let err = resolve(&q, &wts, &sessions, &parked).unwrap_err();
        assert_eq!(err.reason(), "no overlap", "{}", err.detail());
        assert!(
            err.detail().contains("straddles the window"),
            "{}",
            err.detail()
        );

        // A file with nothing in it is the same answer for the same reason.
        assert_eq!(
            resolve(&q, &wts, &sessions, &no_turns)
                .unwrap_err()
                .reason(),
            "no overlap"
        );

        // One turn inside, everything else held: unchanged, and the top rung.
        let spoke = |_: &Path| {
            vec![
                turn_at("2026-08-19T07:05:00Z", "before"),
                turn_at("2026-08-19T07:30:00Z", "join is working on recount-hwk.31"),
                turn_at("2026-08-19T07:55:00Z", "after"),
            ]
        };
        let m = resolve(&q, &wts, &sessions, &spoke).unwrap();
        assert_eq!(
            score(&m),
            TOPOLOGY_DISCRIMINATING + OVERLAP + MENTION,
            "a turn inside scores exactly what it always did"
        );
        // The signal is now stated as the count that earned it, and the
        // in-window turns travel with the answer.
        assert_eq!(m.in_window.len(), 1);
        assert_eq!(m.in_window[0].ts, ts("2026-08-19T07:30:00Z"));
        assert!(
            m.evidence[1]
                .detail
                .starts_with("1 turn(s) of this session"),
            "{}",
            m.evidence[1].detail
        );
    }

    /// The anti-drift pin, at the layer the bug lived in: the overlap signal
    /// and the turns a renderer prints are ONE decision, not two conditions
    /// that agree today. `Match::in_window` is the gate, so it is impossible
    /// for a scored answer to carry a time-overlap signal and hand a renderer
    /// nothing to show.
    #[test]
    fn a_scored_answer_can_never_award_overlap_with_an_empty_window() {
        let wts = fleet(&["join", "skeleton"]);
        let sessions = vec![
            session(
                "a",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "b",
                Some("/r"),
                "2026-08-19T00:00:00Z",
                "2026-08-19T23:59:00Z",
            ),
        ];
        // Every shape of transcript, including the ones that used to slip
        // through: silent, empty, all-outside, one-inside, many-inside.
        let readers: [&Reader; 4] = [
            &no_turns,
            &silent,
            &|_| vec![turn_at("2026-08-19T06:00:00Z", "long before")],
            &|_| {
                vec![
                    turn_at("2026-08-19T07:29:00Z", "in"),
                    turn_at("2026-08-19T07:31:00Z", "also in"),
                ]
            },
        ];
        for (i, turns) in readers.iter().enumerate() {
            let Ok(m) = resolve(&query("join", Some("join")), &wts, &sessions, *turns) else {
                continue;
            };
            let awarded = m.evidence.iter().any(|e| e.signal == Signal::TimeOverlap);
            assert!(
                awarded && !m.in_window.is_empty(),
                "reader {i}: overlap awarded={awarded} but {} turn(s) in the window",
                m.in_window.len()
            );
        }
    }

    /// Signal 3 alone, and the amendment's core case: three sessions with one
    /// cwd, all overlapping, told apart only by what they said. Topology may
    /// not carry this — the answer clears the bar but must stay visibly short
    /// of the topological one.
    #[test]
    fn topology_b_is_never_confident_on_topology_alone() {
        let wts = fleet(&["join", "skeleton", "ci"]);
        // Every subagent transcript records the parent's cwd: the repo root.
        let sessions = vec![
            session(
                "skeleton",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "join",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "ci",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
        ];

        // Nobody says anything: the chain filtered three sessions and cannot
        // choose. The best score must be below the bar, so `at` refuses.
        let mute = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap();
        assert!(
            score(&mute) < CONFIDENT,
            "topology B answered {} on topology alone — the arkanoid-v1 failure",
            mute.tier
        );
        assert_eq!(score(&mute), TOPOLOGY_FILTER + OVERLAP);
        assert!(mute.evidence[0]
            .detail
            .contains("topology B (subagent-per-agent)"));
        assert!(mute.evidence[0].detail.contains("repo-level match only"));
        // The silence is on the record, not implied by the number.
        assert_eq!(signals(&mute)[2], Signal::TranscriptMention);
        assert!(mute.evidence[2].detail.contains("no mention"));

        // One of them names the bead: it wins, and only it clears the bar.
        let told = transcripts(&[
            (
                "join",
                "PACT_AGENT=join working recount-hwk.31: the scoring join",
            ),
            ("ci", "the ci bead, which mentions nobody else"),
        ]);
        let m = resolve(&query("join", Some("join")), &wts, &sessions, &told).unwrap();
        assert_eq!(m.session.path, sessions[1].path);
        assert_eq!(score(&m), TOPOLOGY_FILTER + OVERLAP + MENTION);
        assert!(score(&m) >= CONFIDENT);
        assert!(
            score(&m) < TOPOLOGY_DISCRIMINATING + OVERLAP,
            "a mention under topology B must not read like a topological match"
        );
        assert!(m.evidence[0]
            .detail
            .contains("topology B (subagent-per-agent)"));
        assert!(m.evidence[2].detail.contains("`join` at line 1"));
    }

    /// The same degeneracy in the other shape the bead names: every candidate
    /// has the *same* cwd, and it is the named worktree. The chain confirmed
    /// the worktree and still cannot say which session.
    #[test]
    fn several_sessions_in_one_worktree_do_not_discriminate_either() {
        let wts = fleet(&["join"]);
        let sessions = vec![
            session(
                "a",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "b",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
        ];

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap();
        assert!(score(&m) < CONFIDENT, "{}", m.tier);
        assert!(m.evidence[0].detail.contains("ambiguous"));
    }

    /// Signal 2 alone: the hard gate. Topology is perfect and the clock says no.
    #[test]
    fn no_overlap_is_a_refusal_not_a_low_score() {
        let wts = fleet(&["join"]);
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T09:00:00Z",
            "2026-08-19T10:00:00Z",
        )];

        let err = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no overlap");
        assert!(
            err.detail().contains("07:25:00Z..2026-08-19T07:35:00Z"),
            "{}",
            err.detail()
        );
        assert!(err.detail().contains("a.jsonl"));
    }

    /// A session with no clock at all cannot be shown to overlap, so it is not
    /// treated as overlapping everything.
    #[test]
    fn a_session_with_no_timestamps_does_not_overlap() {
        let wts = fleet(&["join"]);
        let mut s = session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        );
        s.first_ts = None;
        s.last_ts = None;

        let err = resolve(&query("join", Some("join")), &wts, &[s], &silent).unwrap_err();
        assert_eq!(err.reason(), "no overlap");
    }

    /// Both refusals exist and the right one is chosen: nothing resolves into
    /// the worktree at all, so the question is about the topology, not the clock.
    #[test]
    fn no_cwd_match_when_nothing_resolves_into_the_repo() {
        let wts = fleet(&["join"]);
        let sessions = vec![
            session(
                "a",
                Some("/elsewhere"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session("b", None, "2026-08-19T07:00:00Z", "2026-08-19T08:00:00Z"),
        ];

        let err = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no cwd match");
        assert!(
            err.detail().contains("1 resolve elsewhere"),
            "{}",
            err.detail()
        );
        assert!(err.detail().contains("1 state no cwd"), "{}", err.detail());

        // And a sibling worktree is exclusion by topology, not a weak match.
        let sibling = vec![session(
            "c",
            Some("/r/wt/skeleton"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];
        let wts = fleet(&["join", "skeleton"]);
        assert_eq!(
            resolve(&query("join", Some("join")), &wts, &sibling, &silent)
                .unwrap_err()
                .reason(),
            "no cwd match"
        );
    }

    /// The spec's pruned-worktree obligation: git still lists it, the disk does
    /// not have it, and `at` says so rather than answering from its leftovers.
    #[test]
    fn a_pruned_worktree_is_refused_by_name() {
        let wts = vec![wt(REPO, "main", true), wt("/r/wt/gone", "gone", false)];
        let sessions = vec![session(
            "a",
            Some("/r/wt/gone"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let err = resolve(&query("gone", Some("gone")), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no cwd match");
        assert!(err.detail().contains("gone from disk"), "{}", err.detail());
        assert!(err.detail().contains("/r/wt/gone"), "{}", err.detail());
    }

    /// An `invoked_from` no worktree answers to is a refusal that names what
    /// the repo does have — the misdiagnosis this tool exists to prevent is
    /// answering anyway.
    #[test]
    fn an_unknown_invoked_from_refuses_and_lists_the_worktrees() {
        let wts = fleet(&["join"]);
        let sessions = vec![session(
            "a",
            Some("/r"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let err = resolve(&query("join", Some("ghost")), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no cwd match");
        assert!(err.detail().contains("`main`, `join`"), "{}", err.detail());
    }

    /// No invoked_from: the agent name is tried as a worktree name, and the
    /// substitution is stated rather than assumed.
    #[test]
    fn a_missing_invoked_from_falls_back_to_the_agent_name() {
        let wts = fleet(&["join"]);
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let m = resolve(&query("join", None), &wts, &sessions, &silent).unwrap();
        assert_eq!(score(&m), TOPOLOGY_DISCRIMINATING + OVERLAP);

        // And with no worktree of that name, the chain is repo-wide: it can
        // still filter, never discriminate.
        let m = resolve(&query("orchestrator", None), &wts, &sessions, &silent).unwrap();
        assert!(score(&m) < CONFIDENT);
        assert!(m.evidence[0].detail.contains("topology unnamed"));
    }

    /// A mention never creates a match on its own: this session says the name
    /// in every line and is still excluded, because the chain excluded it.
    #[test]
    fn a_mention_cannot_rescue_a_session_the_chain_excluded() {
        let wts = fleet(&["join", "skeleton"]);
        let sessions = vec![session(
            "loud",
            Some("/r/wt/skeleton"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];
        let loud = transcripts(&[("loud", "join join join recount-hwk.31")]);

        assert_eq!(
            resolve(&query("join", Some("join")), &wts, &sessions, &loud)
                .unwrap_err()
                .reason(),
            "no cwd match"
        );
    }

    /// Topology outranks a mention when it discriminates: the spec's precedence
    /// is an order, not a popularity contest.
    #[test]
    fn topology_outranks_a_mention_when_it_discriminates() {
        let wts = fleet(&["join"]);
        let sessions = vec![
            session(
                "talker",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "worker",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
        ];
        let talk = transcripts(&[("talker", "I am reviewing join and recount-hwk.31 for them")]);

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &talk).unwrap();
        assert_eq!(m.session.path, sessions[1].path);
    }

    /// The word-boundary rule, which under topology B is load-bearing: this is
    /// the decisive signal, so `joined` must not be `join`.
    #[test]
    fn a_mention_is_a_word_not_a_substring() {
        assert_eq!(find_word("wt/join is mine", "join"), Some(3));
        assert_eq!(find_word("PACT_AGENT=join", "join"), Some(11));
        assert_eq!(find_word("Join the fleet", "join"), Some(0));
        assert_eq!(
            find_word("bead recount-hwk.31: go", "recount-hwk.31"),
            Some(5)
        );
        assert_eq!(find_word("they joined the fleet", "join"), None);
        assert_eq!(find_word("rejoin later", "join"), None);
        assert_eq!(find_word("anything", ""), None);
    }

    /// The spec's "never a bare answer", checked as an invariant rather than
    /// trusted: every outcome of every shape carries its reason.
    #[test]
    fn every_outcome_explains_itself() {
        let wts = fleet(&["join", "skeleton"]);
        let all = [
            session(
                "a",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "b",
                Some("/r"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "c",
                Some("/elsewhere"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session(
                "d",
                Some("/r/wt/skeleton"),
                "2026-08-19T09:00:00Z",
                "2026-08-19T10:00:00Z",
            ),
        ];
        let said = transcripts(&[("b", "recount-hwk.31 mentioned here")]);

        for invoked in [Some("join"), Some("skeleton"), Some("ghost"), None] {
            for n in 0..=all.len() {
                let q = query("join", invoked);
                match resolve(&q, &wts, &all[..n], &said) {
                    Ok(m) => {
                        assert!(!m.evidence.is_empty(), "a match with no evidence");
                        assert!((0.0..=1.0).contains(&score(&m)));
                        assert!(
                            m.evidence.iter().any(|e| e.signal == Signal::Topology),
                            "no topology evidence"
                        );
                        assert!(
                            m.evidence.iter().any(|e| e.signal == Signal::TimeOverlap),
                            "no overlap evidence"
                        );
                    }
                    Err(e) => {
                        assert!(!e.detail().is_empty(), "{:?} with no detail", e.reason());
                        assert!(["no cwd match", "no overlap"].contains(&e.reason()));
                    }
                }
            }
        }
    }

    #[test]
    fn snippets_are_one_line_and_bounded() {
        let long = format!("{}join{}", "x ".repeat(60), " y".repeat(60));
        let at = find_word(&long, "join").unwrap();
        let s = snippet(&long, at, 4);
        assert!(s.starts_with('…') && s.ends_with('…'), "{s}");
        assert!(s.contains("join") && !s.contains('\n'));
        assert!(s.chars().count() < 100, "{s}");
        assert_eq!(snippet("just join", 5, 4), "just join");
    }

    /// A fleet whose encoded session directories are real directories under a
    /// tempdir. The keyed tier stats real files, so its tests hand it real
    /// files rather than a mock of `is_file`.
    fn keyed_tree() -> (TempDir, Vec<Worktree>) {
        let td = tempfile::tempdir().unwrap();
        let mut wts = fleet(&["join"]);
        for w in &mut wts {
            w.session_dir = td.path().join(format!("-r-{}", w.name));
        }
        (td, wts)
    }

    /// Write `<session_dir>/<SESSION>/subagents/agent-<sub>.jsonl`.
    fn top_level_file(w: &Worktree) -> PathBuf {
        fs::create_dir_all(&w.session_dir).unwrap();
        let p = w.session_dir.join(format!("{SESSION}.jsonl"));
        fs::write(&p, "").unwrap();
        p
    }

    fn keyed_file(w: &Worktree, sub: &str) -> PathBuf {
        let p = w
            .session_dir
            .join(SESSION)
            .join("subagents")
            .join(format!("agent-{sub}.jsonl"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "").unwrap();
        p
    }

    fn session_at(path: &Path, cwd: Option<&str>) -> SessionMeta {
        SessionMeta {
            path: path.to_path_buf(),
            ..session("keyed", cwd, "2026-08-19T07:00:00Z", "2026-08-19T08:00:00Z")
        }
    }

    /// [`query`], plus the 0.11.0 chain.
    fn keyed_query<'a>(agent: &'a str, sub: Option<&'a str>) -> Query<'a> {
        Query {
            harness_session: Some(SESSION),
            harness_subagent: sub,
            ..query(agent, Some("join"))
        }
    }

    /// The keyed tier, right by construction: the file the event names is the
    /// file on disk, so the answer is an identity and there is nothing to score.
    #[test]
    fn a_keyed_event_is_answered_by_name_and_not_by_the_ladder() {
        let (_td, wts) = keyed_tree();
        // Under the SECOND worktree's directory, so the loop is doing work: the
        // event does not say which encoded directory its session lives in.
        let path = keyed_file(&wts[1], SUB);
        let sessions = vec![session_at(&path, Some("/r"))];

        let m = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &silent).unwrap();
        assert_eq!(m.tier, Tier::Keyed);
        assert_eq!(m.session.path, path);
        assert_eq!(m.tier.confidence(), None, "an identity has no score");
        assert!(m.tier.clears(1.0), "keyed clears every bar");
        // One signal, and it names the filename that proved it.
        assert_eq!(signals(&m), [Signal::Keyed]);
        assert!(
            m.evidence[0].detail.contains(&format!("agent-{SUB}.jsonl")),
            "{}",
            m.evidence[0].detail
        );
        assert!(m.evidence[0].detail.contains(SUB));
        assert!(m.evidence[0].detail.contains("No candidate set"));

        // And it serialises as the identity, not as a number nobody computed.
        let v: serde_json::Value = serde_json::to_value(&m).unwrap();
        assert_eq!(v["tier"], "keyed");
        assert!(v["confidence"].is_null());
    }

    /// The new refusal, and the two causes it must tell apart for the reader.
    /// A keyed id whose file is absent is NOT `no cwd match`: no cwd was
    /// consulted, and sending an auditor to look at the topology would be
    /// sending them to the wrong question.
    #[test]
    fn a_keyed_id_whose_file_is_absent_is_its_own_refusal() {
        let (_td, wts) = keyed_tree();
        // A session that WOULD have won topologically, to prove the keyed tier
        // refuses instead of quietly falling back to a guess.
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let err = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        assert_ne!(err.reason(), "no cwd match");
        let d = err.detail();
        // It names the path it looked at, both of them.
        assert!(d.contains(&format!("agent-{SUB}.jsonl")), "{d}");
        assert!(d.contains("-r-main"), "{d}");
        assert!(d.contains("-r-join"), "{d}");
        // And the two real causes, which are different next steps: look
        // elsewhere here, versus you cannot answer this here at all.
        assert!(d.contains("PRUNED"), "{d}");
        assert!(d.contains("LEDGER CAME FROM ANOTHER MACHINE"), "{d}");
    }

    /// A malformed id is a third thing: nothing was stat'd, so nothing is
    /// missing. The sentence must send the reader to the ledger, not the disk.
    #[test]
    fn a_malformed_keyed_id_is_not_an_absent_file() {
        let (_td, wts) = keyed_tree();

        let err = resolve(&keyed_query("join", Some("../etc")), &wts, &[], &silent).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        let d = err.detail();
        assert!(d.contains("harness_subagent"), "{d}");
        assert!(d.contains("not usable as one"), "{d}");
        assert!(d.contains("the ledger, not the disk"), "{d}");
        // Not the absence sentence: this transcript did not go missing.
        assert!(!d.contains("PRUNED"), "{d}");

        // Half a key is the same kind of thing — an identity that is not one —
        // and still not an absent file.
        let half = Query {
            harness_session: None,
            harness_subagent: Some(SUB),
            ..query("join", Some("join"))
        };
        let err = resolve(&half, &wts, &[], &silent).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        assert!(
            err.detail().contains("harness_session is empty"),
            "{}",
            err.detail()
        );
    }

    /// The other half of the keyed tier: an event with harness_session and no
    /// subagent id was written by a TOP-LEVEL agent, and `<session>.jsonl`
    /// names its transcript exactly as `agent-<id>.jsonl` names a subagent's.
    #[test]
    fn a_session_with_no_subagent_id_is_keyed_to_the_top_level_transcript() {
        let (_td, wts) = keyed_tree();
        // Both files exist under the SECOND worktree's directory, so the loop
        // is doing work and the choice between them is being made: the event
        // names no subagent, so the top-level one is its transcript.
        let sub = keyed_file(&wts[1], SUB);
        let top = top_level_file(&wts[1]);
        let sessions = vec![
            // The ladder's champion, to prove it was never asked.
            session(
                "a",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            session_at(&top, Some("/r")),
            session_at(&sub, Some("/r")),
        ];

        let m = resolve(&keyed_query("join", None), &wts, &sessions, &silent).unwrap();
        assert_eq!(m.tier, Tier::Keyed, "a named file is not a guess");
        assert_eq!(m.session.path, top);
        assert_ne!(m.session.path, sub, "it named the session, not a subagent");
        assert_eq!(m.tier.confidence(), None, "an identity has no score");
        assert_eq!(signals(&m), [Signal::Keyed]);

        // Same tier as a subagent hit, deliberately different words: the reader
        // has to be able to tell who acted.
        let d = &m.evidence[0].detail;
        assert!(d.contains("TOP-LEVEL session transcript"), "{d}");
        assert!(d.contains("nobody's subagent"), "{d}");
        assert!(d.contains(&format!("{SESSION}.jsonl")), "{d}");
        assert!(!d.contains("agent-"), "{d}");
        assert!(
            d.contains("the topological ladder was not consulted"),
            "{d}"
        );
    }

    /// The same refusal as an absent subagent file, in words that fit a
    /// top-level one: the two causes still hold, but the worktree-tidying half
    /// of the first is a subagent's story and must not be told as if it were
    /// this row's.
    #[test]
    fn an_absent_top_level_transcript_refuses_in_its_own_words() {
        let (_td, wts) = keyed_tree();
        // A session that WOULD have won topologically: the keyed tier refuses
        // rather than quietly falling back to a guess, for this half too.
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let err = resolve(&keyed_query("join", None), &wts, &sessions, &silent).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        let d = err.detail();
        assert!(d.contains(&format!("{SESSION}.jsonl")), "{d}");
        assert!(!d.contains("agent-"), "{d}");
        assert!(d.contains("-r-main") && d.contains("-r-join"), "{d}");
        // The two causes, unchanged, plus the clause that keeps the first one
        // from lying to a top-level reader.
        assert!(d.contains("PRUNED"), "{d}");
        assert!(d.contains("LEDGER CAME FROM ANOTHER MACHINE"), "{d}");
        assert!(d.contains("OUTLIVES THE RUN"), "{d}");
    }

    /// The third sentence, and the one most easily broken. A v0.1.0-era row
    /// carries neither id. It is OLD, not defective, and the evidence must not
    /// tell its reader that a subagent id was expected and missing.
    #[test]
    fn a_row_with_no_chain_at_all_is_old_rather_than_missing_an_id() {
        let (_td, wts) = keyed_tree();
        // Both keyed files are on disk. Neither is reachable: a pre-contract
        // row names nothing, so there is nothing to join.
        keyed_file(&wts[1], SUB);
        top_level_file(&wts[1]);
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &silent).unwrap();
        assert_eq!(
            m.tier,
            Tier::Topological(Confidence::new(TOPOLOGY_DISCRIMINATING + OVERLAP))
        );
        let why = m.evidence.last().unwrap();
        assert_eq!(why.signal, Signal::Keyed);
        assert!(why.detail.contains("no harness chain on this event at all"));
        assert!(why.detail.contains("PRE-CONTRACT"));
        assert!(why.detail.contains("OLD, not defective"));
        assert!(why.detail.contains("INFERRED"));
        // Not the parent-keyed story: this row is not a top-level agent's, it
        // predates the field that would say so.
        assert!(!why.detail.contains("not written by a subagent"));
        assert!(!why.detail.contains("TOP-LEVEL"));
        // And not the absent-file story: nothing here has gone missing.
        assert!(!why.detail.contains("PRUNED"));
    }

    /// Both keyed halves could apply and the subagent id wins: it names the
    /// narrower file, and the parent session is not who wrote the event.
    #[test]
    fn a_subagent_id_outranks_the_top_level_file_when_both_are_on_disk() {
        let (_td, wts) = keyed_tree();
        let sub = keyed_file(&wts[0], SUB);
        let top = top_level_file(&wts[0]);
        let sessions = vec![session_at(&top, Some("/r")), session_at(&sub, Some("/r"))];

        let m = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &silent).unwrap();
        assert_eq!(m.tier, Tier::Keyed);
        assert_eq!(m.session.path, sub);
        let d = &m.evidence[0].detail;
        assert!(d.contains(&format!("harness_subagent `{SUB}`")), "{d}");
        assert!(!d.contains("TOP-LEVEL"), "{d}");
    }

    /// The one that matters: the two tiers disagree, and keyed wins.
    ///
    /// The topological ladder is handed its best possible case — topology A,
    /// discriminating, with a mention, which is 0.95 and the highest score this
    /// tool can produce — and the event names a different transcript. The keyed
    /// answer is the one that comes back, and it says why.
    #[test]
    fn keyed_beats_the_ladder_even_at_its_best_score() {
        let (_td, wts) = keyed_tree();
        let path = keyed_file(&wts[0], SUB);
        let sessions = vec![
            // The ladder's champion: cwd IS the named worktree, and it says the
            // agent name and the bead id.
            session(
                "champion",
                Some("/r/wt/join"),
                "2026-08-19T07:00:00Z",
                "2026-08-19T08:00:00Z",
            ),
            // The transcript the event actually names, at the repo root, saying
            // nothing at all.
            session_at(&path, Some("/r")),
        ];
        let told = transcripts(&[("champion", "PACT_AGENT=join on recount-hwk.31")]);

        // What the ladder alone would have said, on the same inputs.
        let inferred = resolve(&query("join", Some("join")), &wts, &sessions, &told).unwrap();
        assert_eq!(inferred.session.path, sessions[0].path);
        assert_eq!(
            score(&inferred),
            TOPOLOGY_DISCRIMINATING + OVERLAP + MENTION
        );

        // What it says once the event names its transcript.
        let keyed = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &told).unwrap();
        assert_eq!(keyed.session.path, path);
        assert_eq!(keyed.tier, Tier::Keyed);
        assert!(
            keyed.tier > inferred.tier,
            "the ladder outranked an identity: {} vs {}",
            keyed.tier,
            inferred.tier
        );
        // The evidence says which tier answered, and the losing signals are not
        // in it: nothing was scored, so there is nothing to report scoring.
        assert_eq!(signals(&keyed), [Signal::Keyed]);
        assert!(keyed.evidence[0]
            .detail
            .contains("the topological ladder was not consulted"));
    }

    /// The file is there and nothing read it. Practically unreachable — the
    /// mtime prune cannot drop a transcript that was being written when the
    /// event fired — but an identity must never be downgraded to a guess in
    /// silence, so it has a sentence of its own.
    #[test]
    fn a_named_transcript_nobody_read_says_so_rather_than_guessing() {
        let (_td, wts) = keyed_tree();
        let path = keyed_file(&wts[0], SUB);

        let err = resolve(&keyed_query("join", Some(SUB)), &wts, &[], &silent).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        assert!(err.detail().contains("IS on disk"), "{}", err.detail());
        assert!(
            err.detail().contains(&path.display().to_string()),
            "{}",
            err.detail()
        );
    }
}
