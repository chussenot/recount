//! `recount sessions --repo <p> [--since --until]`
//!
//! Spec: "sessions whose cwd resolves into the repo or its worktrees,
//! fleet-identity guesses attached."
//!
//! This is the first command to land, so the two shapes below are the house
//! style the other four follow. Both are decisions, not accidents.
//!
//! # Output: JSON on stdout, digest on stderr
//!
//! "all --json; human digest second" has to survive `recount sessions --json |
//! jq`, so the two outputs go to different streams:
//!
//! * **without `--json`** — the digest, on stdout, and nothing else.
//! * **with `--json`** — the JSON document on stdout, the same digest on
//!   stderr, printed second. A pipe gets clean JSON; a person watching the
//!   terminal still gets the readable version; `2>/dev/null` is not required
//!   but works.
//!
//! Neither stream is optional and neither is a summary of the other: the JSON
//! is the contract, the digest is the reading.
//!
//! # The JSON contract
//!
//! Audit scripts consume this, so it is versioned and it is stable. Every
//! command emits the same envelope — `command`, `schema`, then its payload —
//! and `schema` only ever moves when a field is removed or its meaning
//! changes. Added fields do not bump it; readers must ignore what they do not
//! know.
//!
//! ```text
//! { "command": "sessions", "schema": 1,
//!   "repo": "/abs/path",                          // canonical
//!   "window": { "since": <rfc3339|null>, "until": <rfc3339|null> },
//!   "session_dirs": ["/home/u/.claude/projects/-abs-path", ...],
//!   "discovered": 12,                             // before the cwd filter
//!   "excluded_by_cwd": 1,                         // said a cwd, outside the repo
//!   "sessions": [ {
//!       "path": "…/subagents/agent-<hex>.jsonl",
//!       "connector": "claude_code",
//!       "cwd": <path|null>,
//!       "first_ts": <rfc3339|null>, "last_ts": <rfc3339|null>,
//!       "turn_count": 128, "parse_failures": 2,
//!       "worktree": <name|null>,                  // null: the cwd did not say
//!       "repo_match": "cwd" | "session_dir",
//!       "fleet_identity_guess": {
//!           "agent": <name|null>,                 // null: we cannot tell
//!           "confidence": 0.90,
//!           "evidence": [ { "signal": "...", "detail": "..." }, ... ]
//!       } } ],
//!   "warnings": ["worktree `gone` is listed by git but its directory is gone"] }
//! ```
//!
//! # A guess is shaped like a guess
//!
//! Nothing in this output states a fleet identity as a fact. The key is named
//! `fleet_identity_guess`; its `agent` is nullable and is never printed
//! without a `confidence` beside it; its `evidence` is never empty — when
//! `agent` is null the evidence says *why* we could not tell, which is itself
//! the answer an auditor needs. The digest spells the same thing in words: "a
//! GUESS (confidence 0.90)", or "unknown — no guess".
//!
//! The two signals are the ones this tree can actually support, and
//! `docs/format-notes.md` §4.3 measured both:
//!
//! 1. **Topology** — the session's `cwd` resolved through
//!    [`worktree::for_cwd`] to a linked worktree, whose name is the pact
//!    `invoked_from`. Correct when it fires, and §4.3 measured that for a pact
//!    fleet it usually will not: `cwd` is the Claude Code *process's*
//!    directory, and a pact agent enters its worktree with `cd` inside a Bash
//!    call, so every agent of one run reports the repo root.
//! 2. **Transcript mention** — the literal `PACT_AGENT=<name>` assignment in
//!    the file's bytes. §4.3 again: for pact worktrees this is not a
//!    tiebreaker, it is the only signal that separates sibling agents. It is a
//!    substring scan rather than a parse, because the name reads the same in a
//!    JSON string as in a raw line and this must work for a connector whose
//!    reader has its own opinions. Two different names found means *no* guess
//!    — an agent quoting another agent's assignment is not that agent.
//!
//! A better signal slots in without changing the shape: a connector that
//! surfaces the `.meta.json` `description` sidecar (§3.4) adds one
//! [`Evidence`] to the same vector and one arm to the arithmetic in
//! [`guess_identity`]. The output type does not move.
//!
//! # No sessions is an answer
//!
//! A repo with no transcripts prints an empty list and exits 0. Not an error,
//! not a warning wearing an error's clothes: recount runs from repos that have
//! never seen a coding agent, and "none" is the honest result.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::cli::SessionsArgs;
use crate::cmd::EXIT_OK;
use crate::connector::{self, Connector};
use crate::model::{Confidence, Evidence, SessionMeta, Signal, TimeWindow};
use crate::worktree::{self, Worktree};

