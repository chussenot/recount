//! The join: which session was driving this identity at this moment, and how
//! sure are we.
//!
//! This is the whole point of the tool, so its rules are written down rather
//! than left in the scoring arithmetic.
//!
//! # Two tiers, and only one of them is scored
//!
//! pact 0.11.0 stamps `harness_subagent` on the events it writes, and an event
//! carrying one NAMES its transcript. Resolving it is a path join and one
//! `is_file()` — no candidate set, no score, no tie to break — so it is not a
//! fifth rung on the ladder below but a tier ABOVE the whole ladder:
//! [`Tier::Keyed`], evidenced by the filename that proved it. Everything from
//! here to [`CONFIDENT`] is the tier BELOW: calibrated inference, unchanged by
//! this, and reached only by an event with no `harness_subagent`. Neither tier
//! is tuned in terms of the other — one is an identity, the other is a guess.
//!
//! Which tier answered is on every result, and when it was the lower one the
//! [`Signal::Keyed`] evidence line says why the higher one did not apply. The
//! two ways that happens are different sentences with different next steps: **no
//! `harness_subagent` on the event** means it was not written by a subagent,
//! which is normal and where the ladder answers; **a keyed id whose file is
//! absent** means the event named a transcript that is not there, which is a
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
//! 2. **Time overlap** — REQUIRED, not weighted. A session whose turns do not
//!    overlap the event window is not a weak match, it is not a match: it
//!    cannot have produced the event. No overlap short-circuits to
//!    [`Refusal::NoOverlap`] before any score is computed.
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

/// One session that survived the chain, and how well it survived it.
struct Candidate<'a> {
    session: &'a SessionMeta,
    /// The worktree its cwd resolves to — the named one, or an ancestor of it.
    resolved: &'a Worktree,
    /// Its cwd names the queried worktree itself, not merely an ancestor.
    exact: bool,
}

