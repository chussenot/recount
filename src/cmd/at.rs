//! `recount at --repo <p> --agent <name> --ts <rfc3339> [--window 5m]`
//!
//! Spec: "resolve the session driving that identity at that moment; print the
//! transcript window. Exit 0 found / 1 no-confident-match with the reason (no
//! cwd match vs no overlap)."
//!
//! Both exits are answers. `EXIT_NO_MATCH` is only ever returned alongside a
//! printed `join::Refusal`, and `EXIT_OK` is only ever returned alongside the
//! match's confidence AND evidence — there is no path here that prints a bare
//! answer, in either output mode.
//!
//! # Three outcomes behind two exit codes
//!
//! [`join::resolve`] deliberately does not apply [`CONFIDENT`]; it returns the
//! best candidate *with its score* and leaves the threshold to this command.
//! Applying it splits exit 1 in two, and the two must not be collapsed:
//!
//! | outcome           | exit | printed                                        |
//! |-------------------|-----:|------------------------------------------------|
//! | `match`           |    0 | session, confidence, evidence, transcript window |
//! | `low_confidence`  |    1 | session, confidence, evidence — no transcript    |
//! | `refused`         |    1 | the reason (no cwd match / no overlap) + detail  |
//!
//! **`low_confidence` prints the candidate on purpose.** A score under the bar
//! is not a [`crate::join::Refusal`]: both gates passed, a real session is there, and what
//! failed is the arithmetic. Its evidence is precisely what tells an auditor
//! where to look next — a 0.40 says "topology B, and the transcript never
//! names the agent", which is a fact about the fleet's shape, not an absence
//! of data. Exiting 1 with nothing would throw that away.
//!
//! **`refused` prints no candidate, also on purpose.** The two [`crate::join::Refusal`]
//! variants are structural: the chain produced nobody to score. `reason` is one
//! of the spec's exactly-two strings, straight out of `Refusal::reason`, and
//! the digest says which question it sends the reader to — the topology or the
//! clock.
//!
//! # Why the transcript is withheld below the bar
//!
//! The transcript window is the answer to "what was this agent saying". Under
//! topology B (`join`'s module doc) every session in the repo overlaps every
//! moment, so a sub-threshold candidate's transcript is *a* transcript, not
//! *the* transcript. Printing it under a heading that reads like testimony is
//! the arkanoid-v1 misdiagnosis with extra steps. The path is printed, so
//! `recount grep` or `sed -n` is one command away for a reader who has decided
//! the evidence justifies it.
//!
//! # Which regime produced the number
//!
//! The confidence alone distinguishes the four regimes `join` scores (0.95 /
//! 0.70 / 0.65 / 0.40), but nobody should have to memorise the table: the
//! topology [`Evidence`] says "topology A (process-per-agent) … the chain
//! discriminates" or "topology B … the transcript mention decides" in words,
//! and every outcome prints every piece of evidence it has. That is the "never
//! a bare answer" clause, honoured in both streams.
//!
//! # The scan is cheap first and honest second
//!
//! Discovery is windowed, which is the spec's cheapness clause: transcripts are
//! append-only, so mtime rules out everything that ended before the window
//! without opening it. That prune also deletes the evidence for "no overlap" —
//! a session that ran yesterday never reaches the join, so the join has no
//! candidate and refuses with "no cwd match", which is *false* and sends the
//! reader to the topology when the answer was the clock. [`decide`] therefore
//! repeats the scan unwindowed when, and only when, the cheap pass refuses on
//! topology. A match never pays for it: overlap is a hard gate, so a pruned
//! session could not have won either scan. Measured on this repo: 43ms for the
//! answering path, 110ms for the one that is about to explain itself.
//!
//! # Output
//!
//! The house shape from [`crate::cmd::sessions`]: envelope `{command, schema,
//! …payload}`; with `--json` the document alone on stdout and the digest
//! second on stderr; without it, the digest alone on stdout.
//!
//! ```text
//! { "command": "at", "schema": 1,
//!   "repo": "/abs/path", "agent": "cmdat",
//!   "ts": "2026-08-19T07:03:12Z",
//!   "window": { "since": "…T06:58:12Z", "until": "…T07:08:12Z" },
//!   "session_dirs": ["/home/u/.claude/projects/-abs-path", …],
//!   "considered": 7,                  // sessions the window could not rule out
//!   "threshold": 0.5,
//!   "outcome": "match" | "low_confidence" | "refused",
//!   "best": { "session": {…}, "confidence": 0.95, "confident": true,
//!             "evidence": [ { "signal": "topology", "detail": "…" }, … ],
//!             "transcript": [ { "ts": …, "role": …, "text": …, "line": … } ] },
//!   "refusal": { "reason": "no cwd match", "detail": "…" } }
//! ```
//!
//! `best` and `refusal` are exactly one non-null each. `transcript` is `[]`
//! unless `outcome` is `"match"`.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;

use crate::cli::AtArgs;
use crate::cmd::{EXIT_NO_MATCH, EXIT_OK};
use crate::connector::{self, Connector};
use crate::join::{self, Match, Query, CONFIDENT};
use crate::model::{Evidence, SessionMeta, Signal, Tier, TimeWindow, Turn};
use crate::pact_events::{self, Event};
use crate::worktree::{self, Worktree};

/// Bumped only when a field leaves or changes meaning, like every other
/// command's.
const SCHEMA: u32 = 1;

/// The spec's first refusal, spelled once. [`crate::join::Refusal::reason`]
/// produces it; [`resolve`] branches on it.
const NO_CWD_MATCH: &str = "no cwd match";

/// The digest quotes a turn up to this many characters. The full text is in
/// the JSON: one `tool_result` can be megabytes, and the digest is for reading.
const DIGEST_TURN_CHARS: usize = 600;

pub fn run(args: &AtArgs, json: bool) -> Result<u8> {
    run_with(args, None, None, json)
}

/// `at`, plus everything a pact event carries that the CLI has no flag for.
///
/// `explain` calls this rather than [`run`]: only the command holding the event
/// can supply `invoked_from`, the bead id inside `detail`, the two harness ids
/// that gate the keyed tier, and the attribution chain the header prints.
/// Passing `None, None` — which is what [`run`]  does — is not a lesser join,
/// it is the one `at`'s flags can express: the chain then falls back to "is
/// there a worktree named after the agent", says so in its evidence, and prints
/// no header, because a header with five dashes in it is not testimony.
///
/// The event is taken whole rather than as five arguments because it is one
/// thing: pact stamps the chain as a unit, and a caller that can pass four of
/// its links can pass the fifth.
pub fn run_with(
    args: &AtArgs,
    event: Option<&Event>,
    bead: Option<&str>,
    json: bool,
) -> Result<u8> {
    let report = resolve(args, event, bead)?;
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(report.exit_code())
}

