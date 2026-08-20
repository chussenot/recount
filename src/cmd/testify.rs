//! `pact audit --json | recount testify --repo .`
//!
//! An audit finding goes in; the reason why comes out. This is the command the
//! rest of recount exists to make possible: [`crate::cmd::explain`] answers one
//! ledger line at a time from a number a human typed, and `testify` answers
//! every finding a whole audit produced, in one pass, without anybody reading a
//! line number off a screen.
//!
//! # It annotates, it does not summarise
//!
//! Every finding comes back out **verbatim**, under `finding`, with the
//! testimony attached beside it. That is what makes this a pipeline stage
//! rather than a report: whatever the caller was going to do with `pact audit
//! --json` it can still do, and `.findings[].testimony` is simply there now.
//! Dropping fields recount does not understand would make this the last stage
//! any pipeline could have.
//!
//! # Deriving the shape rather than declaring it
//!
//! pact's audit document has no `findings` key. Each check writes its own
//! array — `double_wins`, `stale_holds`, `topology_mismatches`, twelve of them
//! today — and the element shapes differ per check: a double-win names
//! `incoming_agent` and `incoming_at`, a stale hold names `agent` and
//! `opened_at`, a topology mismatch names neither.
//!
//! So the reader is structural, not schematic. Every top-level array is a
//! finding list unless it is one of the four statistics arrays in
//! [`NOT_FINDINGS`], and inside an element the agent and the timestamp are
//! found by *name shape* — `agent`, `*_agent`, a short alias list taken from
//! pact's own structs; `at`, `*_at`, `since` — rather than by a per-check
//! table. A check pact adds tomorrow is testified without a change here, which
//! is the same tolerance [`crate::pact_events`] applies to the ledger itself.
//!
//! A finding carrying no agent/timestamp pair is **not an error**: it is passed
//! through untouched and counted, because a topology mismatch is a real finding
//! about a fleet and not about any one agent, and there is no session to name.
//!
//! # Exit codes
//!
//! `0` when every finding that could be testified was, `1` when some could not
//! — and then `untestified` lists exactly which, because an exit code with no
//! list behind it sends the reader back to the whole document. An **empty**
//! findings set is `0`: a clean audit is not a failure. Malformed input is a
//! hard error — one sentence on stderr, no document at all.
//!
//! # Output
//!
//! The house envelope, `{command, schema, …payload}`; with `--json` the
//! document alone on stdout and the digest second on stderr, without it the
//! digest alone on stdout.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::cli::TestifyArgs;
use crate::cmd::{EXIT_NO_MATCH, EXIT_OK};
use crate::connector::{self, Connector};
use crate::join::{self, Query, CONFIDENT};
use crate::model::{Evidence, SessionMeta, Signal, Tier, TimeWindow, Turn};
use crate::pact_events::{self, Event, Events};
use crate::worktree::{self, Worktree};

/// New command, so it starts here. Bumped only when a field leaves or changes
/// meaning.
const SCHEMA: u32 = 1;

/// The digest quotes a turn up to this many characters, as `at` does. The full
/// text is in the JSON.
const DIGEST_TURN_CHARS: usize = 600;

/// Top-level arrays of the audit document that are statistics, not findings.
///
/// The list is a denylist rather than an allowlist on purpose: pact grows
/// checks — nine today, twelve arrays — and an allowlist would silently pass
/// every new one through as untestifiable. These four are the whole of what
/// `pact audit --json` writes as an array that is not a finding, and each is a
/// tally over the log rather than something an agent did at a moment.
const NOT_FINDINGS: &[&str] = &["agents", "annotations", "per_agent", "top_contended"];

/// Keys naming the agent, in the order they are preferred, before the generic
/// `*_agent` sweep. Taken from pact's own finding structs: `incoming_agent`
/// (double-win), `refused_agent` (silent-contention), `first_agent`
/// (concurrent-write), `acquired_by`/`released_by` (merge-divergence),
/// `holder` (cross-held-commit), `author` (uncovered-commit).
const AGENT_ALIASES: &[&str] = &[
    "incoming_agent",
    "refused_agent",
    "committer_agent",
    "first_agent",
    "acquired_by",
    "released_by",
    "holder",
    "author",
];

/// Keys naming the moment, in preference order, before the generic `*_at`
/// sweep.
///
/// Order matters and is not alphabetical: a stale hold carries both `opened_at`
/// and `closed_at`, and the moment worth reading a transcript around is when
/// the agent *took* the lease — the decision — not when the sweep closed it.
/// An alphabetical sweep would pick `closed_at` and quietly testify the wrong
/// minute.
const TS_ALIASES: &[&str] = &[
    "opened_at",
    "acquired_at",
    "incoming_at",
    "refused_at",
    "first_opened_at",
    "hold_opened_at",
    "released_at",
    "since",
];

/// Keys naming the ledger line the finding came from, in preference order.
/// A line reaches the whole attribution chain — and with it the keyed tier.
const LINE_ALIASES: &[&str] = &["incoming_line", "opened_line", "since_line"];

pub fn run(args: &TestifyArgs, json: bool) -> Result<u8> {
    let (source, text) = input(args.file.as_deref(), &mut io::stdin().lock())?;
    let doc: Value = serde_json::from_str(&text).with_context(|| {
        format!("{source} is not JSON — testify reads a `pact audit --json` document")
    })?;

    let report = resolve(args, &doc, source)?;
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(report.exit_code())
}

