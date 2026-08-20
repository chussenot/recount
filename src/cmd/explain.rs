//! `recount explain --repo <p> --event-line <N>`
//!
//! Spec: "read event N from .pact/events.jsonl, run `at` on it — one command
//! from finding to testimony."
//!
//! "run `at` on it" is literal: this module reads the event, builds an
//! [`AtArgs`] from its `agent` and `at`, and calls [`crate::cmd::at::run_with`].
//! It does not reimplement the join, and it inherits `at`'s exit codes and
//! refusal reasons for free. `N` is 1-based, matching [`crate::pact_events`];
//! out of range is a clean error naming the line count.
//!
//! # Why `run_with` and not `run`
//!
//! A ledger row carries things the `at` CLI has no flag for, and `explain` is
//! the only command that has them, because it is the only command that reads an
//! event. Three of them are links of the topological join: `invoked_from` (the
//! worktree the agent acted from) and, inside `detail`, the bead id that the
//! transcript-mention tiebreaker looks for. The fourth is not a signal to score
//! at all — it is an answer. A pact 0.11.0 row names the transcript it was
//! written from (`harness_session` + `harness_subagent`), and
//! [`crate::join::resolve`] takes that as the keyed tier: a path join and a
//! `stat`, no cwd, no clock, no score.
//!
//! **That handoff is why this line exists.** `at` invoked from the CLI has no
//! flag that can name a transcript, so its every answer is inferred; the keyed
//! tier is reachable from a real command only through here. Calling plain
//! [`crate::cmd::at::run`] would not merely score a *worse* join than the one
//! the finding already handed us — it would throw away a certainty and replace
//! it with a guess.
//!
//! # Where the event itself appears
//!
//! `at` owns stdout: under `--json` it prints one document there, and its
//! digest to stderr second. `explain` does not print a second document beside
//! it — a stdout carrying two JSON values stops being a thing you can pipe to
//! `jq` or `json.loads`, and that contract is worth more than convenience.
//!
//! So the event context rides with the *digest*, on whichever stream the digest
//! is going to (stdout when human, stderr under `--json`), printed above `at`'s
//! own. Both streams then read in the order the question was asked: this is the
//! event, and here is the session behind it. Nothing is lost from the machine
//! document either — `agent`, `ts` and `window` are in it as `at`'s inputs, and
//! `invoked_from` and the bead id show up in the evidence they earn.