/// Discover and join, printing nothing. Separate from [`run_with`] because the
/// whole pipeline — git, the path encoding, the connectors, the join — is worth
/// asserting against a real tree, and a test cannot read a report it has
/// already printed to the process's own stdout.
fn resolve(args: &AtArgs, event: Option<&Event>, bead: Option<&str>) -> Result<Report> {
    // Resolution before any file opens, the spec's cheapness clause: --repo
    // becomes encoded session directories, and the absent ones are dropped for
    // one stat each. A repo no agent ever ran in reaches no connector at all.
    let worktrees = worktree::worktrees(&args.repo)?;
    let dirs: Vec<PathBuf> = worktree::session_dirs(&args.repo)?
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let repo = worktrees
        .first()
        .map_or_else(|| args.repo.clone(), |w| w.path.clone());

    // Who else the ledger says wrote session-only rows under this event's
    // session — the one question the event cannot answer about itself, and
    // only asked of a session-only row, the one shape a subagent can forge by
    // omission. A ledger that cannot be read leaves it empty: the row keys as
    // it always did, which is the behaviour before this check existed.
    let ledger = event
        .filter(|e| e.harness_subagent.is_none() && e.harness_session.is_some())
        .and_then(|_| pact_events::read(&pact_events::events_path(&args.repo)).ok());
    let session_agents = match (&ledger, event.and_then(|e| e.harness_session.as_deref())) {
        (Some(l), Some(session)) => l.session_only_agents(session),
        _ => Vec::new(),
    };

    let query = Query {
        agent: &args.agent,
        at: args.ts,
        window: args.window,
        invoked_from: event.and_then(|e| e.invoked_from.as_deref()),
        bead,
        // The two ids that gate the keyed tier. They come from the event and
        // from nowhere else: `at`'s flags cannot name a transcript, so a plain
        // `recount at` still resolves topologically, as it always did.
        harness_session: event.and_then(|e| e.harness_session.as_deref()),
        harness_subagent: event.and_then(|e| e.harness_subagent.as_deref()),
        session_agents: &session_agents,
    };
    let mut report = decide(&query, &repo, &worktrees, &dirs, &connector::registry());
    attribute(&mut report, event);
    Ok(report)
}

/// The join, scanned cheaply first and honestly second.
///
/// Split from [`resolve`] so it can be driven over a fixture tree without a
/// `$HOME` — and because the retry below is the subtlest decision in this file.
fn decide(
    query: &Query,
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    connectors: &[Box<dyn Connector>],
) -> Report {
    let report = join_over(
        query,
        repo,
        worktrees,
        dirs,
        connectors,
        &TimeWindow::around(query.at, query.window),
    );

    // The windowed scan above is the spec's cheap path: sessions are
    // append-only, so mtime rules out transcripts that ended before the window
    // WITHOUT OPENING THEM. It is also the one thing that can make `at` give
    // the wrong reason. A session that ran yesterday is pruned before the join
    // sees it, so the join finds no candidate at all and refuses with "no cwd
    // match" — sending the auditor to the topology when the answer was the
    // clock, which is the exact collapse the spec's two reasons exist to
    // prevent.
    //
    // So when, and only when, the cheap path refuses on topology, the scan is
    // repeated unwindowed to find the true reason. A match never needs it:
    // time overlap is a hard gate, so a pruned session could not have won, and
    // the answer on the fast path is the answer on the slow one. The cost
    // falls on the run that is about to explain itself, never on the run that
    // has an answer.
    if report
        .refusal
        .as_ref()
        .is_some_and(|r| r.reason == NO_CWD_MATCH)
    {
        return join_over(
            query,
            repo,
            worktrees,
            dirs,
            connectors,
            &TimeWindow::default(),
        );
    }
    report
}

/// Discover over `dirs` with `scan` as the pruning window, then join. `scan` is
/// separate from the query's own window on purpose — see [`decide`].
fn join_over(
    query: &Query,
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    connectors: &[Box<dyn Connector>],
    scan: &TimeWindow,
) -> Report {
    let sessions = if dirs.is_empty() {
        Vec::new()
    } else {
        connector::discover_all(connectors, dirs, scan)
    };
    let report = answer(
        query,
        repo,
        worktrees,
        dirs,
        &sessions,
        &reader(connectors, &sessions),
    );
    report
}

/// Which of the three things happened. Two of them exit 1; see the module doc
/// for why they are not one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    /// A candidate at or above [`CONFIDENT`].
    Match,
    /// A candidate below it. Printed anyway, transcript withheld.
    LowConfidence,
    /// No candidate at all: one of the spec's two structural reasons.
    Refused,
}

/// The best candidate the chain produced, confident or not.
#[derive(Debug, Clone, Serialize)]
struct Best {
    session: SessionMeta,
    /// Flattened to `{"tier": …, "confidence": …}`: `confidence` keeps its
    /// meaning and its number for a topological answer, and is an explicit
    /// `null` for a keyed one, which has no position on the ladder.
    #[serde(flatten)]
    tier: Tier,
    /// Never empty — [`join::Match`] cannot be built without it.
    evidence: Vec<Evidence>,
    /// `join::Match::confident()`. Duplicated from the comparison so a reader
    /// of the JSON never has to know the threshold — or the lead rule — to
    /// know the verdict.
    confident: bool,
    /// Nothing structural picked this session out, so this answer is a LEAD:
    /// never `confident`, whatever `confidence` says. Topologically, the chain
    /// filtered but did not discriminate — a 0.65 a mention picked out and a
    /// 0.40 nothing spoke for are both leads. Keyed, the ledger attributes the
    /// named top-level transcript to more than one agent. A separate field
    /// from `confident` because "the score fell short" and "nothing could have
    /// made this an answer" are different news.
    lead: bool,
    /// Turns inside the queried window. Empty unless `confident`.
    transcript: Vec<Turn>,
}

/// A [`crate::join::Refusal`] flattened for `jq`: the spec's two-word reason and the
/// sentence behind it, rather than an externally tagged enum.
#[derive(Debug, Clone, Serialize)]
struct RefusalOut {
    reason: &'static str,
    detail: String,
}

/// The attribution pact 0.11.0 stamped on the event itself: who ran the agent,
/// under what model, in which transcript, on which branch.
///
/// Verbatim, and every field optional, because a row written before 0.11.0 —
/// or by an orchestrator, which has a session and is nobody's subagent —
/// legitimately carries fewer than five. An absent link is printed as an absent
/// link, never inferred: the point of a chain is that a reader can see where it
/// stops.
///
/// # `declared_model` is named for what it is
///
/// It is `PACT_MODEL` — what the spawner ASKED FOR. It is not evidence that any
/// model answered, and the two genuinely come apart: a fleet that declares
/// `claude-opus-5` and gets served something else has an orchestration bug, and
/// a header that printed this as "model" would hide it behind a word that reads
/// like an observation. The digest carries the same marker in words.
#[derive(Debug, Clone, Serialize)]
struct Chain {
    harness: Option<String>,
    declared_model: Option<String>,
    harness_session: Option<String>,
    harness_subagent: Option<String>,
    branch: Option<String>,
}

impl Chain {
    fn of(e: &Event) -> Self {
        Chain {
            harness: e.harness.clone(),
            declared_model: e.model.clone(),
            harness_session: e.harness_session.clone(),
            harness_subagent: e.harness_subagent.clone(),
            branch: e.branch.clone(),
        }
    }
}

/// What the declared model and the answering transcript said, side by side.
///
/// Present only when BOTH sides exist. Every absence is silence: the spec's
/// rule is never to error on a comparison that cannot be made, and a comparison
/// with one side missing is not a weak comparison, it is not one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ModelCheck {
    /// `PACT_MODEL`, from the event.
    declared: String,
    /// Every distinct `message.model` the transcript recorded, most frequent
    /// first. A list, not a value — see [`Verdict::Switched`].
    recorded: Vec<String>,
    verdict: Verdict,
}

/// The three things this comparison can honestly say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    /// Every value the transcript recorded is the declared model.
    Agrees,
    /// The declared model is among them, and it is not the only one: the run
    /// changed model part-way through. Not drift — the declaration held, for
    /// part of the run — and the most interesting thing here, so it is its own
    /// verdict rather than a footnote on either of the others.
    Switched,
    /// The declared model is not among them at all. Orchestration drift.
    Differs,
}

/// A model slug with a trailing `-YYYYMMDD` release date removed.
///
/// `claude-haiku-4-5-20251001` and `claude-haiku-4-5` are the same model, and
/// they meet: the transcript records whatever the API returned, and `PACT_MODEL`
/// is whatever a human typed. An exact string compare reports that pair as a
/// discrepancy, which is a false accusation about an agent that did exactly what
/// it was told.
fn undated(slug: &str) -> &str {
    match slug.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => slug,
    }
}