/// Bumped only when a field leaves or changes meaning. See the module doc.
const SCHEMA: u32 = 1;

pub fn run(args: &SessionsArgs, json: bool) -> Result<u8> {
    let window = args.window.window();
    // The spec's performance clause, in two lines and before any file opens:
    // `--repo` becomes the encoded directories, and the ones that are not
    // there are dropped. A directory that does not exist has nothing in it, so
    // a repo no agent has ever run in reaches no connector at all — at a cost
    // of one `stat` per worktree.
    let worktrees = worktree::worktrees(&args.repo)?;
    let dirs: Vec<PathBuf> = worktree::session_dirs(&args.repo)?
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let repo = worktrees
        .first()
        .map_or_else(|| args.repo.clone(), |w| w.path.clone());

    let report = collect(&connector::registry(), &repo, &worktrees, &dirs, &window);
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(EXIT_OK)
}

/// How a session was tied to the repo. Both are reported rather than one being
/// silently treated as the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoMatch {
    /// The session stated a `cwd` and it resolved into the repo or a worktree.
    Cwd,
    /// The session stated no `cwd`; it is here because it is filed under one
    /// of this repo's encoded session directories. Weaker — the encoding is
    /// not injective (`worktree`'s module doc) — so it is labelled, not hidden.
    SessionDir,
}

impl RepoMatch {
    fn label(self) -> &'static str {
        match self {
            RepoMatch::Cwd => "cwd",
            RepoMatch::SessionDir => "session dir",
        }
    }
}