/// The document to testify, and what to call where it came from.
///
/// Separate from [`run`] so both halves of "`--file` and stdin are the same
/// input" can be asserted without a subprocess.
fn input(file: Option<&Path>, stdin: &mut dyn Read) -> Result<(String, String)> {
    match file {
        Some(p) => {
            let text =
                std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
            Ok((p.display().to_string(), text))
        }
        None => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .context("reading the audit document from stdin")?;
            Ok(("stdin".to_string(), text))
        }
    }
}

/// Discover once, join per finding. Prints nothing, for the same reason
/// `at::resolve` prints nothing: a test cannot read a report the command has
/// already written to the process's own stdout.
fn resolve(args: &TestifyArgs, doc: &Value, source: String) -> Result<Report> {
    let worktrees = worktree::worktrees(&args.repo)?;
    let dirs: Vec<PathBuf> = worktree::session_dirs(&args.repo)?
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let repo = worktrees
        .first()
        .map_or_else(|| args.repo.clone(), |w| w.path.clone());

    // Unwindowed, and once: the findings of one audit span the whole run, so
    // there is no single window to prune by, and re-discovering per finding
    // would re-read the corpus N times to reach the same set.
    let connectors = connector::registry();
    let sessions = if dirs.is_empty() {
        Vec::new()
    } else {
        connector::discover_all(&connectors, &dirs, &TimeWindow::default())
    };

    // The ledger is read at most once, and its absence is not an error: a
    // finding that names no line never needed it, and an audit mailed from
    // another machine legitimately arrives without one.
    let ledger = pact_events::read(&pact_events::events_path(&args.repo)).ok();
    let turns = reader(&connectors, &sessions);

    Ok(annotate(
        args,
        doc,
        source,
        &repo,
        &worktrees,
        &dirs,
        &sessions,
        ledger.as_ref(),
        &turns,
    ))
}

/// Everything after the discovery: walk the findings, join each one, count.
///
/// Takes its world as arguments so it is drivable over a fixture with no
/// `$HOME`, no git repo and no file on disk — the same reason
/// `at::answer` takes its own.
#[allow(clippy::too_many_arguments)]
fn annotate(
    args: &TestifyArgs,
    doc: &Value,
    source: String,
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    sessions: &[SessionMeta],
    ledger: Option<&Events>,
    turns: &dyn Fn(&Path) -> Vec<Turn>,
) -> Report {
    let mut out = Vec::new();
    let mut untestified = Vec::new();
    let (mut testified, mut passed_through) = (0usize, 0usize);

    for raw in raw_findings(doc) {
        let where_ = format!("{}[{}]", raw.check, raw.index);
        let Some((agent, ts)) = raw.agent_and_ts() else {
            passed_through += 1;
            out.push(Testimony::passed_through(raw));
            continue;
        };
        let event = raw.event_line().and_then(|n| ledger?.line(n).ok());
        let bead = event.and_then(bead);
        let query = Query {
            agent: &agent,
            at: ts,
            window: args.window,
            invoked_from: event.and_then(|e| e.invoked_from.as_deref()),
            bead,
            harness_session: event.and_then(|e| e.harness_session.as_deref()),
            harness_subagent: event.and_then(|e| e.harness_subagent.as_deref()),
        };
        let window = TimeWindow::around(ts, args.window);
        let t = match join::resolve(&query, worktrees, sessions, turns) {
            Ok(m) if m.tier.clears(CONFIDENT) => {
                testified += 1;
                Testimony::matched(&raw, &agent, ts, &window, m)
            }
            Ok(m) => {
                untestified.push(format!(
                    "{where_} agent `{agent}` at {} — below the bar: tier {} ({})",
                    stamp(ts),
                    m.tier,
                    m.session.path.display()
                ));
                Testimony::low_confidence(&raw, &agent, ts, &window, m)
            }
            Err(r) => {
                untestified.push(format!(
                    "{where_} agent `{agent}` at {} — {}: {}",
                    stamp(ts),
                    r.reason(),
                    r.detail()
                ));
                Testimony::refused(&raw, &agent, ts, &window, r.reason(), r.detail())
            }
        };
        out.push(t);
    }

    Report {
        command: "testify",
        schema: SCHEMA,
        repo: repo.to_path_buf(),
        source,
        check: doc.get("check").and_then(Value::as_str).map(str::to_string),
        window_secs: args.window.num_seconds(),
        session_dirs: dirs.to_vec(),
        considered: sessions.len(),
        threshold: CONFIDENT,
        totals: Totals {
            findings: out.len(),
            testified,
            untestified: untestified.len(),
            passed_through,
        },
        findings: out,
        untestified,
    }
}

/// One element of one of the audit document's finding arrays, still verbatim.
struct Raw {
    /// The key it came out of — `stale_holds`, `double_wins`, … — so a reader
    /// can put an annotation back where it belongs.
    check: String,
    index: usize,
    value: Value,
}

impl Raw {
    fn object(&self) -> Option<&Map<String, Value>> {
        self.value.as_object()
    }