/// Declared against recorded, or `None` for "say nothing".
///
/// Two sources are deliberately NOT consulted here. The sidecar's own `model`
/// key is a spawn-time alias (`sonnet`, `opus`) and is not a slug, so comparing
/// it would manufacture a discrepancy out of a naming convention. And
/// `<synthetic>` — the stub Claude Code writes on its own API-error records, 72
/// of 80 of them rate-limit and auth failures — is already excluded upstream in
/// [`crate::connector::claude_code::Identity::models`] and must stay excluded:
/// those lines are still turns, because "the agent got a 429" is exactly what an
/// audit wants to read, but they are not a model that answered.
fn model_check(declared: Option<&str>, recorded: &[String]) -> Option<ModelCheck> {
    let declared = declared?;
    if recorded.is_empty() {
        return None;
    }
    let hit = |m: &&String| undated(m) == undated(declared);
    let verdict = if recorded.iter().all(|m| hit(&m)) {
        Verdict::Agrees
    } else if recorded.iter().any(|m| hit(&m)) {
        Verdict::Switched
    } else {
        Verdict::Differs
    };
    Some(ModelCheck {
        declared: declared.to_string(),
        recorded: recorded.to_vec(),
        verdict,
    })
}

/// Every model the winning transcript actually recorded, most frequent first.
///
/// Only `claude_code` knows: `generic_jsonl` reads no model field by design, so
/// there is nothing to ask it and an empty list from it is silence rather than
/// disagreement. This re-reads the one file that won, like [`best`] does for the
/// transcript, and for the same reason — the alternative is a `SessionMeta` that
/// carries the set, which is a change across three other beads' files.
fn recorded_models(meta: &SessionMeta) -> Vec<String> {
    if meta.connector == connector::claude_code::ClaudeCode.name() {
        connector::claude_code::scan(&meta.path).identity.models
    } else {
        Vec::new()
    }
}

/// Hang the event's chain, and the one comparison it makes possible, on a
/// report the join has already finished.
///
/// After the join rather than inside it, because none of this is an input to
/// the join: the chain is what the ledger says about the question, and the
/// comparison is between that and the answer.
///
/// **The comparison waits for a confident match.** Below the bar there is a
/// candidate but not a verdict, and attributing that session's recorded model to
/// this agent would be the same mistake as printing its transcript: a fact about
/// *a* session, printed under a heading that reads as a fact about *the* agent.
/// A false "orchestration drift" is worse than no line at all.
fn attribute(r: &mut Report, event: Option<&Event>) {
    let Some(e) = event else { return };
    r.chain = Some(Chain::of(e));
    if let Some(b) = &r.best {
        if b.confident {
            r.model_check = model_check(e.model.as_deref(), &recorded_models(&b.session));
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct Report {
    command: &'static str,
    schema: u32,
    repo: PathBuf,
    agent: String,
    ts: DateTime<Utc>,
    window: TimeWindow,
    session_dirs: Vec<PathBuf>,
    /// Sessions the window could not rule out — the field the join chose from.
    considered: usize,
    threshold: f64,
    /// What pact stamped on the event. `null` for a plain `recount at`, which
    /// has no event: a chain of five nulls would read like a broken ledger.
    chain: Option<Chain>,
    /// `null` whenever either side is missing, or the match is not confident.
    /// See [`attribute`].
    model_check: Option<ModelCheck>,
    outcome: Outcome,
    best: Option<Best>,
    refusal: Option<RefusalOut>,
}

impl Report {
    fn exit_code(&self) -> u8 {
        match self.outcome {
            Outcome::Match => EXIT_OK,
            Outcome::LowConfidence | Outcome::Refused => EXIT_NO_MATCH,
        }
    }
}

/// Everything after the resolution: run the join, apply the threshold, collect
/// the window.
///
/// The inputs are arguments rather than rediscovered here so this is testable
/// without a `$HOME`, a git repo, or a single file on disk — the same reason
/// [`crate::cmd::sessions::collect`] takes its own.
fn answer(
    query: &Query,
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    sessions: &[SessionMeta],
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Report {
    let window = TimeWindow::around(query.at, query.window);
    let (outcome, best, refusal) = match join::resolve(query, worktrees, sessions, turns) {
        Ok(m) => {
            let confident = m.confident();
            (
                if confident {
                    Outcome::Match
                } else {
                    Outcome::LowConfidence
                },
                Some(best(m, confident)),
                None,
            )
        }
        Err(r) => (
            Outcome::Refused,
            None,
            Some(RefusalOut {
                reason: r.reason(),
                detail: r.detail().to_string(),
            }),
        ),
    };

    Report {
        command: "at",
        schema: SCHEMA,
        repo: repo.to_path_buf(),
        agent: query.agent.to_string(),
        ts: query.at,
        window,
        session_dirs: dirs.to_vec(),
        considered: sessions.len(),
        threshold: CONFIDENT,
        // Filled by `attribute`, which runs after the join: neither is an input
        // to it.
        chain: None,
        model_check: None,
        outcome,
        best,
        refusal,
    }
}

/// The winner, with its transcript window — the join's own
/// [`Match::in_window`], never re-derived here.
///
/// This used to re-open the file and re-apply the window test, and that second
/// opinion is exactly how `at` came to print "no single turn falls inside it"
/// underneath a time-overlap signal the scorer had just awarded. There is one
/// definition of "inside the window" now and it lives in
/// [`crate::join`]; this function only decides whether the reader is allowed
/// to SEE it, which is the threshold question and this command's job.
fn best(m: Match, confident: bool) -> Best {
    Best {
        session: m.session,
        tier: m.tier,
        evidence: m.evidence,
        confident,
        lead: m.lead,
        // Withheld below the bar on purpose — see the module doc. The join
        // found these turns either way; what changes is whether the evidence
        // justifies reading them as testimony.
        transcript: if confident { m.in_window } else { Vec::new() },
    }
}

/// A transcript reader that asks the connector that claimed each path.
///
/// Not "try them in order": both connectors accept `*.jsonl`, so the fallback
/// would answer for a `claude_code` file with a pile of parse failures.
/// [`connector::discover_all`] already recorded who owns what, in
/// [`SessionMeta::connector`].
fn reader<'a>(
    connectors: &'a [Box<dyn Connector>],
    sessions: &'a [SessionMeta],
) -> impl Fn(&Path) -> Vec<Turn> + 'a {
    let owner: HashMap<&Path, &str> = sessions
        .iter()
        .map(|s| (s.path.as_path(), s.connector.as_str()))
        .collect();
    move |path: &Path| {
        owner
            .get(path)
            .and_then(|name| connectors.iter().find(|c| c.name() == *name))
            .map_or_else(Vec::new, |c| c.read(path))
    }
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
/// The sinks are arguments so the split itself can be asserted.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors are ignored on purpose: `recount at --json | head`
        // closing the pipe is not a failure of the command.
        let _ = writeln!(out, "{rendered}");
        let _ = digest(report, err);
    } else {
        let _ = digest(report, out);
    }
    Ok(())
}

fn digest(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    writeln!(w, "recount at — {}", r.repo.display())?;
    writeln!(w, "  agent    `{}`", r.agent)?;
    writeln!(
        w,
        "  moment   {} · window ±{} each side → {}",
        stamp(Some(r.ts)),
        half_width(r),
        fmt_window(&r.window)
    )?;
    writeln!(
        w,
        "  scanned  {} session dir(s) · {} session(s) the window could not rule out",
        r.session_dirs.len(),
        r.considered
    )?;
    chain(r, w)?;

    if let Some(b) = &r.best {
        writeln!(
            w,
            "\n  {}  {}",
            if b.confident {
                "match         "
            } else {
                "best candidate"
            },
            b.session.path.display()
        )?;
        // Three words, not two: a lead is not confident for a different reason
        // than a low score is, and the reader has to be able to tell which
        // without doing arithmetic on the number beside it.
        let verdict = match (b.confident, b.lead) {
            (true, _) => "CONFIDENT",
            (false, true) => "LEAD — NOT CONFIDENT",
            (false, false) => "NOT CONFIDENT",
        };
        // Which tier answered, in the word it serialises as. A keyed answer has
        // no number and no threshold to clear, and printing "confidence keyed ·
        // threshold 0.50" would invite a reader to look for the score that is
        // missing. Nothing is missing: there was nothing to be uncertain about.
        match b.tier.confidence() {
            Some(c) => writeln!(
                w,
                "    tier {} · confidence {c} · threshold {:.2} · {verdict}",
                b.tier.name(),
                r.threshold
            )?,
            None => writeln!(
                w,
                "    tier {} · no score, and none is missing: the event named this \
                 transcript · {verdict}",
                b.tier.name()
            )?,
        }
        for e in &b.evidence {
            writeln!(w, "      · {}: {}", signal_words(e.signal), e.detail)?;
        }
        model_line(r, w)?;
    }

    match r.outcome {
        Outcome::Match => transcript(r, w)?,
        Outcome::LowConfidence
            if r.best
                .as_ref()
                .is_some_and(|b| b.lead && b.tier == Tier::Keyed) =>
        {
            writeln!(
                w,
                "\n  no confident match — the event named this transcript, but the ledger \
                 names more\n  than one agent for the session it belongs to, and the \
                 keyed line above says\n  which. The file is the session's; whether it is \
                 this agent's is what the\n  ledger cannot say. That makes this a LEAD — \
                 somewhere to look, not the\n  answer. The transcript window is withheld: it \
                 would read as testimony the\n  ledger does not support. The path is printed \
                 if you decide otherwise."
            )?
        }
        Outcome::LowConfidence if r.best.as_ref().is_some_and(|b| b.lead) => writeln!(
            w,
            "\n  no confident match — but NOT for either of the spec's two reasons, and \
             NOT\n  because the score fell short: both gates passed and the score is \
             printed above.\n  The chain could not tell this session from its siblings, so \
             nothing structural\n  picked it out; the topology line above says what did, \
             and how well that has\n  held up. That makes this a LEAD — somewhere to look, \
             not the answer. The\n  transcript window is withheld: it would read as \
             testimony the evidence does not\n  support. The path is printed if you decide \
             otherwise."
        )?,
        Outcome::LowConfidence => writeln!(
            w,
            "\n  no confident match — but NOT for either of the spec's two reasons: both \
             gates\n  passed, and what fell short is the score. The evidence above says \
             why.\n  The transcript window is withheld at this confidence: it would read \
             as testimony\n  the evidence does not support. The path is printed if you \
             decide otherwise."
        )?,
        Outcome::Refused => {
            let refusal = r.refusal.as_ref().expect("refused implies a refusal");
            writeln!(w, "\n  no confident match — reason: {}", refusal.reason)?;
            writeln!(w, "    {}", refusal.detail)?;
            writeln!(
                w,
                "  no candidate to show: this is a structural refusal, not a weak match. \
                 Look at\n  the {}.",
                match refusal.reason {
                    "no overlap" => "clock",
                    // Reachable only for an event that named a transcript, which
                    // is why it arrived with this bead: the ids are usable or
                    // they are not, and either way the disk is the next place to
                    // look, not the fleet's shape.
                    "no keyed transcript" =>
                        "transcript store — or at which machine you are standing on",
                    _ => "topology",
                }
            )?;
        }
    }
    Ok(())
}

/// The five links pact stamped on the event, in the order the chain runs:
/// harness, model, session, subagent, branch.
///
/// Nothing at all without an event — see [`Report::chain`]. With one, all five
/// are printed including the absent ones, as `—`: which link pact could not
/// stamp is a fact about the ledger that an auditor is entitled to see, and
/// omitting the empty rows would leave them guessing whether the field was
/// missing or the renderer was.
fn chain(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let Some(c) = &r.chain else { return Ok(()) };
    let or_dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_string());
    writeln!(w, "\n  chain    what pact stamped on the event itself")?;
    writeln!(w, "    harness           {}", or_dash(&c.harness))?;
    writeln!(
        w,
        "    model             {} (declared — the PACT_MODEL the spawner asked for, which \
         is\n                      an intent, not an observation: it is not evidence that \
         this\n                      model answered)",
        or_dash(&c.declared_model)
    )?;
    writeln!(w, "    harness_session   {}", or_dash(&c.harness_session))?;
    writeln!(w, "    harness_subagent  {}", or_dash(&c.harness_subagent))?;
    writeln!(w, "    branch            {}", or_dash(&c.branch))
}