/// An inference about which fleet agent ran a session. Read the module doc:
/// every field here exists to stop this being mistaken for a record.
#[derive(Debug, Clone, Serialize)]
pub struct FleetGuess {
    /// The identity we think ran this session, or `None` when we cannot tell.
    pub agent: Option<String>,
    pub confidence: Confidence,
    /// Never empty. When `agent` is `None` this is why.
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEntry {
    pub path: PathBuf,
    pub connector: String,
    pub cwd: Option<PathBuf>,
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub turn_count: usize,
    pub parse_failures: usize,
    /// The checkout the `cwd` resolved into, `None` when the cwd did not say.
    pub worktree: Option<String>,
    pub repo_match: RepoMatch,
    pub fleet_identity_guess: FleetGuess,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub command: &'static str,
    pub schema: u32,
    pub repo: PathBuf,
    pub window: TimeWindow,
    pub session_dirs: Vec<PathBuf>,
    /// Sessions the connectors returned, before the cwd filter.
    pub discovered: usize,
    /// Sessions dropped because they named a cwd outside the repo.
    pub excluded_by_cwd: usize,
    pub sessions: Vec<SessionEntry>,
    pub warnings: Vec<String>,
}

/// Everything after the resolution: discover, filter to the repo, guess.
///
/// `dirs` and `worktrees` are arguments rather than rediscovered here so the
/// resolution happens once, in [`run`], before anything opens a file — and so
/// this is testable without a `$HOME`.
fn collect(
    connectors: &[Box<dyn Connector>],
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    window: &TimeWindow,
) -> Report {
    // No directory to look in is a question with a known answer, so no
    // connector is asked it.
    let discovered: Vec<SessionMeta> = if dirs.is_empty() {
        Vec::new()
    } else {
        connectors
            .iter()
            .flat_map(|c| c.discover(dirs, window))
            .collect()
    };

    let mut excluded_by_cwd = 0;
    let mut sessions: Vec<SessionEntry> = Vec::new();
    for meta in &discovered {
        let Some((repo_match, worktree)) = tie_to_repo(worktrees, meta.cwd.as_deref()) else {
            excluded_by_cwd += 1;
            continue;
        };
        sessions.push(SessionEntry {
            fleet_identity_guess: guess_identity(meta, worktrees, worktree),
            path: meta.path.clone(),
            connector: meta.connector.clone(),
            cwd: meta.cwd.clone(),
            first_ts: meta.first_ts,
            last_ts: meta.last_ts,
            turn_count: meta.turn_count,
            parse_failures: meta.parse_failures,
            worktree: worktree.map(|w| w.name.clone()),
            repo_match,
        });
    }
    // Oldest first, ties broken by path. Sessions with no parsed timestamp
    // sort first (`None < Some`), which is where a reader wants them: an
    // unstamped transcript is the one worth looking at.
    sessions.sort_by(|a, b| {
        a.first_ts
            .cmp(&b.first_ts)
            .then_with(|| a.path.cmp(&b.path))
    });

    Report {
        command: "sessions",
        schema: SCHEMA,
        repo: repo.to_path_buf(),
        window: *window,
        session_dirs: dirs.to_vec(),
        discovered: discovered.len(),
        excluded_by_cwd,
        sessions,
        warnings: worktrees
            .iter()
            .filter(|w| !w.alive)
            .map(|w| {
                format!(
                    "worktree `{}` is listed by git but {} is gone from disk; its transcripts outlive it",
                    w.name,
                    w.path.display()
                )
            })
            .collect(),
    }
}

/// Does this session belong to the repo, and to which checkout?
///
/// `None` means exclude. The spec filters on "sessions whose cwd resolves into
/// the repo or its worktrees", and the three cases are not the same:
///
/// * a cwd that resolves — kept, [`RepoMatch::Cwd`], worktree named;
/// * a cwd that resolves *elsewhere* — dropped. It is a real contradiction:
///   the file sits in this repo's encoded directory but says it ran somewhere
///   else, which the lossy encoding makes possible (`worktree`'s module doc)
///   and `docs/format-notes.md` §4.2 measured happening;
/// * **no cwd at all** — kept, [`RepoMatch::SessionDir`]. `generic_jsonl` never
///   states one and `journal.jsonl` has none, and the encoded directory is
///   still real evidence. Dropping these would turn "the file does not say"
///   into "no sessions found", which is the misdiagnosis this tool exists to
///   prevent. The weaker basis is labelled instead.
///
/// ponytail: this reads ONE cwd, and `SessionMeta.cwd` is a summary rather
/// than the evidence. `docs/format-notes.md` §4.2 measured 46 of 871 files
/// holding more than one cwd and one holding nine, and claude_code (landing
/// on `wt/claudecode`, pact-msg-229b6cd0ee47491b) resolves the single field
/// by most-frequent-wins. So a session that did half its work in this repo
/// and more of it elsewhere lands in `excluded_by_cwd` instead of the list.
/// It is *counted*, not silently dropped, which is the only reason this is
/// shippable. The fix needs a field that does not exist yet: take the whole
/// `Vec<PathBuf>` and keep the session if ANY of them resolves — one
/// signature change here and one at the call site, once both branches are on
/// master. Tracked as recount-hwk.33's follow-up.
fn tie_to_repo<'a>(
    worktrees: &'a [Worktree],
    cwd: Option<&Path>,
) -> Option<(RepoMatch, Option<&'a Worktree>)> {
    match cwd {
        None => Some((RepoMatch::SessionDir, None)),
        Some(cwd) => worktree::for_cwd(worktrees, cwd).map(|w| (RepoMatch::Cwd, Some(w))),
    }
}

/// The literal `PACT_AGENT=` assignment, which is how a pact agent is told who
/// it is.
const PACT_AGENT: &str = "PACT_AGENT=";