    /// The two things a join needs, or nothing. Both or neither: an agent
    /// without a moment cannot be placed in time and a moment without an agent
    /// cannot be placed at all.
    fn agent_and_ts(&self) -> Option<(String, DateTime<Utc>)> {
        let o = self.object()?;
        let agent = pick(o, "agent", "_agent", AGENT_ALIASES, |v| {
            v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
        })?;
        let ts = pick(o, "at", "_at", TS_ALIASES, |v| {
            DateTime::parse_from_rfc3339(v.as_str()?)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        })?;
        Some((agent, ts))
    }

    fn event_line(&self) -> Option<usize> {
        let o = self.object()?;
        pick(o, "line", "_line", LINE_ALIASES, |v| {
            usize::try_from(v.as_u64()?).ok().filter(|n| *n > 0)
        })
    }
}

/// The first value this finding gives up for a field, trying the exact key,
/// then the aliases in the order pact's structs suggest, then every key with
/// the suffix in the map's own (sorted) order.
///
/// `get` both extracts and validates: a key of the right *name* holding the
/// wrong *type* — `"at": 17`, `"agent": null` — falls through to the next
/// candidate rather than aborting the finding.
fn pick<T>(
    o: &Map<String, Value>,
    exact: &str,
    suffix: &str,
    aliases: &[&str],
    get: impl Fn(&Value) -> Option<T>,
) -> Option<T> {
    std::iter::once(exact)
        .chain(aliases.iter().copied())
        .filter_map(|k| o.get(k))
        .chain(
            o.iter()
                .filter(|(k, _)| k.ends_with(suffix))
                .map(|(_, v)| v),
        )
        .find_map(get)
}

/// Every finding in the document, in a stable order.
///
/// Three input shapes, because all three turn up: the whole `pact audit --json`
/// object; a bare array, which is what `jq '.stale_holds'` hands over; and a
/// single finding object, which is what `jq '.stale_holds[0]'` hands over.
fn raw_findings(doc: &Value) -> Vec<Raw> {
    match doc {
        Value::Array(items) => numbered("findings", items),
        Value::Object(o) => o
            .iter()
            .filter(|(k, _)| !NOT_FINDINGS.contains(&k.as_str()))
            .filter_map(|(k, v)| Some(numbered(k, v.as_array()?)))
            .flatten()
            .collect(),
        _ => Vec::new(),
    }
}

fn numbered(check: &str, items: &[Value]) -> Vec<Raw> {
    items
        .iter()
        .enumerate()
        .map(|(index, value)| Raw {
            check: check.to_string(),
            index,
            value: value.clone(),
        })
        .collect()
}

/// The bead id a ledger row's `detail` opens with, if it opens with one — the
/// transcript-mention tiebreaker's needle. The same reading
/// [`crate::cmd::explain`] makes of the same field; anything that is not a
/// single bare token before a colon is not an id.
fn bead(e: &Event) -> Option<&str> {
    let id = e.detail.as_deref()?.split_once(':')?.0;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

/// A transcript reader that asks the connector that claimed each path — the
/// same rule `at` uses, and for the same reason: both connectors accept
/// `*.jsonl`, so "try them in order" would answer for a `claude_code` file with
/// a pile of parse failures.
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

/// What happened to one finding. `passed_through` is the fourth outcome and the
/// one that is not a verdict: nothing was asked, so nothing was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Match,
    LowConfidence,
    Refused,
    PassedThrough,
}

/// The session recount named, and the evidence that named it.
#[derive(Debug, Clone, Serialize)]
struct Testified {
    session: SessionMeta,
    /// `{"tier": …, "confidence": …}`, flattened as everywhere else: a keyed
    /// answer writes an explicit `null` where the number would be.
    #[serde(flatten)]
    tier: Tier,
    confident: bool,
    evidence: Vec<Evidence>,
    /// The window the transcript was cut to: `ts ± --window`.
    window: TimeWindow,
    /// Turns inside it. Empty unless `confident`.
    transcript: Vec<Turn>,
}

#[derive(Debug, Clone, Serialize)]
struct RefusalOut {
    reason: &'static str,
    detail: String,
}

/// One finding, back out, annotated.
#[derive(Debug, Clone, Serialize)]
struct Testimony {
    check: String,
    index: usize,
    /// `null` on a pass-through: the finding named nobody.
    agent: Option<String>,
    ts: Option<DateTime<Utc>>,
    /// The ledger line the finding pointed at, when it pointed at one. That
    /// line is what reaches the attribution chain and the keyed tier, and it is
    /// also the argument for `recount explain --event-line`.
    event_line: Option<usize>,
    outcome: Outcome,
    testimony: Option<Testified>,
    refusal: Option<RefusalOut>,
    /// The finding as pact wrote it, unchanged. See the module doc.
    finding: Value,
}

impl Testimony {
    fn shell(raw: &Raw, agent: &str, ts: DateTime<Utc>, outcome: Outcome) -> Self {
        Testimony {
            check: raw.check.clone(),
            index: raw.index,
            agent: Some(agent.to_string()),
            ts: Some(ts),
            event_line: raw.event_line(),
            outcome,
            testimony: None,
            refusal: None,
            finding: raw.value.clone(),
        }
    }

    fn passed_through(raw: Raw) -> Self {
        Testimony {
            check: raw.check,
            index: raw.index,
            agent: None,
            ts: None,
            event_line: None,
            outcome: Outcome::PassedThrough,
            testimony: None,
            refusal: None,
            finding: raw.value,
        }
    }

