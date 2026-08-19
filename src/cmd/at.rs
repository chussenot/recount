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
use crate::model::{Confidence, Evidence, SessionMeta, Signal, TimeWindow, Turn};
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

/// `at`, plus the two [`Query`] fields the CLI has no flag for.
///
/// `explain` calls this rather than [`run`]: a pact event carries an
/// `invoked_from` and often a bead id in its `detail`, and those are the first
/// and third links of the join. Passing `None, None` — which is what [`run`]
/// does — is not a lesser join, it is the one `at`'s flags can express: the
/// chain then falls back to "is there a worktree named after the agent", and
/// says so in its evidence.
pub fn run_with(
    args: &AtArgs,
    invoked_from: Option<&str>,
    bead: Option<&str>,
    json: bool,
) -> Result<u8> {
    let report = resolve(args, invoked_from, bead)?;
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
fn resolve(args: &AtArgs, invoked_from: Option<&str>, bead: Option<&str>) -> Result<Report> {
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

    let query = Query {
        agent: &args.agent,
        at: args.ts,
        window: args.window,
        invoked_from,
        bead,
    };
    Ok(decide(
        &query,
        &repo,
        &worktrees,
        &dirs,
        &connector::registry(),
    ))
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
    confidence: Confidence,
    /// Never empty — [`join::Match`] cannot be built without it.
    evidence: Vec<Evidence>,
    /// `confidence >= CONFIDENT`. Duplicated from the comparison so a reader of
    /// the JSON never has to know the threshold to know the verdict.
    confident: bool,
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
            let confident = m.confidence.value() >= CONFIDENT;
            (
                if confident {
                    Outcome::Match
                } else {
                    Outcome::LowConfidence
                },
                Some(best(m, confident, &window, turns)),
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
        outcome,
        best,
        refusal,
    }
}

/// The winner, with its transcript window — read only when it is going to be
/// printed. This re-opens the one file the join already read for the mention
/// tiebreaker; one extra read of one file is cheaper than threading turns back
/// out of the scorer for the candidates that lost.
fn best(
    m: Match,
    confident: bool,
    window: &TimeWindow,
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Best {
    let transcript = if confident {
        turns(&m.session.path)
            .into_iter()
            .filter(|t| window.contains(t.ts))
            .collect()
    } else {
        Vec::new()
    };
    Best {
        session: m.session,
        confidence: m.confidence,
        evidence: m.evidence,
        confident,
        transcript,
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
        writeln!(
            w,
            "    confidence {} · threshold {:.2} · {}",
            b.confidence,
            r.threshold,
            if b.confident {
                "CONFIDENT"
            } else {
                "NOT CONFIDENT"
            }
        )?;
        for e in &b.evidence {
            writeln!(w, "      · {}: {}", signal_words(e.signal), e.detail)?;
        }
    }

    match r.outcome {
        Outcome::Match => transcript(r, w)?,
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
                    _ => "topology",
                }
            )?;
        }
    }
    Ok(())
}

/// The turns inside the window, or the fact that there are none.
fn transcript(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let turns = r.best.as_ref().map_or(&[][..], |b| &b.transcript);
    if turns.is_empty() {
        writeln!(
            w,
            "\n  transcript window — empty: the session's span overlaps {}, \
             but no single turn\n  falls inside it. Widen --window.",
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

/// A [`Signal`] in words, for the digest; the JSON spells the same three via
/// serde. A private copy of `sessions`'s, because it is five lines and sharing
/// it would mean editing a file this bead does not own.
fn signal_words(signal: Signal) -> &'static str {
    match signal {
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

    fn query<'a>(agent: &'a str, at: &str, window: Duration) -> Query<'a> {
        Query {
            agent,
            at: ts(at),
            window,
            invoked_from: None,
            bead: None,
        }
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
        assert!(
            (b.confidence.value() - 0.95).abs() < 1e-9,
            "{}",
            b.confidence
        );
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
        assert!(
            (b.confidence.value() - 0.40).abs() < 1e-9,
            "{}",
            b.confidence
        );
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
            let wide = run_join(
                &query("skeleton", NOON, Duration::minutes(5)),
                std::slice::from_ref(s),
                vec![],
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
                vec![],
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
            vec![],
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

        // The directory `$HOME/.claude/projects` would hold for this repo,
        // named by the real encoder rather than by hand.
        let dir = root.join("projects").join(worktree::encode(&repo));
        fs::create_dir_all(&dir).unwrap();
        let line = |at: &str, text: &str| {
            format!(
                r#"{{"type":"assistant","timestamp":"{at}","cwd":"{}","message":{{"role":"assistant","content":[{{"type":"text","text":{text:?}}}]}}}}"#,
                repo.display()
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

        let hit = at(NOON);
        assert_eq!(hit.outcome, Outcome::Match);
        assert_eq!(hit.exit_code(), EXIT_OK);
        assert_eq!(hit.considered, 1);
        let b = hit.best.as_ref().unwrap();
        // Topology unnamed — no worktree is called `cmdat` — plus overlap plus
        // the mention the transcript makes: 0.2 + 0.2 + 0.25.
        assert_eq!(b.confidence.value(), 0.65);
        // Only the two turns inside ±5m; the 11:00 one is out.
        assert_eq!(b.transcript.len(), 2, "{:?}", b.transcript);
        let text = text_of(&hit);
        assert!(text.contains("the join is topological"), "{text}");
        assert!(text.contains("CONFIDENT"), "{text}");

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
    }
}