/// Which fleet agent ran this session? Read the module doc first — this is an
/// inference and its return type says so.
///
/// Both signals always contribute exactly one [`Evidence`], including when
/// they name no one — which is what keeps `evidence` non-empty even for a
/// guess that came out empty, and why "unknown" is an answer with reasons
/// rather than a shrug.
///
/// `worktree` is the checkout the cwd resolved into, if any.
fn guess_identity(
    meta: &SessionMeta,
    worktrees: &[Worktree],
    worktree: Option<&Worktree>,
) -> FleetGuess {
    let mut evidence = Vec::new();

    // Signal 1: topology. The main checkout is not an identity — every agent
    // of a pact run reports it (format-notes §4.3) — so only a linked worktree
    // names anyone.
    let main = worktrees.first().map(|w| w.path.as_path());
    let from_cwd = match (worktree, &meta.cwd) {
        (Some(w), Some(cwd)) if Some(w.path.as_path()) != main => {
            evidence.push(Evidence::new(
                Signal::Topology,
                format!("cwd {} is worktree `{}`", cwd.display(), w.name),
            ));
            Some(w.name.clone())
        }
        (Some(_), Some(cwd)) => {
            evidence.push(Evidence::new(
                Signal::Topology,
                format!(
                    "cwd {} is the repo root, which every agent of a pact run shares \
                     (docs/format-notes.md §4.3) — it names no one",
                    cwd.display()
                ),
            ));
            None
        }
        _ => {
            evidence.push(Evidence::new(
                Signal::Topology,
                "the session states no cwd, so topology names no one".to_string(),
            ));
            None
        }
    };

    // Signal 2: the transcript names an identity out loud.
    let named = pact_agent_names(&meta.path);
    let mentioned: Option<String> = match named.len() {
        0 => {
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!("no `{PACT_AGENT}` assignment appears in the transcript"),
            ));
            None
        }
        1 => {
            let name = named.iter().next().expect("len 1").clone();
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!("`{PACT_AGENT}{name}` appears in the transcript"),
            ));
            Some(name)
        }
        n => {
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!(
                    "the transcript names {n} identities ({}) — an agent quoting \
                     another agent's assignment is not that agent",
                    named.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            ));
            // One of them being the worktree we are standing in does break the
            // tie: the other names are being discussed, this one is where the
            // work happened.
            from_cwd.as_ref().filter(|w| named.contains(*w)).cloned()
        }
    };

    // The arithmetic, in one place. Both signals agreeing is the only thing
    // that gets near certainty; the mention alone outranks topology alone,
    // because §4.3 measured topology going quiet for exactly the fleet this
    // tool audits.
    let (agent, confidence) = match (&mentioned, &from_cwd) {
        // Ambiguity first: when the transcript named several identities the
        // worktree only broke a tie, which is not the same as two independent
        // signals agreeing, and must not be priced as if it were.
        (Some(_), Some(_)) if named.len() > 1 => (mentioned.clone(), 0.80),
        (Some(m), Some(w)) if m == w => (mentioned.clone(), 0.90),
        (Some(_), Some(_)) => (mentioned.clone(), 0.40),
        (Some(_), None) => (mentioned.clone(), 0.70),
        (None, Some(_)) => (from_cwd.clone(), 0.50),
        (None, None) => (None, 0.0),
    };
    FleetGuess {
        agent,
        confidence: Confidence::new(confidence),
        evidence,
    }
}