/// Declared against recorded, one line, or nothing at all.
///
/// The `Switched` wording is the decision this bead had to make. A transcript
/// with two real models in it is not a discrepancy and not an agreement: the
/// declaration was honoured for part of the run and something changed the model
/// under it, which is a fact about the run and the most interesting thing this
/// comparison can surface. Folding it into either neighbour would either hide a
/// mid-run switch or accuse an agent that was obeying its declaration.
fn model_line(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let Some(m) = &r.model_check else {
        return Ok(());
    };
    let recorded = m.recorded.join(", then ");
    match m.verdict {
        Verdict::Agrees => writeln!(
            w,
            "      · model: declared {}, session records {recorded} — they agree",
            m.declared
        ),
        Verdict::Switched => writeln!(
            w,
            "      · model: declared {}, session records {recorded} — the run SWITCHED model \
             mid-transcript.\n        Not drift: the declaration held for part of it. The \
             values are most-frequent-first,\n        not chronological, so the order above \
             is not the order it happened in",
            m.declared
        ),
        Verdict::Differs => writeln!(
            w,
            "      · model: declared {}, session records {recorded} — ORCHESTRATION DRIFT. \
             The\n        spawner asked for one model and another answered, for every turn \
             in this\n        transcript",
            m.declared
        ),
    }
}

/// The turns inside the window, or the fact that there are none.
///
/// **Only a KEYED answer can reach the empty branch.** A topological match
/// carries the turns the overlap gate found and that list cannot be empty —
/// it is the gate ([`Match::in_window`]). So an empty window here is never the
/// scorer and the renderer disagreeing about overlap; it is an identity the
/// event NAMED, which never asked the clock anything.
fn transcript(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let turns = r.best.as_ref().map_or(&[][..], |b| &b.transcript);
    if turns.is_empty() {
        writeln!(
            w,
            "\n  transcript window — empty: no turn falls inside {}.\
             \n  The event NAMED this transcript, so the identity does not rest on the clock\
             \n  and nothing here is in doubt — there is only nothing to quote. Widen --window\
             \n  to read what the session was saying around the moment.",
            fmt_window(&r.window)
        )?;
        return Ok(());
    }
    writeln!(
        w,
        "\n  transcript window — {} turn(s) in {}",
        turns.len(),
        fmt_window(&r.window)
    )?;
    for t in turns {
        writeln!(
            w,
            "\n    [{}] {} (line {})",
            stamp(Some(t.ts)),
            t.role,
            t.line
        )?;
        for line in clip(&t.text).lines() {
            writeln!(w, "      {line}")?;
        }
    }
    Ok(())
}

/// One turn's text, cut to [`DIGEST_TURN_CHARS`] on a char boundary, saying so.
fn clip(text: &str) -> String {
    match text.char_indices().nth(DIGEST_TURN_CHARS) {
        None => text.to_string(),
        Some((cut, _)) => format!(
            "{}…\n[+{} more characters — the full text is in --json]",
            &text[..cut],
            text.chars().count() - DIGEST_TURN_CHARS
        ),
    }
}