    /// The transcript is the join's own [`join::Match::in_window`], not a
    /// second pass over the file: one definition of "inside the window", in
    /// `join`, or the scorer and this renderer drift apart again.
    fn matched(
        raw: &Raw,
        agent: &str,
        ts: DateTime<Utc>,
        window: &TimeWindow,
        m: join::Match,
    ) -> Self {
        let mut t = Self::shell(raw, agent, ts, Outcome::Match);
        t.testimony = Some(Testified {
            session: m.session,
            tier: m.tier,
            confident: true,
            evidence: m.evidence,
            window: *window,
            transcript: m.in_window,
        });
        t
    }

    /// A candidate under the bar is printed with its evidence and **without**
    /// its transcript, exactly as `at` does: a sub-threshold candidate's
    /// transcript is *a* transcript, not *the* one, and printing it under a
    /// heading that reads like testimony is the misdiagnosis recount exists to
    /// prevent.
    fn low_confidence(
        raw: &Raw,
        agent: &str,
        ts: DateTime<Utc>,
        window: &TimeWindow,
        m: join::Match,
    ) -> Self {
        let mut t = Self::shell(raw, agent, ts, Outcome::LowConfidence);
        t.testimony = Some(Testified {
            session: m.session,
            tier: m.tier,
            confident: false,
            evidence: m.evidence,
            window: *window,
            transcript: Vec::new(),
        });
        t
    }

    fn refused(
        raw: &Raw,
        agent: &str,
        ts: DateTime<Utc>,
        _window: &TimeWindow,
        reason: &'static str,
        detail: &str,
    ) -> Self {
        let mut t = Self::shell(raw, agent, ts, Outcome::Refused);
        t.refusal = Some(RefusalOut {
            reason,
            detail: detail.to_string(),
        });
        t
    }
}

#[derive(Debug, Clone, Serialize)]
struct Totals {
    findings: usize,
    testified: usize,
    untestified: usize,
    /// Findings carrying no agent/timestamp pair. Not a failure — see the
    /// module doc.
    passed_through: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Report {
    command: &'static str,
    schema: u32,
    repo: PathBuf,
    /// `stdin`, or the `--file` path.
    source: String,
    /// The audit document's own `check` field, when it had one. `null` for a
    /// summary document, which carries no findings arrays at all.
    check: Option<String>,
    /// `--window`, the half-width each side of a finding's moment.
    window_secs: i64,
    session_dirs: Vec<PathBuf>,
    /// Sessions discovered across those directories. Unwindowed — see
    /// [`resolve`].
    considered: usize,
    threshold: f64,
    totals: Totals,
    findings: Vec<Testimony>,
    /// One line per finding that had an agent and a moment and still got no
    /// confident match. The exit code is 1 if and only if this is non-empty.
    untestified: Vec<String>,
}

impl Report {
    fn exit_code(&self) -> u8 {
        if self.untestified.is_empty() {
            EXIT_OK
        } else {
            EXIT_NO_MATCH
        }
    }
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
/// One digest function, two sinks — `tests/degradation.rs` asserts
/// `json.stderr == plain.stdout` byte for byte, and a second renderer is how
/// that stops being true.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors ignored on purpose: a closed pipe is not a failed
        // command.
        let _ = writeln!(out, "{rendered}");
        let _ = digest(report, err);
    } else {
        let _ = digest(report, out);
    }
    Ok(())
}

fn digest(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    writeln!(w, "recount testify — {}", r.repo.display())?;
    writeln!(w, "  source   {}", r.source)?;
    writeln!(
        w,
        "  input    {}",
        match &r.check {
            Some(c) => format!("pact audit --check {c}"),
            None => "pact audit summary (no --check)".to_string(),
        }
    )?;
    writeln!(
        w,
        "  scanned  {} session dir(s) · {} session(s) · window ±{}s each side",
        r.session_dirs.len(),
        r.considered,
        r.window_secs
    )?;
    writeln!(
        w,
        "  findings {} · testified {} · untestified {} · passed through {}",
        r.totals.findings, r.totals.testified, r.totals.untestified, r.totals.passed_through
    )?;

    if r.totals.findings == 0 {
        writeln!(
            w,
            "\n  nothing to testify: the audit found nothing. A clean audit is not a failure."
        )?;
        return Ok(());
    }

    for t in &r.findings {
        one(t, r.threshold, w)?;
    }

    if !r.untestified.is_empty() {
        writeln!(
            w,
            "\n  {} finding(s) could not be testified:",
            r.untestified.len()
        )?;
        for line in &r.untestified {
            writeln!(w, "    {line}")?;
        }
    }
    Ok(())
}