/// Every distinct `<name>` in a `PACT_AGENT=<name>` assignment anywhere in the
/// file's bytes.
///
/// A substring scan, not a parse: see the module doc. An unreadable file names
/// no one, which is the same answer as a file that mentions no one — neither
/// is an error.
///
/// ponytail: this re-reads each surviving transcript, which `discover` has
/// just read and the page cache still holds. If a real corpus makes that
/// measurable, the fix is for connectors to surface the identity fields
/// format-notes §3.4 already lists (`attributionAgent`, the `.meta.json`
/// `description`) and for this scan to become the fallback — not a cache.
fn pact_agent_names(path: &Path) -> BTreeSet<String> {
    let Ok(bytes) = fs::read(path) else {
        return BTreeSet::new();
    };
    String::from_utf8_lossy(&bytes)
        .split(PACT_AGENT)
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
/// See the module doc.
///
/// The sinks are arguments so the split itself can be asserted — it is the
/// house rule the other four commands inherit, and a rule nothing tests is a
/// suggestion.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors are ignored on purpose: `recount sessions --json | head`
        // closing the pipe is not a failure of the command.
        let _ = writeln!(out, "{rendered}");
        let _ = digest(report, err);
    } else {
        let _ = digest(report, out);
    }
    Ok(())
}

fn digest(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let stamp = |t: Option<DateTime<Utc>>| match t {
        Some(t) => t.to_rfc3339_opts(SecondsFormat::Secs, true),
        None => "unbounded".to_string(),
    };

    writeln!(w, "recount sessions — {}", r.repo.display())?;
    writeln!(
        w,
        "  window   since {} · until {}",
        stamp(r.window.since),
        stamp(r.window.until)
    )?;
    write!(
        w,
        "  scanned  {} session dir(s) · {} discovered",
        r.session_dirs.len(),
        r.discovered
    )?;
    if r.excluded_by_cwd > 0 {
        write!(
            w,
            " · {} excluded (cwd outside the repo)",
            r.excluded_by_cwd
        )?;
    }
    writeln!(w)?;

    if r.sessions.is_empty() {
        writeln!(
            w,
            "  no sessions — a normal answer, not a failure: recount often runs where \
             transcripts do not exist"
        )?;
    }

    for s in &r.sessions {
        writeln!(w, "\n  {}", s.path.display())?;
        let mut facts = vec![
            s.connector.clone(),
            format!(
                "{} turn{}",
                s.turn_count,
                if s.turn_count == 1 { "" } else { "s" }
            ),
        ];
        if s.parse_failures > 0 {
            facts.push(format!("{} parse failures", s.parse_failures));
        }
        facts.push(match (s.first_ts, s.last_ts) {
            (Some(a), Some(b)) => format!("{} → {}", stamp(Some(a)), stamp(Some(b))),
            _ => "no parsed timestamps".to_string(),
        });
        facts.push(format!("matched by {}", s.repo_match.label()));
        if let Some(name) = &s.worktree {
            facts.push(format!("worktree `{name}`"));
        }
        writeln!(w, "    {}", facts.join(" · "))?;

        let g = &s.fleet_identity_guess;
        match &g.agent {
            Some(a) => writeln!(
                w,
                "    fleet identity: probably `{a}` — a GUESS (confidence {})",
                g.confidence
            )?,
            None => writeln!(w, "    fleet identity: unknown — no guess")?,
        }
        for e in &g.evidence {
            writeln!(w, "      · {}: {}", signal_words(e.signal), e.detail)?;
        }
    }

    for warning in &r.warnings {
        writeln!(w, "\n! {warning}")?;
    }
    Ok(())
}