/// The half-width back out of the window, so the digest can state the
/// semantics rather than leaving `[a, b]` to be reverse-engineered.
fn half_width(r: &Report) -> String {
    let d = r.window.since.map_or(Duration::zero(), |s| r.ts - s);
    let secs = d.num_seconds();
    for (unit, n) in [("w", 604_800), ("d", 86_400), ("h", 3_600), ("m", 60)] {
        if secs >= n && secs % n == 0 {
            return format!("{}{unit}", secs / n);
        }
    }
    format!("{secs}s")
}

fn fmt_window(w: &TimeWindow) -> String {
    format!("[{} … {}]", stamp(w.since), stamp(w.until))
}

fn stamp(t: Option<DateTime<Utc>>) -> String {
    match t {
        Some(t) => t.to_rfc3339_opts(SecondsFormat::Secs, true),
        None => "unbounded".to_string(),
    }
}

/// A [`Signal`] in words, for the digest; the JSON spells the same four via
/// serde. A private copy of `sessions`'s, because it is five lines and sharing
/// it would mean editing a file this bead does not own.
fn signal_words(signal: Signal) -> &'static str {
    match signal {
        Signal::Keyed => "keyed",
        Signal::Topology => "topology",
        Signal::TimeOverlap => "time overlap",
        Signal::TranscriptMention => "transcript mention",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::process::Command;

    use serde_json::Value;
    use tempfile::TempDir;

    use crate::model::{parse_ts, Role};

    const NOON: &str = "2026-03-01T12:00:00Z";

    fn ts(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    fn wt(path: &str, name: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive: true,
            session_dir: PathBuf::from("/nowhere"),
        }
    }

    /// main + one linked worktree, the shape every pact fleet has.
    fn fleet() -> Vec<Worktree> {
        vec![wt("/repo", "main"), wt("/repo/wt/skeleton", "skeleton")]
    }

    fn session(name: &str, cwd: Option<&str>, first: &str, last: &str) -> SessionMeta {
        SessionMeta {
            path: PathBuf::from(name),
            cwd: cwd.map(PathBuf::from),
            first_ts: Some(ts(first)),
            last_ts: Some(ts(last)),
            turn_count: 2,
            parse_failures: 0,
            // recount-hwk.45: a line the connector declined (over the 8 MB cap)
            // is not a line that was damaged. Fixtures have neither.
            declined: 0,
            connector: "claude_code".to_string(),
        }
    }

    fn turn(at: &str, text: &str) -> Turn {
        Turn {
            ts: ts(at),
            role: Role::Assistant,
            text: text.to_string(),
            line: 1,
        }
    }

    /// A pact 0.11.0 row with the whole chain on it. `None` fields are the
    /// pre-0.11.0 and orchestrator shapes.
    fn event(model: Option<&str>, subagent: Option<&str>) -> Event {
        Event {
            line: 1,
            at: ts(NOON),
            agent: "skeleton".to_string(),
            path: Some("src/cmd/at.rs".to_string()),
            invoked_from: Some("skeleton".to_string()),
            detail: Some("recount-1cw.6: the chain".to_string()),
            harness: Some("claude-code".to_string()),
            model: model.map(str::to_string),
            harness_session: Some("cd7c75eb-c39f-4be4-92ae-29efda5d7d69".to_string()),
            harness_subagent: subagent.map(str::to_string),
            branch: Some("wt/atchain".to_string()),
        }
    }

    fn query<'a>(agent: &'a str, at: &str, window: Duration) -> Query<'a> {
        Query {
            agent,
            at: ts(at),
            window,
            invoked_from: None,
            bead: None,
            harness_session: None,
            harness_subagent: None,
            session_agents: &[],
        }
    }

    /// The score behind a topological answer. Panics on a keyed one, which
    /// is the assertion: `at` cannot produce one until recount-1cw.6 gives it
    /// the fields.
    fn score(b: &Best) -> f64 {
        b.tier.confidence().expect("a topological answer").value()
    }

    fn run_join(q: &Query, sessions: &[SessionMeta], turns: Vec<Turn>) -> Report {
        answer(
            q,
            Path::new("/repo"),
            &fleet(),
            &[PathBuf::from("/dir")],
            sessions,
            &|_| turns.clone(),
        )
    }

    fn text_of(r: &Report) -> String {
        let mut buf = Vec::new();
        digest(r, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// Topology A with a mention: the whole point of the command.
    #[test]
    fn a_confident_match_exits_0_with_confidence_evidence_and_the_window() {
        let s = session(
            "/s/skeleton.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let r = run_join(
            &query("skeleton", NOON, Duration::minutes(5)),
            &[s],
            vec![
                turn(
                    "2026-03-01T11:00:00Z",
                    "PACT_AGENT=skeleton, before the window",
                ),
                turn("2026-03-01T12:01:00Z", "wiring the module tree"),
            ],
        );

        assert_eq!(r.outcome, Outcome::Match);
        assert_eq!(r.exit_code(), EXIT_OK);
        let b = r.best.as_ref().unwrap();
        assert!(b.confident);
        // 0.95: topology A, discriminating, plus the mention.
        assert!((score(b) - 0.95).abs() < 1e-9, "{}", b.tier);
        assert!(!b.evidence.is_empty());
        // The window filters the transcript: the 11:00 turn is outside it.
        assert_eq!(b.transcript.len(), 1, "{:?}", b.transcript);
        assert_eq!(b.transcript[0].text, "wiring the module tree");

        let text = text_of(&r);
        assert!(text.contains("confidence 0.95"), "{text}");
        assert!(text.contains("CONFIDENT"), "{text}");
        assert!(text.contains("topology A"), "{text}");
        assert!(text.contains("transcript window — 1 turn(s)"), "{text}");
        assert!(text.contains("wiring the module tree"), "{text}");
    }

    /// A candidate under the bar exits 1 — and prints itself, because its
    /// evidence is the answer to "why not".
    #[test]
    fn a_below_threshold_candidate_exits_1_but_still_shows_its_evidence() {
        // Topology B: the session sits at the repo root, as every subagent
        // fleet's transcripts do, and never names the agent.
        let s = session(
            "/s/parent.jsonl",
            Some("/repo"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let r = run_join(
            &query("skeleton", NOON, Duration::minutes(5)),
            &[s],
            vec![turn("2026-03-01T12:01:00Z", "no identity is stated here")],
        );

        assert_eq!(r.outcome, Outcome::LowConfidence);
        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
        let b = r.best.as_ref().unwrap();
        assert!(!b.confident);
        assert!((score(b) - 0.40).abs() < 1e-9, "{}", b.tier);
        assert!(b.transcript.is_empty(), "the transcript is withheld");
        assert!(r.refusal.is_none(), "a low score is not a Refusal");

        let text = text_of(&r);
        assert!(text.contains("best candidate"), "{text}");
        assert!(text.contains("/s/parent.jsonl"), "{text}");
        assert!(text.contains("confidence 0.40"), "{text}");
        assert!(text.contains("NOT CONFIDENT"), "{text}");
        assert!(text.contains("topology B"), "{text}");
        assert!(text.contains("no mention of `skeleton`"), "{text}");
        // It must not be mistaken for either structural refusal.
        assert!(!text.contains("reason: no cwd match"), "{text}");
        assert!(!text.contains("reason: no overlap"), "{text}");
    }

    /// The first of the spec's two reasons, named.
    #[test]
    fn no_cwd_match_says_no_cwd_match() {
        let s = session(
            "/s/other.jsonl",
            Some("/somewhere/else"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let r = run_join(&query("ghost", NOON, Duration::minutes(5)), &[s], vec![]);

        assert_eq!(r.outcome, Outcome::Refused);
        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
        assert!(r.best.is_none());
        assert_eq!(r.refusal.as_ref().unwrap().reason, "no cwd match");

        let text = text_of(&r);
        assert!(text.contains("reason: no cwd match"), "{text}");
        assert!(text.contains("Look at\n  the topology"), "{text}");
        assert!(!text.contains("no overlap"), "{text}");
    }

    /// The second, and it must not be reported as the first: the cwd did
    /// match, the clock did not.
    #[test]
    fn no_overlap_says_no_overlap() {
        let s = session(
            "/s/yesterday.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-02-28T09:00:00Z",
            "2026-02-28T10:00:00Z",
        );
        let r = run_join(&query("skeleton", NOON, Duration::minutes(5)), &[s], vec![]);

        assert_eq!(r.outcome, Outcome::Refused);
        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
        let refusal = r.refusal.as_ref().unwrap();
        assert_eq!(refusal.reason, "no overlap");
        // The refusal names the session it rejected and the span it covers,
        // which is what makes "look at the clock" actionable.
        assert!(refusal.detail.contains("yesterday.jsonl"), "{refusal:?}");

        let text = text_of(&r);
        assert!(text.contains("reason: no overlap"), "{text}");
        assert!(text.contains("Look at\n  the clock"), "{text}");
    }

    /// `--window` is HALF-WIDTH EACH SIDE, pinned in wave 0
    /// (`TimeWindow::around`). A session four minutes *after* --ts is inside a
    /// 5m window and outside a 1m one — and the second case is "no overlap",
    /// not "no cwd match".
    #[test]
    fn the_window_is_half_width_on_each_side_of_ts() {
        let after = session(
            "/s/after.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T12:04:00Z",
            "2026-03-01T12:04:30Z",
        );
        let before = session(
            "/s/before.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T11:56:00Z",
            "2026-03-01T11:56:30Z",
        );

        for s in [&after, &before] {
            // A turn where the session says it ran. Overlap is a gate on
            // turns, so a fixture with a span and no turn in it refuses —
            // which is what `a_span_that_straddles_the_window_is_not_overlap`
            // is for, not this test.
            let spoke = vec![turn(
                &s.first_ts
                    .unwrap()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                "working",
            )];
            let wide = run_join(
                &query("skeleton", NOON, Duration::minutes(5)),
                std::slice::from_ref(s),
                spoke.clone(),
            );
            assert_eq!(
                wide.window.since.unwrap(),
                ts("2026-03-01T11:55:00Z"),
                "since is ts - window"
            );
            assert_eq!(
                wide.window.until.unwrap(),
                ts("2026-03-01T12:05:00Z"),
                "until is ts + window"
            );
            assert_eq!(
                wide.outcome,
                Outcome::Match,
                "{} is inside ±5m",
                s.path.display()
            );

            let narrow = run_join(
                &query("skeleton", NOON, Duration::minutes(1)),
                std::slice::from_ref(s),
                spoke,
            );
            assert_eq!(narrow.outcome, Outcome::Refused);
            assert_eq!(
                narrow.refusal.as_ref().unwrap().reason,
                "no overlap",
                "{} is outside ±1m",
                s.path.display()
            );
        }

        // And the digest states the semantics rather than implying them.
        let r = run_join(
            &query("skeleton", NOON, Duration::minutes(5)),
            &[after],
            vec![turn("2026-03-01T12:04:00Z", "working")],
        );
        assert!(
            text_of(&r).contains("window ±5m each side"),
            "{}",
            text_of(&r)
        );
    }

    /// The envelope, the stream split, and the fields an audit script reads.
    #[test]
    fn the_json_is_the_contract_and_stdout_is_only_json() {
        let s = session(
            "/s/skeleton.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let r = run_join(
            &query("skeleton", NOON, Duration::minutes(5)),
            &[s],
            vec![turn("2026-03-01T12:01:00Z", "skeleton reporting")],
        );

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, true, &mut out, &mut err).unwrap();
        let v: Value = serde_json::from_slice(&out).expect("stdout is JSON alone");
        assert!(!err.is_empty(), "the digest is printed second, on stderr");

        assert_eq!(v["command"], "at");
        assert_eq!(v["schema"], 1);
        assert_eq!(v["agent"], "skeleton");
        assert_eq!(v["outcome"], "match");
        assert_eq!(v["threshold"], 0.5);
        assert_eq!(v["considered"], 1);
        assert_eq!(v["window"]["since"], "2026-03-01T11:55:00Z");
        assert_eq!(v["best"]["confident"], true);
        assert_eq!(v["best"]["confidence"], 0.95);
        assert!(!v["best"]["evidence"].as_array().unwrap().is_empty());
        assert_eq!(v["best"]["evidence"][0]["signal"], "topology");
        assert_eq!(v["best"]["tier"], "topological");
        // Present and null, never absent — the house rule for this document.
        assert!(v["chain"].is_null(), "no event was passed");
        assert!(v["model_check"].is_null());
        assert_eq!(v["best"]["transcript"][0]["text"], "skeleton reporting");
        assert!(v["refusal"].is_null());

        // Without --json: the digest alone, on stdout, and no JSON anywhere.
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, false, &mut out, &mut err).unwrap();
        assert!(err.is_empty(), "no --json means nothing on stderr");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("recount at — "), "{text}");
        assert!(serde_json::from_str::<Value>(&text).is_err());

        // And the refusal shape, which audit scripts branch on.
        let refused = run_join(&query("ghost", NOON, Duration::minutes(5)), &[], vec![]);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&refused, true, &mut out, &mut err).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["outcome"], "refused");
        assert!(v["best"].is_null());
        assert_eq!(v["refusal"]["reason"], "no cwd match");
        assert!(v["refusal"]["detail"].as_str().unwrap().contains("ghost"));
    }

    /// A long turn is cut in the digest and whole in the JSON.
    #[test]
    fn the_digest_clips_a_turn_the_json_keeps_it() {
        let long = "x".repeat(DIGEST_TURN_CHARS + 40);
        assert!(clip("short").ends_with("short"));
        let clipped = clip(&long);
        assert!(clipped.contains("[+40 more characters"), "{clipped}");
        assert!(clipped.len() < long.len() + 60);
        // Multi-byte, so the cut cannot land mid-character.
        let wide = "é".repeat(DIGEST_TURN_CHARS + 5);
        assert!(clip(&wide).contains("[+5 more characters"));
    }

    /// The header, and the word that keeps it honest.
    #[test]
    fn the_chain_header_names_all_five_links_and_marks_the_model_declared() {
        let s = session(
            "/s/skeleton.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let mut r = run_join(&query("skeleton", NOON, Duration::minutes(5)), &[s], vec![]);
        attribute(
            &mut r,
            Some(&event(Some("claude-opus-5"), Some("a4acccbbc9bfb"))),
        );

        let text = text_of(&r);
        assert!(text.contains("harness           claude-code"), "{text}");
        assert!(
            text.contains("model             claude-opus-5 (declared"),
            "{text}"
        );
        assert!(
            text.contains("harness_session   cd7c75eb-c39f-4be4-92ae-29efda5d7d69"),
            "{text}"
        );
        assert!(text.contains("harness_subagent  a4acccbbc9bfb"), "{text}");
        assert!(text.contains("branch            wt/atchain"), "{text}");
        // The marker is the whole point: an intent must not read as a fact.
        assert!(text.contains("not an observation"), "{text}");

        // And it is in the document under a name that says the same thing.
        let v: Value = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(v["chain"]["declared_model"], "claude-opus-5");
        assert_eq!(v["chain"]["branch"], "wt/atchain");
        assert!(
            v["chain"].get("model").is_none(),
            "the bare word is not a key"
        );

        // A pre-0.11.0 row stamps fewer links, and the header says which.
        let mut bare = run_join(&query("skeleton", NOON, Duration::minutes(5)), &[], vec![]);
        let mut e = event(None, None);
        (e.harness, e.branch) = (None, None);
        attribute(&mut bare, Some(&e));
        let text = text_of(&bare);
        assert!(text.contains("harness           —"), "{text}");
        assert!(text.contains("model             — (declared"), "{text}");
        assert!(text.contains("harness_subagent  —"), "{text}");

        // No event, no header — five dashes would read like a broken ledger.
        let mut none = run_join(&query("skeleton", NOON, Duration::minutes(5)), &[], vec![]);
        attribute(&mut none, None);
        assert!(!text_of(&none).contains("chain "), "{}", text_of(&none));
        let v: Value = serde_json::from_str(&serde_json::to_string(&none).unwrap()).unwrap();
        assert!(v["chain"].is_null());
        assert!(v["model_check"].is_null());
    }

    /// The four hazards from wave 1, each of which invents a discrepancy if the
    /// comparison is a string equality.
    #[test]
    fn the_model_comparison_agrees_switches_differs_or_says_nothing() {
        let v = |ms: &[&str]| ms.iter().map(|m| (*m).to_string()).collect::<Vec<_>>();
        let check = |d: Option<&str>, ms: &[&str]| model_check(d, &v(ms));

        // Agreement, which this fleet's own 35 transcripts produce.
        assert_eq!(
            check(Some("claude-opus-5"), &["claude-opus-5"])
                .unwrap()
                .verdict,
            Verdict::Agrees
        );
        // Hazard 2: a dated slug is the same model, not a discrepancy.
        assert_eq!(
            check(Some("claude-haiku-4-5"), &["claude-haiku-4-5-20251001"])
                .unwrap()
                .verdict,
            Verdict::Agrees
        );
        assert_eq!(undated("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
        // ... and only a real date. `claude-opus-4-8` keeps its tail.
        assert_eq!(undated("claude-opus-4-8"), "claude-opus-4-8");
        assert_eq!(undated("claude-opus-5"), "claude-opus-5");

        // Hazard 1: two real values, declared among them. A mid-run switch,
        // reported as one, in either order — `models` is most-frequent-first,
        // so the declared model is not necessarily models[0].
        for ms in [
            &["claude-fable-5", "claude-opus-4-8"],
            &["claude-opus-4-8", "claude-fable-5"],
        ] {
            let m = check(Some("claude-fable-5"), ms).unwrap();
            assert_eq!(m.verdict, Verdict::Switched);
            assert_eq!(m.recorded.len(), 2, "both sides are named, not just one");
        }

        // Drift: the declared model is nowhere in the transcript.
        let m = check(Some("claude-opus-5"), &["claude-fable-5"]).unwrap();
        assert_eq!(m.verdict, Verdict::Differs);
        assert_eq!(m.declared, "claude-opus-5");

        // Silence, both ways: nothing declared, and nothing recorded. Never an
        // error, and never a line.
        assert!(check(None, &["claude-opus-5"]).is_none());
        assert!(check(Some("claude-opus-5"), &[]).is_none());
        // Hazard 3 is upstream — `<synthetic>` never reaches `models` — but if
        // it ever did it would read as drift, so this pins that it does not
        // arrive here through some other door.
        assert_eq!(
            recorded_models(&session("/nope.jsonl", None, NOON, NOON)),
            Vec::<String>::new(),
            "a path that is not on disk records nothing, and that is silence"
        );

        // The three sentences, in the digest.
        let s = session(
            "/s/skeleton.jsonl",
            Some("/repo/wt/skeleton"),
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let words = |verdict, ms: &[&str]| {
            let mut r = run_join(
                &query("skeleton", NOON, Duration::minutes(5)),
                std::slice::from_ref(&s),
                vec![turn("2026-03-01T12:00:00Z", "working")],
            );
            r.model_check = Some(ModelCheck {
                declared: "claude-opus-5".to_string(),
                recorded: v(ms),
                verdict,
            });
            text_of(&r)
        };
        assert!(words(Verdict::Agrees, &["claude-opus-5"]).contains("they agree"));
        assert!(words(Verdict::Differs, &["claude-fable-5"])
            .contains("declared claude-opus-5, session records claude-fable-5 — ORCHESTRATION"));
        let switched = words(Verdict::Switched, &["claude-opus-5", "claude-fable-5"]);
        assert!(switched.contains("SWITCHED model"), "{switched}");
        assert!(switched.contains("Not drift"), "{switched}");
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .expect("git on PATH");
        assert!(out.status.success(), "git {args:?}");
    }

    /// The whole pipeline against a real tree: real `git worktree list`, the
    /// real path encoding, the real `claude_code` connector, the real join.
    ///
    /// Not through the built binary: `CARGO_BIN_EXE_recount` is defined only
    /// for integration tests, `tests/` is not this bead's file to create, and
    /// `cargo test` does not build `target/<profile>/recount` for a crate with
    /// no integration test to hand it to. The only thing that leaves untested
    /// is `resolve`'s `$HOME` lookup — which belongs to `worktree` and has its
    /// own tests — and pointing `$HOME` at a fixture would race every other
    /// test in this suite that reads the real one.
    #[test]
    fn a_real_tree_answers_with_a_transcript_and_refuses_on_the_clock() {
        let td = TempDir::new().unwrap();
        let root = td.path().canonicalize().unwrap();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "root"]);
        // Topology A, and it has to be a real worktree for the real `git
        // worktree list` to name it: the session's cwd IS `wt/cmdat`, so the
        // chain singles it out. A cwd at the repo ROOT would not — the root is
        // shared by every agent that is not in a worktree, so it names no one
        // and the answer would be a lead however few sessions sit there
        // (`join::Topology::SharedRoot`).
        let wt = repo.join("wt").join("cmdat");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt/cmdat",
                &wt.display().to_string(),
            ],
        );

        // The directory `$HOME/.claude/projects` would hold for this repo,
        // named by the real encoder rather than by hand.
        let dir = root.join("projects").join(worktree::encode(&repo));
        fs::create_dir_all(&dir).unwrap();
        let line = |at: &str, text: &str| {
            format!(
                r#"{{"type":"assistant","timestamp":"{at}","cwd":"{}","message":{{"role":"assistant","model":"claude-opus-5","content":[{{"type":"text","text":{text:?}}}]}}}}"#,
                wt.display()
            )
        };
        fs::write(
            dir.join("11111111-2222-3333-4444-555555555555.jsonl"),
            format!(
                "{}\n{}\n{}\n",
                line("2026-03-01T11:00:00.000Z", "PACT_AGENT=cmdat starting work"),
                line("2026-03-01T11:58:00.000Z", "reading the join module doc"),
                line("2026-03-01T12:02:00.000Z", "the join is topological"),
            ),
        )
        .unwrap();

        // mtime set to the last turn, which is what append-only gives you on a
        // real corpus — and what makes the prune in `decide` actually fire.
        fs::File::options()
            .write(true)
            .open(dir.join("11111111-2222-3333-4444-555555555555.jsonl"))
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::from(ts("2026-03-01T12:02:00Z"))),
            )
            .unwrap();

        let worktrees = worktree::worktrees(&repo).unwrap();
        let dirs = vec![dir];
        let connectors = connector::registry();
        let at = |t: &str| {
            decide(
                &query("cmdat", t, Duration::minutes(5)),
                &repo,
                &worktrees,
                &dirs,
                &connectors,
            )
        };

        let mut hit = at(NOON);
        attribute(&mut hit, Some(&event(Some("claude-opus-5"), None)));
        assert_eq!(hit.outcome, Outcome::Match);
        assert_eq!(hit.exit_code(), EXIT_OK);
        assert_eq!(hit.considered, 1);
        let b = hit.best.as_ref().unwrap();
        // Topology A — the worktree `cmdat` is named after the agent and the
        // session's cwd is exactly it — plus overlap plus the mention the
        // transcript makes: 0.5 + 0.2 + 0.25.
        assert_eq!(score(b), 0.95);
        assert!(!b.lead, "the chain discriminated, so this is not a lead");
        // Only the two turns inside ±5m; the 11:00 one is out.
        assert_eq!(b.transcript.len(), 2, "{:?}", b.transcript);
        let text = text_of(&hit);
        assert!(text.contains("the join is topological"), "{text}");
        assert!(text.contains("CONFIDENT"), "{text}");
        // Which tier answered, in the word `Tier::name` serialises as.
        assert!(
            text.contains("tier topological · confidence 0.95"),
            "{text}"
        );
        // And the comparison, end to end: a real transcript with a real
        // `message.model` in it against a real declared one.
        assert_eq!(
            recorded_models(&hit.best.as_ref().unwrap().session),
            vec!["claude-opus-5".to_string()]
        );
        assert!(
            text.contains("declared claude-opus-5, session records claude-opus-5 — they agree"),
            "{text}"
        );

        // A keyed answer prints its word and no number, because there is none.
        let mut keyed = at(NOON);
        keyed.best.as_mut().unwrap().tier = Tier::Keyed;
        let text = text_of(&keyed);
        assert!(
            text.contains("tier keyed · no score, and none is missing"),
            "{text}"
        );
        assert!(!text.contains("threshold"), "no bar to clear: {text}");

        // A keyed answer the ledger contradicts is a lead, and says so in the
        // keyed tier's own words — not the topological lead's, which would send
        // the reader to a chain that was never consulted.
        let mut contested = keyed;
        contested.outcome = Outcome::LowConfidence;
        let b = contested.best.as_mut().unwrap();
        (b.confident, b.lead) = (false, true);
        let text = text_of(&contested);
        assert!(text.contains("tier keyed"), "{text}");
        assert!(text.contains("LEAD — NOT CONFIDENT"), "{text}");
        assert!(
            text.contains("names more\n  than one agent for the session"),
            "{text}"
        );
        assert!(!text.contains("The chain could not tell"), "{text}");
        assert_eq!(contested.exit_code(), EXIT_NO_MATCH);

        // Below the bar the comparison is withheld with the transcript: the
        // recorded model of a session we do not believe is this agent's would
        // be a false accusation dressed as a finding.
        let mut weak = at(NOON);
        weak.best.as_mut().unwrap().confident = false;
        attribute(&mut weak, Some(&event(Some("claude-fable-5"), None)));
        assert!(weak.model_check.is_none());
        assert!(
            !text_of(&weak).contains("model: declared"),
            "{}",
            text_of(&weak)
        );

        // One hour later the same session is still the only one in the repo,
        // and the honest answer is the clock, not the topology — even though
        // mtime prunes the file unopened on the cheap scan. That is the retry
        // in `decide`: without it this reads "no cwd match", which is false.
        assert_eq!(
            join_over(
                &query("cmdat", "2026-03-01T13:00:00Z", Duration::minutes(5)),
                &repo,
                &worktrees,
                &dirs,
                &connectors,
                &TimeWindow::around(ts("2026-03-01T13:00:00Z"), Duration::minutes(5)),
            )
            .refusal
            .unwrap()
            .reason,
            NO_CWD_MATCH,
            "the cheap scan alone gives the WRONG reason — which is why the retry exists"
        );
        let miss = at("2026-03-01T13:00:00Z");
        assert_eq!(miss.outcome, Outcome::Refused);
        assert_eq!(miss.exit_code(), EXIT_NO_MATCH);
        assert_eq!(miss.refusal.as_ref().unwrap().reason, "no overlap");
        assert!(text_of(&miss).contains("reason: no overlap"));
    }

    /// The bug this bead was filed on, pinned where a human saw it: the digest
    /// awarded a `time overlap` signal and then, two lines below, said "no
    /// single turn falls inside it". The two sentences came from two code
    /// paths and one of them was wrong.
    ///
    /// They are one decision now — [`Match::in_window`], computed by the join
    /// and printed here — so this asserts the pair can never appear together
    /// again, over every shape of transcript that used to produce it.
    #[test]
    fn the_digest_never_claims_overlap_and_an_empty_transcript_window() {
        let long = session(
            "/s/long.jsonl",
            Some("/repo/wt/skeleton"),
            // Thirteen days: straddles every window there is.
            "2026-02-20T00:00:00Z",
            "2026-03-05T00:00:00Z",
        );
        let cases: [(&str, Vec<Turn>); 4] = [
            ("an empty file", vec![]),
            (
                "turns either side of the window and none in it",
                vec![
                    turn("2026-02-21T09:00:00Z", "days before"),
                    turn("2026-03-04T09:00:00Z", "days after"),
                ],
            ),
            (
                "one turn inside",
                vec![turn("2026-03-01T12:00:30Z", "here")],
            ),
            (
                "one inside, two out",
                vec![
                    turn("2026-02-21T09:00:00Z", "before"),
                    turn("2026-03-01T11:59:00Z", "skeleton is here"),
                    turn("2026-03-04T09:00:00Z", "after"),
                ],
            ),
        ];
        for (what, turns) in cases {
            let r = run_join(
                &query("skeleton", NOON, Duration::minutes(5)),
                std::slice::from_ref(&long),
                turns,
            );
            let text = text_of(&r);
            assert!(
                !(text.contains("time overlap:") && text.contains("transcript window — empty")),
                "{what}: the scorer and the renderer disagree again\n{text}"
            );
            // And the refusal, when it is one, is the clock's and says so.
            if r.outcome == Outcome::Refused {
                assert_eq!(r.refusal.as_ref().unwrap().reason, "no overlap", "{what}");
            }
        }
    }

    /// Against this machine's real corpus, if it has one — through the real
    /// `resolve`, `$HOME` and all. Asserts no count and no verdict: the corpus
    /// shrank by 14 files in under three hours while this was written. What is
    /// asserted is that a real tree yields one of the three outcomes, an exit
    /// code that agrees with it, evidence or a reason behind it, and no panic.
    #[test]
    fn the_real_corpus_produces_an_answer_or_a_reasoned_refusal() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        if !home.join(".claude/projects").is_dir() {
            eprintln!("skipped: no ~/.claude/projects on this machine");
            return;
        }
        let args = AtArgs {
            repo: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            agent: "cmdat".to_string(),
            ts: ts("2026-08-19T12:00:00Z"),
            window: Duration::hours(12),
        };
        let r = resolve(&args, None, None).expect("a real repo resolves");
        let v: Value = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(v["command"], "at");

        match r.outcome {
            Outcome::Match => {
                assert_eq!(r.exit_code(), EXIT_OK);
                assert!(!r.best.as_ref().unwrap().evidence.is_empty());
            }
            Outcome::LowConfidence => {
                assert_eq!(r.exit_code(), EXIT_NO_MATCH);
                let b = r.best.as_ref().unwrap();
                assert!(!b.evidence.is_empty());
                assert!(b.transcript.is_empty());
            }
            Outcome::Refused => {
                assert_eq!(r.exit_code(), EXIT_NO_MATCH);
                let reason = r.refusal.as_ref().unwrap().reason;
                assert!(["no cwd match", "no overlap"].contains(&reason), "{reason}");
            }
        }
        // Whichever way it went, the digest never states a session without a
        // confidence beside it.
        let text = text_of(&r);
        assert!(
            text.contains("confidence ") || text.contains("no confident match"),
            "{text}"
        );
        // …and never awards overlap over an empty transcript window, on real
        // files rather than fixtures.
        assert!(
            !(text.contains("time overlap:") && text.contains("transcript window — empty")),
            "{text}"
        );
    }
}