fn one(t: &Testimony, threshold: f64, w: &mut dyn Write) -> io::Result<()> {
    write!(w, "\n  {}[{}]", t.check, t.index)?;
    match (&t.agent, t.ts) {
        (Some(a), Some(ts)) => write!(w, " — agent `{a}` at {}", stamp(ts))?,
        _ => write!(w, " — no agent and no moment: passed through untouched")?,
    }
    match t.event_line {
        Some(n) => writeln!(w, " (ledger line {n})")?,
        None => writeln!(w)?,
    }

    if let Some(rf) = &t.refusal {
        writeln!(w, "    NO TESTIMONY — {}: {}", rf.reason, rf.detail)?;
    }
    let Some(d) = &t.testimony else { return Ok(()) };

    writeln!(
        w,
        "    tier {}{} · {}",
        d.tier,
        if d.confident {
            String::new()
        } else {
            format!(" — BELOW {threshold}, no transcript")
        },
        d.session.path.display()
    )?;
    for e in &d.evidence {
        writeln!(w, "      · {}: {}", signal_words(e.signal), e.detail)?;
    }
    if !d.confident {
        return Ok(());
    }
    // Reachable only for a KEYED testimony: a topological match's transcript is
    // the overlap gate's own turn list, and the gate is that it is non-empty.
    if d.transcript.is_empty() {
        writeln!(
            w,
            "    transcript window — empty: no turn falls inside {}. The event NAMED this \
             transcript, so nothing is in doubt — there is nothing to quote. Widen --window.",
            fmt_window(&d.window)
        )?;
        return Ok(());
    }
    writeln!(
        w,
        "    transcript window — {} turn(s) in {}",
        d.transcript.len(),
        fmt_window(&d.window)
    )?;
    for turn in &d.transcript {
        writeln!(
            w,
            "\n      [{}] {} (line {})",
            stamp(turn.ts),
            turn.role,
            turn.line
        )?;
        for line in clip(&turn.text).lines() {
            writeln!(w, "        {line}")?;
        }
    }
    Ok(())
}

/// One turn's text, cut on a char boundary, saying so — the same clip `at`
/// applies, and for the same reason: one `tool_result` can be megabytes.
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

/// A [`Signal`] in words, for the digest; the JSON spells the same four via
/// serde. A private copy of `at`'s, for the reason `at`'s is a private copy of
/// `sessions`'s: it is five lines, and sharing it would mean editing a file
/// this bead does not own.
fn signal_words(signal: Signal) -> &'static str {
    match signal {
        Signal::Keyed => "keyed",
        Signal::Topology => "topology",
        Signal::TimeOverlap => "time overlap",
        Signal::TranscriptMention => "transcript mention",
    }
}