/// The two tiers, in order: keyed if the event named its transcript, the
/// scored ladder if it did not.
///
/// **The gate is `harness_subagent` and nothing else.** Not `harness.is_some()`
/// and not `harness_session.is_some()`: measured on this repo's own ledger, five
/// rows carry `harness`, `model`, `harness_session` and `branch` with no subagent
/// id — `kind=context` rows written by a top-level agent, which has a session and
/// is not a subagent of anything, and every orchestrator row looks like that.
/// Proxying on those fields would send the fleet's top-level events into a keyed
/// lookup that cannot succeed, turning a normal state into a refusal.
///
/// `turns` reads a session's transcript for the mention tiebreaker. It is a
/// callback so the join does not decide which connector owns a path and so
/// transcripts are read lazily — most candidates are eliminated by topology or
/// overlap without ever being opened. The keyed tier never calls it: nothing in
/// a transcript could raise an identity.
pub fn resolve(
    query: &Query,
    worktrees: &[Worktree],
    sessions: &[SessionMeta],
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Result<Match, Refusal> {
    match query.harness_subagent {
        Some(subagent) => keyed(query, subagent, worktrees, sessions),
        None => topological(query, worktrees, sessions, turns),
    }
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
    subagent: &str,
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
        match claude_code::keyed(&w.session_dir, session, Some(subagent)) {
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

    Err(Refusal::NoKeyedTranscript {
        detail: format!(
            "the event names transcript agent-{subagent}.jsonl under harness_session \
             {session}, and it is not on disk: looked at {}. Two things do this and they are \
             not the same news. Either the transcript was PRUNED — Claude Code keeps 30 days, \
             and a fleet that tidies its worktrees as it goes takes their encoded session \
             directories with them — in which case look elsewhere on this machine and at your \
             backups. Or this LEDGER CAME FROM ANOTHER MACHINE, in which case the transcript \
             was never here, no amount of looking will find it, and this question cannot be \
             answered on this host at all",
            paths(&looked)
        ),
    })
}

/// The transcript is on disk. Turn it into an answer, or say why the sessions
/// the join was offered do not contain it.
fn keyed_match(
    path: PathBuf,
    session: &str,
    subagent: &str,
    sessions: &[SessionMeta],
) -> Result<Match, Refusal> {
    // Practically unreachable: a transcript being written when the event fired
    // has an mtime at or after it, so the prune cannot drop it (`crate::prune`).
    // Not impossible though, and an identity silently downgraded to a guess is
    // the one outcome this tier exists to prevent.
    let Some(meta) = sessions.iter().find(|s| s.path == path) else {
        return Err(Refusal::NoKeyedTranscript {
            detail: format!(
                "the event names agent-{subagent}.jsonl, which IS on disk at {}, but it is \
                 not among the {} \
                 session(s) the join was offered — no connector read it. The file is there; \
                 what is missing is a reader for it, so look at the file, not for it",
                path.display(),
                sessions.len()
            ),
        });
    };
    Ok(Match {
        session: meta.clone(),
        tier: Tier::Keyed,
        evidence: vec![Evidence::new(
            Signal::Keyed,
            format!(
                "the event named its own transcript: harness_subagent `{subagent}` under \
                 harness_session `{session}` is {}, found at {} by one path join and one \
                 stat. No candidate set, no score, no tie to break — this is an identity, and \
                 the topological ladder was not consulted",
                file_name(meta),
                path.display()
            ),
        )],
    })
}

/// Why the keyed tier did not answer, on every answer the ladder gives.
///
/// One sentence, because there is exactly one way to get here: an event with a
/// `harness_subagent` either resolves keyed or refuses, so reaching the ladder
/// means the field was absent. Distinguishing that from "keyed id present, file
/// absent" is the point — different questions, and this one is not a problem.
const NO_KEY: &str =
    "no `harness_subagent` on this event, so nothing named a transcript: it was not written by \
     a subagent. A top-level agent has a harness_session and is not a subagent of anything, and \
     every orchestrator row in this repo's ledger looks exactly like this — a normal state, not \
     a degraded one. The answer below is INFERRED by the topological ladder, not keyed";

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

    // 3. Time overlap, the one hard gate. A session with no timestamp at all
    //    (an empty or wholly unparseable file) cannot be shown to overlap, so
    //    it does not: unbounded here would mean "always", which is the opposite
    //    of what an absent clock tells us.
    let window = TimeWindow::around(query.at, query.window);
    let (overlapping, missed): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| {
        let w = c.session.window();
        !w.is_unbounded() && w.overlaps(&window)
    });
    if overlapping.is_empty() {
        return Err(Refusal::NoOverlap {
            detail: format!(
                "{} session(s) matched {chain}, but none has a turn in {}: {}",
                missed.len(),
                fmt_window(&window),
                missed
                    .iter()
                    .map(|c| format!(
                        "{} covers {}",
                        file_name(c.session),
                        fmt_window(&c.session.window())
                    ))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        });
    }

    // 4. Did the chain actually single anyone out? One exact link and the
    //    answer is topological; anything else and topology has only filtered,
    //    whatever the spec's happy path assumes.
    let exact = overlapping.iter().filter(|c| c.exact).count();
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
    //    systematically loses its own session. The keyed tier does NOT fix
    //    this. It cannot even reach it: a keyed event has one candidate and
    //    no tie, while every event gwx was measured on is an orchestrator row
    //    with no harness_subagent, which arrives here unchanged.
    let mut best: Option<(f64, Match)> = None;
    for c in &overlapping {
        let (score, scored) = score(query, c, topology, &window, turns);
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
fn score(
    query: &Query,
    c: &Candidate,
    topology: Topology,
    window: &TimeWindow,
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> (f64, Match) {
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

    total += OVERLAP;
    evidence.push(Evidence::new(
        Signal::TimeOverlap,
        format!(
            "session covers {} and overlaps the queried {}",
            fmt_window(&c.session.window()),
            fmt_window(window)
        ),
    ));

    // The transcript is opened only here — after both gates — and only for
    // candidates that survived them.
    let mention = mention(query, &turns(&c.session.path));
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
    evidence.push(Evidence::new(Signal::Keyed, NO_KEY));

    (
        total,
        Match {
            session: c.session.clone(),
            tier: Tier::Topological(Confidence::new(total)),
            evidence,
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

    /// Transcripts by file stem. Anything not listed reads as an empty file,
    /// which is how a session with no mention is spelled.
    fn transcripts(by_stem: &[(&str, &str)]) -> impl Fn(&Path) -> Vec<Turn> {
        let map: HashMap<String, String> = by_stem
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |p: &Path| {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            map.get(stem.as_ref())
                .map(|text| {
                    vec![Turn {
                        ts: ts("2026-08-19T07:30:00Z"),
                        role: Role::User,
                        text: text.clone(),
                        line: 1,
                    }]
                })
                .unwrap_or_default()
        }
    }

    fn none(_: &Path) -> Vec<Turn> {
        Vec::new()
    }

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

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &none).unwrap();
        assert_eq!(m.session.path, sessions[0].path);
        assert_eq!(score(&m), TOPOLOGY_DISCRIMINATING + OVERLAP);
        assert!(score(&m) >= CONFIDENT);
        assert_eq!(
            signals(&m),
            [Signal::Topology, Signal::TimeOverlap, Signal::Keyed]
        );
        // Even the cleanest topological answer says it is not the keyed one.
        assert!(m.evidence[2].detail.starts_with("no `harness_subagent`"));
        assert!(m.evidence[0]
            .detail
            .contains("topology A (process-per-agent)"));
        assert!(m.evidence[0].detail.contains("is worktree `join`"));
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
        let mute = resolve(&query("join", Some("join")), &wts, &sessions, &none).unwrap();
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

        let m = resolve(&query("join", Some("join")), &wts, &sessions, &none).unwrap();
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

        let err = resolve(&query("join", Some("join")), &wts, &sessions, &none).unwrap_err();
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

        let err = resolve(&query("join", Some("join")), &wts, &[s], &none).unwrap_err();
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

        let err = resolve(&query("join", Some("join")), &wts, &sessions, &none).unwrap_err();
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
            resolve(&query("join", Some("join")), &wts, &sibling, &none)
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

        let err = resolve(&query("gone", Some("gone")), &wts, &sessions, &none).unwrap_err();
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

        let err = resolve(&query("join", Some("ghost")), &wts, &sessions, &none).unwrap_err();
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

        let m = resolve(&query("join", None), &wts, &sessions, &none).unwrap();
        assert_eq!(score(&m), TOPOLOGY_DISCRIMINATING + OVERLAP);

        // And with no worktree of that name, the chain is repo-wide: it can
        // still filter, never discriminate.
        let m = resolve(&query("orchestrator", None), &wts, &sessions, &none).unwrap();
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

        let m = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &none).unwrap();
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

        let err = resolve(&keyed_query("join", Some(SUB)), &wts, &sessions, &none).unwrap_err();
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

        let err = resolve(&keyed_query("join", Some("../etc")), &wts, &[], &none).unwrap_err();
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
        let err = resolve(&half, &wts, &[], &none).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        assert!(
            err.detail().contains("harness_session is empty"),
            "{}",
            err.detail()
        );
    }

    /// Wave 1's finding, pinned: the rest of the attribution chain does NOT
    /// imply `harness_subagent`. An event with harness_session and no subagent
    /// id was written by a top-level agent, which is normal — so the ladder
    /// answers, and the answer says why the tier above it did not.
    #[test]
    fn the_chain_without_a_subagent_id_falls_through_and_says_why() {
        let (_td, wts) = keyed_tree();
        // The transcript the keyed tier WOULD have found, if it were gated on
        // harness_session. It is not: this event names no subagent.
        keyed_file(&wts[1], SUB);
        let sessions = vec![session(
            "a",
            Some("/r/wt/join"),
            "2026-08-19T07:00:00Z",
            "2026-08-19T08:00:00Z",
        )];

        let m = resolve(&keyed_query("join", None), &wts, &sessions, &none).unwrap();
        assert_eq!(
            m.tier,
            Tier::Topological(Confidence::new(TOPOLOGY_DISCRIMINATING + OVERLAP))
        );
        let why = &m.evidence[m.evidence.len() - 1];
        assert_eq!(why.signal, Signal::Keyed);
        assert!(why.detail.contains("no `harness_subagent` on this event"));
        assert!(why.detail.contains("not written by a subagent"));
        assert!(why.detail.contains("normal state, not"));
        assert!(why.detail.contains("INFERRED"));
        // The distinction that must not collapse: this is not the sentence an
        // absent keyed file gets.
        assert!(!why.detail.contains("PRUNED"));
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

        let err = resolve(&keyed_query("join", Some(SUB)), &wts, &[], &none).unwrap_err();
        assert_eq!(err.reason(), "no keyed transcript");
        assert!(err.detail().contains("IS on disk"), "{}", err.detail());
        assert!(
            err.detail().contains(&path.display().to_string()),
            "{}",
            err.detail()
        );
    }
}