use std::io::{self, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::cli::{AtArgs, ExplainArgs};
use crate::pact_events::{self, Event};

pub fn run(args: &ExplainArgs, json: bool) -> Result<u8> {
    let path = pact_events::events_path(&args.repo);
    let event = finding(&path, args.event_line)?;

    let at_args = AtArgs {
        repo: args.repo.clone(),
        agent: event.agent.clone(),
        ts: event.at,
        window: args.window,
    };

    // Write errors are ignored for the same reason `at` ignores them: a closed
    // pipe is not a failed command.
    if json {
        let _ = context(&event, &path, &mut io::stderr());
    } else {
        let _ = context(&event, &path, &mut io::stdout());
    }

    // The whole event, not the fields picked out of it: `at` wants
    // `invoked_from` for the topological chain, `harness_session` and
    // `harness_subagent` for the keyed tier, and `harness`, `model` and
    // `branch` for its own header — pact stamps them as one unit and they
    // travel as one. `bead` stays separate because it is not a field: it is
    // this module's reading of `detail`.
    crate::cmd::at::run_with(&at_args, Some(&event), bead(&event), json)
}

/// Event `n` of the ledger, or the reason there is none — split from [`run`]
/// so every way of asking for a line that is not there can be asserted without
/// printing a report to the process's own stdout.
fn finding(path: &Path, n: usize) -> Result<Event> {
    // `pact_events::read` reads a missing ledger as an empty one, on purpose.
    // Here that would come out as "the ledger has 0 lines", which sends an
    // auditor looking for a truncated file that was never there.
    if !path.exists() {
        bail!(
            "no pact ledger at {} — nothing to explain here",
            path.display()
        );
    }
    pact_events::read(path)?
        .line(n)
        .cloned()
        .with_context(|| format!("in {}", path.display()))
}

/// The finding, in the words the ledger used, above `at`'s answer.
fn context(e: &Event, path: &Path, w: &mut dyn Write) -> io::Result<()> {
    writeln!(w, "recount explain — {} line {}", path.display(), e.line)?;
    writeln!(w, "  event    {} · agent `{}`", e.at.to_rfc3339(), e.agent)?;
    if let Some(from) = &e.invoked_from {
        writeln!(w, "  from     worktree `{from}` — the join's first link")?;
    }
    if let Some(p) = &e.path {
        writeln!(w, "  about    {p}")?;
    }
    if let Some(d) = &e.detail {
        writeln!(w, "  detail   {d}")?;
    }
    chain(e, w)?;
    match bead(e) {
        Some(b) => writeln!(w, "  bead     {b} — also looked for in the transcript")?,
        None => writeln!(w, "  bead     none in detail — agent name only")?,
    }
    writeln!(w)
}

/// The pact 0.11.0 attribution chain, in the words the row used.
///
/// Every field is optional and every one is printed only if the row carries it:
/// a pre-0.11.0 ledger prints none of this and is not a degraded ledger, it is
/// an older one. Absent fields are silent rather than `none`, unlike the bead
/// line below — a missing bead means the tiebreaker had nothing to look for,
/// which changes how you read the evidence, whereas a missing chain just means
/// the event predates it.
///
/// `model` is marked `(declared)` and always will be. pact records what the
/// spawner asked for in `PACT_MODEL`; whether that is the model that actually
/// answered is a question only the transcript can settle, and settling it is
/// `at`'s job, in `at`'s header, against the session it resolved. Printing the
/// declaration unmarked here would read as an observation.
fn chain(e: &Event, w: &mut dyn Write) -> io::Result<()> {
    if let Some(h) = &e.harness {
        writeln!(w, "  harness  {h}")?;
    }
    if let Some(m) = &e.model {
        writeln!(w, "  model    {m} (declared)")?;
    }
    if let Some(s) = &e.harness_session {
        writeln!(w, "  session  {s}")?;
    }
    if let Some(sub) = &e.harness_subagent {
        writeln!(
            w,
            "  subagent {sub} — the event names its own transcript: this is the keyed tier"
        )?;
    }
    if let Some(b) = &e.branch {
        writeln!(w, "  branch   {b}")?;
    }
    Ok(())
}

/// The bead id `detail` opens with, if it opens with one.
///
/// pact's leases carry `"recount-hwk.23: the declared seam — ..."`, and the
/// tiebreaker searches transcripts for the id as a *word*: handing it the whole
/// sentence would match nothing. Anything that is not a single bare token
/// before a colon — `"merging wt/ci into master"` — is not an id, and `None`
/// leaves the join to score on the agent name alone.
fn bead(e: &Event) -> Option<&str> {
    let id = e.detail.as_deref()?.split_once(':')?.0;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::path::PathBuf;
    use std::process::Command;

    /// A git repo — `at` resolves worktrees before it joins — holding a ledger
    /// with one good line, one that does not parse, and one more good line, so
    /// a line number has to survive the junk in the middle.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .arg(dir.path())
            .status()
            .unwrap()
            .success());
        std::fs::create_dir(dir.path().join(".pact")).unwrap();
        std::fs::write(
            ledger(dir.path()),
            concat!(
                r#"{"at":"2026-08-19T07:13:57.829764529+00:00","agent":"skeleton","kind":"acquired","path":"src/cmd/explain.rs","detail":"recount-hwk.23: the declared seam","invoked_from":"skeleton","chain_hash":"7f5c","pact_version":"0.11.0","harness":"claude-code","model":"claude-opus-5","harness_session":"cd7c75eb-c39f-4be4-92ae-29efda5d7d69","harness_subagent":"a1ded78ef5b657fa6","branch":"wt/skeleton"}"#,
                "\n",
                "{ not json at all\n",
                r#"{"at":"2026-08-19T08:15:20Z","agent":"integration","detail":"merging wt/ci into master","invoked_from":"integration"}"#,
                "\n",
            ),
        )
        .unwrap();
        dir
    }

    fn ledger(repo: &Path) -> PathBuf {
        pact_events::events_path(repo)
    }

    fn err_of(repo: &Path, n: usize) -> String {
        format!("{:#}", finding(&ledger(repo), n).unwrap_err())
    }

    /// The whole command, on a repo with a ledger and no sessions at all: the
    /// event resolves, `at` runs, and its verdict — a reasoned refusal here —
    /// comes back as this command's exit code. Line 1 is a 0.11.0 row, so the
    /// tier that refuses is the keyed one, and it refuses because the named
    /// transcript is not on this disk — a different sentence from "no cwd
    /// match", and the one that proves the chain was passed. Exit code is the
    /// same either way, which is the point: the chain changes the *reason*, and
    /// on a machine that has the transcript it changes the answer. `at` prints
    /// to the process's own stdout, which the harness does not capture; the few
    /// lines in `cargo test`'s output are this command working.
    #[test]
    fn a_valid_event_line_produces_ats_outcome() {
        let dir = fixture();
        let args = ExplainArgs {
            repo: dir.path().to_path_buf(),
            event_line: 1,
            window: Duration::minutes(5),
        };
        assert_eq!(
            run(&args, false).unwrap(),
            crate::cmd::EXIT_NO_MATCH,
            "no sessions anywhere: `at` refuses, and `explain` inherits it"
        );
    }

    /// What `explain` adds to `at`'s answer: which event, and the two signals
    /// the `at` CLI has no flag for.
    #[test]
    fn the_event_is_visible_in_the_answer() {
        let dir = fixture();
        let e = finding(&ledger(dir.path()), 1).unwrap();
        assert_eq!(e.agent, "skeleton");
        assert_eq!(e.invoked_from.as_deref(), Some("skeleton"));
        assert_eq!(bead(&e), Some("recount-hwk.23"));

        let mut w = Vec::new();
        context(&e, &ledger(dir.path()), &mut w).unwrap();
        let s = String::from_utf8(w).unwrap();
        assert!(s.contains("line 1"), "{s}");
        assert!(s.contains("agent `skeleton`"), "{s}");
        assert!(s.contains("worktree `skeleton`"), "{s}");
        assert!(s.contains("src/cmd/explain.rs"), "{s}");
        assert!(s.contains("recount-hwk.23"), "{s}");
    }

    /// The chain the event declared, in `explain`'s own header — and the marker
    /// on the model, which is the difference between "this is what was asked
    /// for" and "this is what answered".
    #[test]
    fn the_chain_is_in_the_header_and_the_model_is_marked_declared() {
        let dir = fixture();
        let e = finding(&ledger(dir.path()), 1).unwrap();
        assert_eq!(e.harness_subagent.as_deref(), Some("a1ded78ef5b657fa6"));

        let mut w = Vec::new();
        context(&e, &ledger(dir.path()), &mut w).unwrap();
        let s = String::from_utf8(w).unwrap();
        assert!(s.contains("harness  claude-code"), "{s}");
        assert!(s.contains("model    claude-opus-5 (declared)"), "{s}");
        assert!(
            s.contains("session  cd7c75eb-c39f-4be4-92ae-29efda5d7d69"),
            "{s}"
        );
        assert!(s.contains("subagent a1ded78ef5b657fa6"), "{s}");
        assert!(s.contains("keyed tier"), "{s}");
        assert!(s.contains("branch   wt/skeleton"), "{s}");
    }

    /// A pre-0.11.0 row — line 3 — prints no chain at all rather than five
    /// `none`s. An older ledger is not a damaged one.
    #[test]
    fn a_row_without_a_chain_prints_no_chain() {
        let dir = fixture();
        let e = finding(&ledger(dir.path()), 3).unwrap();
        let mut w = Vec::new();
        context(&e, &ledger(dir.path()), &mut w).unwrap();
        let s = String::from_utf8(w).unwrap();
        for line in ["harness", "model", "session", "subagent", "branch"] {
            assert!(!s.contains(line), "{line} named on a pre-0.11.0 row: {s}");
        }
        // ... and what it does carry is still there.
        assert!(s.contains("agent `integration`"), "{s}");
    }

    #[test]
    fn line_zero_is_a_clean_error() {
        let dir = fixture();
        let e = err_of(dir.path(), 0);
        assert!(e.contains("1-based"), "{e}");
        assert!(e.contains("the ledger has 3"), "{e}");
    }

    #[test]
    fn past_the_end_names_the_line_count() {
        let dir = fixture();
        let e = err_of(dir.path(), 9);
        assert!(e.contains("no event on line 9"), "{e}");
        assert!(e.contains("the ledger has 3"), "{e}");
    }

    /// A line that exists but did not parse is a *different* error from one
    /// past the end: the next move is to look at the ledger, not at your
    /// arithmetic.
    #[test]
    fn a_line_that_did_not_parse_says_so() {
        let dir = fixture();
        let e = err_of(dir.path(), 2);
        assert!(e.contains("did not parse"), "{e}");
        assert!(!e.contains("1-based"), "not an out-of-range error: {e}");
        // ... and it did not shift the lines after it.
        assert_eq!(
            finding(&ledger(dir.path()), 3).unwrap().agent,
            "integration"
        );
    }

    /// An absent ledger is not "a ledger with zero lines", which is what the
    /// tolerant reader would otherwise report.
    #[test]
    fn a_missing_ledger_names_the_path_it_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let e = err_of(dir.path(), 1);
        assert!(e.contains("no pact ledger at"), "{e}");
        assert!(
            e.ends_with(".pact/events.jsonl — nothing to explain here"),
            "{e}"
        );
    }

    /// This repo's own ledger, which grows every time the fleet does anything:
    /// properties only, never a count.
    #[test]
    fn addresses_this_repos_own_ledger() {
        let path = ledger(Path::new(env!("CARGO_MANIFEST_DIR")));
        let first = finding(&path, 1).unwrap();
        assert_eq!(first.line, 1);
        assert!(!first.agent.is_empty());
        assert!(first.invoked_from.is_some(), "pact names the worktree");
        let count = pact_events::read(&path).unwrap().line_count;
        let e = format!("{:#}", finding(&path, count + 1).unwrap_err());
        assert!(e.contains(&count.to_string()), "{e}");
        // Some row of a fleet ledger carries a bead id in its detail; that is
        // the signal `explain` exists to hand the join.
        let all = pact_events::read(&path).unwrap();
        assert!(all.events.iter().any(|e| bead(e).is_some()));
    }

    #[test]
    fn a_bead_id_is_the_bare_token_or_nothing() {
        let ev = |detail: Option<&str>| Event {
            line: 1,
            at: chrono::Utc::now(),
            agent: "a".into(),
            path: None,
            invoked_from: None,
            detail: detail.map(str::to_string),
            harness: None,
            model: None,
            harness_session: None,
            harness_subagent: None,
            branch: None,
        };
        assert_eq!(
            bead(&ev(Some("recount-hwk.23: the declared seam — CLI surface"))),
            Some("recount-hwk.23")
        );
        assert_eq!(bead(&ev(Some("merging wt/ci into master"))), None);
        assert_eq!(bead(&ev(Some("wave 4: landed"))), None);
        assert_eq!(bead(&ev(Some(": no id"))), None);
        assert_eq!(bead(&ev(None)), None);
    }
}