/// A [`Signal`] in words, for the digest. The JSON spells the same three as
/// `topology` / `time_overlap` / `transcript_mention`, via serde.
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

    use std::process::Command;

    use serde_json::Value;
    use tempfile::TempDir;

    use crate::connector::generic_jsonl::GenericJsonl;
    use crate::model::{parse_ts, Turn};

    fn wt(path: &str, name: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive: true,
            session_dir: PathBuf::from("/nowhere"),
        }
    }

    fn only_generic() -> Vec<Box<dyn Connector>> {
        vec![Box::new(GenericJsonl)]
    }

    /// `{ts, role, content}` lines, one per argument.
    fn write_session(dir: &Path, name: &str, lines: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines
            .iter()
            .map(|(ts, text)| {
                format!("{{\"ts\":\"{ts}\",\"role\":\"user\",\"content\":{text:?}}}\n")
            })
            .collect();
        fs::write(&path, body).unwrap();
        path
    }

    fn json_of(r: &Report) -> Value {
        serde_json::from_str(&serde_json::to_string(r).unwrap()).unwrap()
    }

    /// The spec's calm-degradation clause, through the real `run`: a repo
    /// nothing has ever run in prints an empty list and exits 0.
    ///
    /// This drives the real connector registry and the real `$HOME`. It is
    /// hermetic because the repo lives in a fresh `TempDir`, so its encoded
    /// session directories (`-tmp-…`) cannot exist on any machine — which is
    /// exactly the fresh-machine shape being tested.
    #[test]
    fn a_repo_with_no_sessions_is_an_empty_list_and_exit_0() {
        let td = TempDir::new().unwrap();
        let repo = td.path().canonicalize().unwrap().join("repo");
        fs::create_dir(&repo).unwrap();
        for args in [
            ["init", "-q"].as_slice(),
            ["commit", "-q", "--allow-empty", "-m", "root"].as_slice(),
        ] {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .output()
                .expect("git on PATH")
                .status
                .success();
            assert!(ok, "git {args:?}");
        }

        let args = SessionsArgs {
            repo: repo.clone(),
            window: crate::cli::WindowArgs {
                since: None,
                until: None,
            },
        };
        assert_eq!(
            run(&args, true).expect("no sessions is not an error"),
            EXIT_OK
        );

        // And the same answer, inspected: an empty list, no warnings, no
        // language that reads as a failure.
        let worktrees = worktree::worktrees(&repo).unwrap();
        let report = collect(
            &connector::registry(),
            &repo,
            &worktrees,
            &[],
            &TimeWindow::default(),
        );
        assert!(report.sessions.is_empty());
        assert_eq!(report.discovered, 0);
        assert!(report.warnings.is_empty());
        let mut text = Vec::new();
        digest(&report, &mut text).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("no sessions"), "{text}");
        assert!(!text.to_lowercase().contains("error"), "{text}");
    }

    /// The house rule the other four commands inherit: `--json` puts the
    /// document on stdout alone and the digest on stderr, second.
    #[test]
    fn json_goes_to_stdout_and_the_digest_to_stderr() {
        let report = collect(
            &only_generic(),
            Path::new("/r"),
            &[wt("/r", "main")],
            &[],
            &TimeWindow::default(),
        );

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&report, true, &mut out, &mut err).unwrap();
        serde_json::from_slice::<Value>(&out).expect("stdout is JSON and nothing else");
        assert!(String::from_utf8_lossy(&err).contains("recount sessions"));

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&report, false, &mut out, &mut err).unwrap();
        assert!(err.is_empty(), "no --json means nothing on stderr");
        assert!(String::from_utf8_lossy(&out).contains("recount sessions"));
        assert!(
            serde_json::from_slice::<Value>(&out).is_err(),
            "the digest is not JSON"
        );
    }

    /// The documented JSON shape, key by key. Audit scripts read this; a change
    /// here is a `SCHEMA` bump.
    #[test]
    fn json_shape_is_the_documented_contract() {
        let td = TempDir::new().unwrap();
        write_session(
            td.path(),
            "s.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions here")],
        );
        let worktrees = [wt("/r", "main")];
        let report = collect(
            &only_generic(),
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );

        let doc = json_of(&report);
        assert_eq!(doc["command"], "sessions");
        assert_eq!(doc["schema"], 1);
        assert_eq!(doc["repo"], "/r");
        assert_eq!(
            doc["window"],
            serde_json::json!({"since": null, "until": null})
        );
        assert_eq!(doc["discovered"], 1);
        assert_eq!(doc["excluded_by_cwd"], 0);
        assert_eq!(doc["warnings"], serde_json::json!([]));

        let s = &doc["sessions"][0];
        for key in [
            "path",
            "connector",
            "cwd",
            "first_ts",
            "last_ts",
            "turn_count",
            "parse_failures",
            "worktree",
            "repo_match",
            "fleet_identity_guess",
        ] {
            assert!(
                !s[key].is_null() || matches!(key, "cwd" | "worktree"),
                "{key}"
            );
        }
        assert_eq!(s["connector"], "generic_jsonl");
        assert_eq!(s["turn_count"], 1);
        assert_eq!(s["first_ts"], "2026-08-19T07:00:00Z");
        // generic_jsonl states no cwd: kept, and labelled with the weaker basis.
        assert_eq!(s["cwd"], Value::Null);
        assert_eq!(s["worktree"], Value::Null);
        assert_eq!(s["repo_match"], "session_dir");

        let g = &s["fleet_identity_guess"];
        assert_eq!(g["agent"], "cmdsessions");
        assert_eq!(g["confidence"], 0.7);
        assert!(g["evidence"].as_array().is_some_and(|e| !e.is_empty()));
        assert_eq!(g["evidence"][1]["signal"], "transcript_mention");

        // And the digest never states the guess as a fact.
        let mut out = Vec::new();
        digest(&report, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("probably `cmdsessions` — a GUESS (confidence 0.70)"),
            "{text}"
        );
    }

    /// `--since`/`--until` reach `prune::partition` through `discover`, and
    /// what mtime cannot rule out the in-file span can.
    #[test]
    fn since_and_until_filter_the_list() {
        let td = TempDir::new().unwrap();
        let old = write_session(td.path(), "old.jsonl", &[("2026-08-01T00:00:00Z", "old")]);
        let new = write_session(td.path(), "new.jsonl", &[("2026-08-19T00:00:00Z", "new")]);
        // mtimes match the content, which is what an append-only file gives.
        for (p, ts) in [
            (&old, "2026-08-01T00:00:00Z"),
            (&new, "2026-08-19T00:00:00Z"),
        ] {
            let f = fs::File::options().write(true).open(p).unwrap();
            f.set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::from(parse_ts(ts).unwrap())),
            )
            .unwrap();
        }
        let worktrees = [wt("/r", "main")];
        let dirs = [td.path().to_path_buf()];
        let names = |w: TimeWindow| -> Vec<PathBuf> {
            collect(&only_generic(), Path::new("/r"), &worktrees, &dirs, &w)
                .sessions
                .into_iter()
                .map(|s| s.path)
                .collect()
        };

        assert_eq!(
            names(TimeWindow::default()).len(),
            2,
            "unbounded takes both"
        );
        assert_eq!(
            names(TimeWindow {
                since: parse_ts("2026-08-10T00:00:00Z").ok(),
                until: None
            }),
            vec![new.clone()],
            "--since prunes the old file on mtime"
        );
        assert_eq!(
            names(TimeWindow {
                since: None,
                until: parse_ts("2026-08-10T00:00:00Z").ok()
            }),
            vec![old],
            "--until cannot prune on mtime, so the span does it"
        );
        assert!(names(TimeWindow {
            since: parse_ts("2027-01-01T00:00:00Z").ok(),
            until: None
        })
        .is_empty());
    }

    /// A connector that reports a `cwd`, which `generic_jsonl` never does.
    /// Everything downstream of the cwd — the filter, the worktree name, the
    /// topology signal — needs one.
    struct Cwds(Vec<(PathBuf, Option<PathBuf>)>);

    impl Connector for Cwds {
        fn name(&self) -> &'static str {
            "test"
        }
        fn roots(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn discover(&self, _dirs: &[PathBuf], _window: &TimeWindow) -> Vec<SessionMeta> {
            self.0
                .iter()
                .map(|(path, cwd)| SessionMeta {
                    path: path.clone(),
                    cwd: cwd.clone(),
                    first_ts: parse_ts("2026-08-19T07:00:00Z").ok(),
                    last_ts: parse_ts("2026-08-19T08:00:00Z").ok(),
                    turn_count: 1,
                    parse_failures: 0,
                    connector: "test".to_string(),
                })
                .collect()
        }
        fn read(&self, _path: &Path) -> Vec<Turn> {
            Vec::new()
        }
    }

    /// The spec's filter — "sessions whose cwd resolves into the repo or its
    /// worktrees" — and the fleet guess that rides on it.
    #[test]
    fn cwd_decides_membership_and_drives_the_guess() {
        let td = TempDir::new().unwrap();
        let inside = write_session(
            td.path(),
            "in.jsonl",
            &[("2026-08-19T07:00:00Z", "no names")],
        );
        let named = write_session(
            td.path(),
            "named.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions")],
        );
        let elsewhere = write_session(td.path(), "out.jsonl", &[("2026-08-19T07:00:00Z", "x")]);
        let quoting = write_session(
            td.path(),
            "quoting.jsonl",
            &[(
                "2026-08-19T07:00:00Z",
                "PACT_AGENT=cmdsessions and PACT_AGENT=join",
            )],
        );

        let worktrees = [
            wt("/r", "main"),
            wt("/r/wt/cmdsessions", "cmdsessions"),
            wt("/r/wt/gone", "gone"),
        ];
        let connectors: Vec<Box<dyn Connector>> = vec![Box::new(Cwds(vec![
            (inside.clone(), Some(PathBuf::from("/r/src"))),
            (named.clone(), Some(PathBuf::from("/r/wt/cmdsessions"))),
            (elsewhere, Some(PathBuf::from("/some/other/repo"))),
            (quoting.clone(), Some(PathBuf::from("/r/wt/cmdsessions"))),
        ]))];

        let r = collect(
            &connectors,
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );

        assert_eq!(r.discovered, 4);
        assert_eq!(r.excluded_by_cwd, 1, "the cwd in another repo is dropped");
        let by_path = |p: &Path| r.sessions.iter().find(|s| s.path == p).expect("kept");

        // Repo-root cwd: a member, but §4.3 says it names no one.
        let plain = by_path(&inside);
        assert_eq!(plain.repo_match, RepoMatch::Cwd);
        assert_eq!(plain.worktree.as_deref(), Some("main"));
        assert_eq!(plain.fleet_identity_guess.agent, None);
        assert_eq!(plain.fleet_identity_guess.confidence.value(), 0.0);
        assert_eq!(
            plain.fleet_identity_guess.evidence.len(),
            2,
            "unknown is an answer with reasons, not a shrug"
        );

        // Both signals agree — the strongest this ever gets, and still a guess.
        let strong = by_path(&named);
        assert_eq!(strong.worktree.as_deref(), Some("cmdsessions"));
        assert_eq!(
            strong.fleet_identity_guess.agent.as_deref(),
            Some("cmdsessions")
        );
        assert_eq!(strong.fleet_identity_guess.confidence.value(), 0.90);

        // Two names in one transcript: the worktree breaks the tie, at a
        // visibly lower confidence.
        let ambiguous = by_path(&quoting);
        assert_eq!(
            ambiguous.fleet_identity_guess.agent.as_deref(),
            Some("cmdsessions")
        );
        assert_eq!(ambiguous.fleet_identity_guess.confidence.value(), 0.80);
        assert!(ambiguous
            .fleet_identity_guess
            .evidence
            .iter()
            .any(|e| e.detail.contains("names 2 identities")));

        // A worktree git lists but the filesystem does not is a warning, never
        // a silent omission.
        let mut worktrees = worktrees;
        worktrees[2].alive = false;
        let r = collect(
            &connectors,
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("`gone`"), "{:?}", r.warnings);
    }

    #[test]
    fn pact_agent_names_reads_the_assignment_and_nothing_else() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("t.jsonl");
        fs::write(
            &path,
            "export PACT_AGENT=cmdsessions BEADS_ACTOR=cmdsessions\n\
             PACT_AGENT=wave-2.a, then PACT_AGENT= nothing\n",
        )
        .unwrap();
        assert_eq!(
            pact_agent_names(&path).into_iter().collect::<Vec<_>>(),
            ["cmdsessions", "wave-2.a"]
        );
        // A file that is not there names no one, and does not fail.
        assert!(pact_agent_names(&td.path().join("nope")).is_empty());
    }
}