fn fmt_window(w: &TimeWindow) -> String {
    format!(
        "[{} … {}]",
        w.since.map_or_else(|| "—".to_string(), stamp),
        w.until.map_or_else(|| "—".to_string(), stamp)
    )
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use chrono::Duration;

    use crate::model::{parse_ts, Role};

    const NOON: &str = "2026-03-01T12:00:00Z";

    fn ts(s: &str) -> DateTime<Utc> {
        parse_ts(s).unwrap()
    }

    fn args() -> TestifyArgs {
        TestifyArgs {
            repo: PathBuf::from("/repo"),
            file: None,
            window: Duration::minutes(5),
        }
    }

    fn wt(path: &str, name: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive: true,
            session_dir: PathBuf::from("/nowhere"),
        }
    }

    fn fleet() -> Vec<Worktree> {
        vec![wt("/repo", "main"), wt("/repo/wt/skeleton", "skeleton")]
    }

    fn session(path: &str, cwd: &str, first: &str, last: &str) -> SessionMeta {
        SessionMeta {
            path: PathBuf::from(path),
            cwd: Some(PathBuf::from(cwd)),
            first_ts: Some(ts(first)),
            last_ts: Some(ts(last)),
            turn_count: 2,
            parse_failures: 0,
            declined: 0,
            connector: "claude_code".to_string(),
        }
    }

    /// A session that overlaps noon and sits in the `skeleton` worktree: the
    /// shape a finding about `skeleton` at noon should land on.
    fn skeleton_session() -> SessionMeta {
        session(
            "/s/skeleton.jsonl",
            "/repo/wt/skeleton",
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        )
    }

    fn turn(at: &str, text: &str) -> Turn {
        Turn {
            ts: ts(at),
            role: Role::Assistant,
            text: text.to_string(),
            line: 1,
        }
    }

    fn run_over(doc: &Value, sessions: &[SessionMeta], turns: Vec<Turn>) -> Report {
        annotate(
            &args(),
            doc,
            "stdin".to_string(),
            Path::new("/repo"),
            &fleet(),
            &[PathBuf::from("/dir")],
            sessions,
            None,
            &|_| turns.clone(),
        )
    }

    fn text_of(r: &Report) -> String {
        let mut buf = Vec::new();
        digest(r, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    /// A canned `pact audit --check stale-holds --json`, whose one finding is
    /// about an agent whose session is right there: the whole command, working.
    #[test]
    fn a_finding_that_testifies_exits_0_and_carries_the_transcript() {
        let doc = json!({
            "check": "stale-holds",
            "events_scanned": 240,
            "stale_holds": [ {
                "path": "src/cmd/at.rs",
                "agent": "skeleton",
                "opened_at": NOON,
                "closed_at": "2026-03-01T14:00:00Z",
                "opened_line": 12,
                "held_secs": 7200,
                "renewals": 0
            } ]
        });
        let r = run_over(
            &doc,
            &[skeleton_session()],
            vec![
                turn("2026-03-01T11:00:00Z", "long before the window"),
                turn("2026-03-01T12:01:00Z", "holding at.rs while the join lands"),
            ],
        );

        assert_eq!(r.exit_code(), EXIT_OK);
        assert_eq!(r.totals.findings, 1);
        assert_eq!(r.totals.testified, 1);
        assert!(r.untestified.is_empty());
        assert_eq!(r.check.as_deref(), Some("stale-holds"));

        let t = &r.findings[0];
        assert_eq!(t.check, "stale_holds");
        assert_eq!(t.outcome, Outcome::Match);
        assert_eq!(t.agent.as_deref(), Some("skeleton"));
        // `opened_at`, not the alphabetically-first `closed_at`: the moment the
        // lease was taken is the decision worth reading a transcript around.
        assert_eq!(t.ts, Some(ts(NOON)));
        assert_eq!(t.event_line, Some(12));

        let d = t.testimony.as_ref().unwrap();
        assert!(d.confident);
        assert_eq!(d.transcript.len(), 1, "{:?}", d.transcript);
        assert_eq!(d.transcript[0].text, "holding at.rs while the join lands");

        // The finding itself came back out untouched — this is a pipeline stage.
        assert_eq!(t.finding["path"], "src/cmd/at.rs");
        assert_eq!(t.finding["held_secs"], 7200);

        let text = text_of(&r);
        assert!(text.contains("stale_holds[0] — agent `skeleton`"), "{text}");
        assert!(text.contains("ledger line 12"), "{text}");
        assert!(
            text.contains("holding at.rs while the join lands"),
            "{text}"
        );
    }

    /// A finding whose agent has no session anywhere: exit 1, and the output
    /// names which finding and why.
    #[test]
    fn a_finding_with_no_match_exits_1_and_is_listed() {
        let doc = json!({
            "check": "double-win",
            "double_wins": [ {
                "path": "src/join.rs",
                "incoming_agent": "ghost",
                "incoming_kind": "acquired",
                "incoming_at": NOON,
                "incoming_line": 40,
                "already_holding": [ { "agent": "skeleton", "since": NOON, "since_line": 7 } ]
            } ]
        });
        let r = run_over(&doc, &[], Vec::new());

        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
        assert_eq!(r.totals.untestified, 1);
        assert_eq!(r.findings[0].outcome, Outcome::Refused);
        assert_eq!(r.findings[0].agent.as_deref(), Some("ghost"));
        assert_eq!(
            r.findings[0].refusal.as_ref().unwrap().reason,
            "no cwd match"
        );

        let listed = &r.untestified[0];
        assert!(
            listed.starts_with("double_wins[0] agent `ghost`"),
            "{listed}"
        );
        assert!(listed.contains("no cwd match"), "{listed}");
        let text = text_of(&r);
        assert!(
            text.contains("1 finding(s) could not be testified:"),
            "{text}"
        );
        assert!(text.contains("double_wins[0] agent `ghost`"), "{text}");
    }

    /// The check that ran and found nothing. Exit 0: a clean audit is the
    /// outcome this whole release is trying to reach, not a failure to report.
    #[test]
    fn an_empty_findings_set_is_exit_0() {
        let doc = json!({
            "check": "double-win",
            "double_wins": [],
            "stale_holds": [],
            "events_scanned": 240
        });
        let r = run_over(&doc, &[skeleton_session()], Vec::new());
        assert_eq!(r.exit_code(), EXIT_OK);
        assert_eq!(r.totals.findings, 0);
        assert!(r.findings.is_empty());
        let text = text_of(&r);
        assert!(text.contains("nothing to testify"), "{text}");
        assert!(text.contains("A clean audit is not a failure."), "{text}");
    }

    /// A `pact audit --json` summary carries no findings at all — and its four
    /// statistics arrays must not be mistaken for some.
    #[test]
    fn a_summary_documents_statistics_are_not_findings() {
        let doc = json!({
            "events": 240,
            "agents": ["skeleton", "orchestrator"],
            "annotations": [],
            "per_agent": [ { "agent": "skeleton", "events": 32, "holds": 16 } ],
            "top_contended": [ { "path": "src/cmd/at.rs", "holds": 6 } ],
            "by_kind": { "acquired": 116 }
        });
        let r = run_over(&doc, &[skeleton_session()], Vec::new());
        assert_eq!(r.totals.findings, 0, "{:?}", r.findings);
        assert_eq!(r.exit_code(), EXIT_OK);
        assert_eq!(r.check, None);
        assert!(text_of(&r).contains("pact audit summary (no --check)"));
    }

    /// A topology mismatch names a checkout, not an agent, and no moment at
    /// all. It is a real finding and it is not an error: through, untouched,
    /// counted.
    #[test]
    fn a_finding_with_no_agent_or_ts_passes_through() {
        let doc = json!({
            "check": "topology",
            "topology_mismatches": [ { "invoked_from": "main", "events": 4 } ]
        });
        let r = run_over(&doc, &[skeleton_session()], Vec::new());

        assert_eq!(r.exit_code(), EXIT_OK, "a pass-through is not a failure");
        assert_eq!(r.totals.passed_through, 1);
        assert_eq!(r.totals.untestified, 0);
        let t = &r.findings[0];
        assert_eq!(t.outcome, Outcome::PassedThrough);
        assert_eq!(t.agent, None);
        assert_eq!(t.finding["invoked_from"], "main");
        assert!(text_of(&r).contains("no agent and no moment: passed through untouched"));
    }

    /// Below the bar is not a match: exit 1, the candidate is shown with its
    /// evidence, and the transcript is withheld.
    #[test]
    fn a_below_threshold_candidate_is_untestified_and_keeps_its_transcript_back() {
        // A session at the repo root that never names the agent: topology B.
        let s = session(
            "/s/parent.jsonl",
            "/repo",
            "2026-03-01T11:55:00Z",
            "2026-03-01T12:05:00Z",
        );
        let doc = json!({ "stale_holds": [ { "agent": "skeleton", "opened_at": NOON } ] });
        let r = run_over(&doc, &[s], vec![turn(NOON, "no identity is stated here")]);

        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
        assert_eq!(r.findings[0].outcome, Outcome::LowConfidence);
        let d = r.findings[0].testimony.as_ref().unwrap();
        assert!(!d.confident);
        assert!(d.transcript.is_empty(), "a guess is not testimony");
        assert!(
            !d.evidence.is_empty(),
            "the evidence is the answer to why not"
        );
        assert!(
            r.untestified[0].contains("below the bar"),
            "{}",
            r.untestified[0]
        );
    }

    /// Malformed JSON is a clean error naming where it came from, not a panic
    /// and not half a document.
    #[test]
    fn malformed_input_is_a_clean_error() {
        for bad in ["{ not json at all", "", "\n\n"] {
            let e = serde_json::from_str::<Value>(bad).unwrap_err();
            // The message `run` wraps it in; asserted here rather than by
            // driving `run`, which would print to the process's own stdout.
            let msg =
                format!("stdin is not JSON — testify reads a `pact audit --json` document: {e}");
            assert!(msg.contains("is not JSON"), "{msg}");
        }
        // And a document that IS JSON but is not an object or an array is not
        // an error either — it simply has no findings.
        assert!(raw_findings(&json!(7)).is_empty());
        assert!(raw_findings(&json!("nope")).is_empty());
        assert!(raw_findings(&Value::Null).is_empty());
    }

    /// `--file` and stdin are the same input, and say so differently.
    #[test]
    fn file_and_stdin_are_the_same_document() {
        let body = r#"{"check":"stale-holds","stale_holds":[]}"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.json");
        std::fs::write(&path, body).unwrap();

        let (from_file, file_text) = input(Some(&path), &mut io::empty()).unwrap();
        let (from_stdin, stdin_text) = input(None, &mut body.as_bytes()).unwrap();

        assert_eq!(file_text, stdin_text);
        assert_eq!(from_stdin, "stdin");
        assert_eq!(from_file, path.display().to_string());

        let doc: Value = serde_json::from_str(&file_text).unwrap();
        assert_eq!(run_over(&doc, &[], Vec::new()).totals.findings, 0);

        // A `--file` that is not there is a clean error naming it, not a panic.
        let e = format!(
            "{:#}",
            input(Some(&dir.path().join("nope.json")), &mut io::empty()).unwrap_err()
        );
        assert!(e.contains("reading "), "{e}");
        assert!(e.contains("nope.json"), "{e}");
    }

    /// The stream split, asserted here because `tests/degradation.rs` asserts
    /// it of every other command: one digest, two sinks, byte for byte.
    #[test]
    fn the_digest_is_identical_with_and_without_json() {
        let doc = json!({ "stale_holds": [ { "agent": "skeleton", "opened_at": NOON } ] });
        let r = run_over(&doc, &[skeleton_session()], vec![turn(NOON, "working")]);

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, true, &mut out, &mut err).unwrap();
        let (mut plain, mut nothing) = (Vec::new(), Vec::new());
        emit(&r, false, &mut plain, &mut nothing).unwrap();

        assert_eq!(err, plain, "json.stderr must equal plain.stdout");
        assert!(nothing.is_empty(), "no --json means nothing on stderr");
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["command"], "testify");
        assert_eq!(v["schema"], 1);
        assert_eq!(v["findings"][0]["finding"]["agent"], "skeleton");
    }

    /// A bare array is what `jq '.stale_holds'` hands over, and a bare object
    /// is what `jq '.stale_holds[0]'` hands over. Both are documents somebody
    /// will pipe in.
    #[test]
    fn a_bare_array_or_object_is_read_as_findings() {
        let one = json!({ "agent": "skeleton", "opened_at": NOON });
        let r = run_over(
            &json!([one.clone(), one.clone()]),
            &[skeleton_session()],
            Vec::new(),
        );
        assert_eq!(r.totals.findings, 2);
        assert_eq!(r.findings[0].check, "findings");
        assert_eq!(r.findings[1].index, 1);

        // A single object has no array to walk, so it yields nothing rather
        // than pretending its scalar fields are findings.
        assert!(raw_findings(&one).is_empty());
    }

    /// The field sweep, on shapes taken from pact's own structs — and on one
    /// nobody has written yet, which is the point of sweeping by name shape.
    #[test]
    fn agent_and_moment_are_found_by_name_shape() {
        let raw = |v: Value| Raw {
            check: "c".into(),
            index: 0,
            value: v,
        };
        let at = |v: Value| raw(v).agent_and_ts();

        // silent-contention
        assert_eq!(
            at(json!({ "refused_agent": "bench", "refused_at": NOON, "holder": "skeleton" })),
            Some(("bench".to_string(), ts(NOON)))
        );
        // claim-lease-divergence
        assert_eq!(
            at(json!({ "agent": "tier", "acquired_at": NOON, "bead": "recount-1cw.9" })),
            Some(("tier".to_string(), ts(NOON)))
        );
        // a check that does not exist yet, using the house naming
        assert_eq!(
            at(json!({ "blaming_agent": "future", "noticed_at": NOON })),
            Some(("future".to_string(), ts(NOON)))
        );
        // Half a pair is not a pair.
        assert_eq!(at(json!({ "agent": "lonely" })), None);
        assert_eq!(at(json!({ "at": NOON })), None);
        // Right name, wrong type: fall through rather than abort.
        assert_eq!(
            at(json!({ "agent": null, "holder": "real", "at": 17, "opened_at": NOON })),
            Some(("real".to_string(), ts(NOON)))
        );
        // Not an object at all.
        assert_eq!(at(json!("a string finding")), None);
        assert_eq!(raw(json!("a string finding")).event_line(), None);
    }

    /// The ledger line, which is how a finding reaches the attribution chain.
    #[test]
    fn the_ledger_line_is_found_and_zero_is_not_a_line() {
        let raw = |v: Value| Raw {
            check: "c".into(),
            index: 0,
            value: v,
        };
        assert_eq!(raw(json!({ "line": 7 })).event_line(), Some(7));
        assert_eq!(raw(json!({ "incoming_line": 40 })).event_line(), Some(40));
        assert_eq!(raw(json!({ "opened_line": 3 })).event_line(), Some(3));
        // 1-based everywhere in this crate, so 0 is not a line.
        assert_eq!(raw(json!({ "line": 0 })).event_line(), None);
        assert_eq!(raw(json!({ "held_secs": 90 })).event_line(), None);
    }

    /// A ledger row naming its own transcript is the keyed tier, and `testify`
    /// reaches it the only way anything can: through the line the finding
    /// pointed at.
    #[test]
    fn a_finding_naming_a_line_carries_the_chain_into_the_query() {
        let e = Event {
            line: 12,
            at: ts(NOON),
            agent: "skeleton".into(),
            path: None,
            invoked_from: Some("skeleton".into()),
            detail: Some("recount-1cw.9: audit findings in".into()),
            harness: Some("claude-code".into()),
            model: Some("claude-opus-5".into()),
            harness_session: Some("cd7c75eb-c39f-4be4-92ae-29efda5d7d69".into()),
            harness_subagent: Some("a18a1d576ceef7599".into()),
            branch: Some("wt/testify".into()),
        };
        assert_eq!(bead(&e), Some("recount-1cw.9"));
        assert_eq!(
            bead(&Event {
                detail: Some("merging wt/ci into master".into()),
                ..e.clone()
            }),
            None
        );

        // The keyed tier refuses when the named transcript is not on this disk
        // — a different sentence from "no cwd match", and the one that proves
        // the chain was passed through from the finding's line.
        let ledger = Events {
            events: vec![e],
            parse_failures: 0,
            line_count: 12,
        };
        let doc = json!({ "stale_holds": [ { "agent": "skeleton", "opened_at": NOON, "opened_line": 12 } ] });
        let r = annotate(
            &args(),
            &doc,
            "stdin".to_string(),
            Path::new("/repo"),
            &fleet(),
            &[PathBuf::from("/dir")],
            &[skeleton_session()],
            Some(&ledger),
            &|_| Vec::new(),
        );
        assert_eq!(
            r.findings[0].refusal.as_ref().unwrap().reason,
            "no keyed transcript"
        );
        assert_eq!(r.exit_code(), EXIT_NO_MATCH);
    }

    /// Findings from several checks in one document all come out, each labelled
    /// with the check it came from, in a stable order.
    #[test]
    fn every_check_in_one_document_is_walked() {
        let doc = json!({
            "double_wins": [ { "incoming_agent": "a", "incoming_at": NOON } ],
            "stale_holds": [ { "agent": "b", "opened_at": NOON } ],
            "topology_mismatches": [ { "invoked_from": "main", "events": 4 } ]
        });
        let r = run_over(&doc, &[], Vec::new());
        assert_eq!(r.totals.findings, 3);
        assert_eq!(r.totals.passed_through, 1);
        assert_eq!(r.totals.untestified, 2);
        let checks: Vec<&str> = r.findings.iter().map(|t| t.check.as_str()).collect();
        assert_eq!(
            checks,
            ["double_wins", "stale_holds", "topology_mismatches"]
        );
    }

    /// Evidence is never empty, on either tier — the crate-wide rule, asserted
    /// here because `testify` is a new place it could be broken.
    #[test]
    fn a_testified_finding_shows_the_evidence_that_named_it() {
        let doc = json!({ "stale_holds": [ { "agent": "skeleton", "opened_at": NOON } ] });
        let r = run_over(
            &doc,
            &[skeleton_session()],
            vec![turn(NOON, "PACT_AGENT=skeleton at work")],
        );
        let d = r.findings[0].testimony.as_ref().unwrap();
        assert!(!d.evidence.is_empty());
        assert!(d
            .evidence
            .iter()
            .any(|e| matches!(e.signal, Signal::Topology)));
        assert!(text_of(&r).contains("tier "), "{}", text_of(&r));
    }
}
