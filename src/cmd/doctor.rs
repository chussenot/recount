//! `recount doctor`
//!
//! Spec: "connector roots, session counts, parse-failure tallies,
//! join-readiness (events present? worktree paths alive?)."
//!
//! Doctor is the command that must never fail: it is what you run when nothing
//! else works. In a bare directory with no `~/.claude`, no `.pact`, and no git
//! repo it reports four absences and exits 0 — the proof obligation is "calm
//! degradation, never a panic".
//!
//! # Every input is a finding, never an error
//!
//! [`run`] returns `Ok(EXIT_OK)` for every input, and it is the only command
//! that promises that. The rest of the tool is allowed to say "that is not a
//! repo" and stop; doctor is the command a human reaches for *because*
//! something already said that, so refusing to run would leave them with no
//! way to find out why. Concretely: [`crate::worktree::worktrees`] returns
//! `Err` for a `--repo` that is not a git repository (a deliberate W1
//! decision — "no sessions found" is the wrong way to say "that is not a
//! repo"), and the integration obligation runs every command in a bare
//! tempdir, which is not one. Doctor absorbs that `Err` as the finding "not a
//! git repository" and keeps going: connector roots exist independently of any
//! repo, and so do the session counts under them.
//!
//! Nothing here is graded, either. A pruned worktree, a corrupt ledger and a
//! missing `~/.claude` all print at the same volume and all exit `0`. The
//! `ready` flag says whether a join is *possible*; the findings say why not.
//! Doctor reports, it does not judge — `at` is the command that refuses.
//!
//! # Two scopes, and why they differ
//!
//! The connector section is a **whole-machine** sweep: `roots()` with an
//! unbounded window, exactly as [`crate::connector`]'s trait doc specifies,
//! because "how many sessions does this connector see at all, and how many are
//! damaged?" is not a question about any one repo. The join-readiness section
//! is **repo-scoped** and opens nothing at all — it only needs to know which
//! encoded directories exist and how many `*.jsonl` files sit under them.
//!
//! # Output: the house envelope, JSON on stdout, digest on stderr
//!
//! The spec says of every command "all --json; human digest second", and
//! [`crate::cmd::sessions`] settled what that means — its module doc is the
//! statement of the rule, this is one of the four commands that follow it:
//!
//! * **without `--json`** — the digest, on stdout, and nothing else.
//! * **with `--json`** — the JSON document on stdout *alone*, the same digest
//!   on stderr, printed second. `recount doctor --json | jq` works and a person
//!   watching still gets the reading, because the digest is not what `--json`
//!   replaces.
//!
//! The document carries the same `{command, schema, ...payload}` envelope every
//! command emits, so a script can tell two recount documents apart without
//! guessing from their keys. `schema` moves only when a field leaves or changes
//! meaning; added fields do not bump it and readers must ignore what they do
//! not know.
//!
//! ponytail: the machine sweep reads every session file the connector can see,
//! so on a large corpus doctor costs a full scan. That is the price of a real
//! `parse_failures` tally — mtime cannot tell you a file is damaged. If it ever
//! hurts, give doctor the same `--since/--until` the other commands have rather
//! than faking the number.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::cli::DoctorArgs;
use crate::cmd::EXIT_OK;
use crate::connector::{self, Connector};
use crate::model::TimeWindow;
use crate::pact_events::{self, Events};
use crate::worktree::{self, Worktree};

/// Bumped only when a field leaves or changes meaning. See the module doc.
const SCHEMA: u32 = 1;

/// The whole report. `--json` prints this; the human digest is a rendering of
/// the same struct, so the two can never disagree.
#[derive(Debug, Serialize)]
pub struct Report {
    /// The house envelope, first two fields, same as every other command.
    pub command: &'static str,
    pub schema: u32,
    /// `--repo`, canonicalised when it exists, verbatim when it does not.
    pub repo: PathBuf,
    pub connectors: Vec<ConnectorReport>,
    pub join_readiness: JoinReadiness,
    /// Everything worth a human's attention, in the order it was discovered.
    /// One line each, never an error, never a warning level: a finding.
    pub findings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectorReport {
    pub name: String,
    /// Where this connector looks, present or not — reported verbatim,
    /// including the absent ones.
    pub roots: Vec<RootReport>,
    /// Sessions visible under `roots` with an unbounded window.
    pub sessions: usize,
    /// Lines those sessions could not parse. Damage, not deliberate skips.
    pub parse_failures: usize,
}

#[derive(Debug, Serialize)]
pub struct RootReport {
    pub path: PathBuf,
    pub exists: bool,
}

/// Is a join even possible here, and why not.
#[derive(Debug, Serialize)]
pub struct JoinReadiness {
    /// A join needs all four: a ledger with events, a git topology to resolve
    /// `invoked_from` through, a live checkout, and sessions to join to.
    pub ready: bool,
    pub repo_is_git: bool,
    pub events_path: PathBuf,
    /// The file exists. Distinct from `events == 0`, which is why this is
    /// asked of the filesystem rather than of `pact_events::read` — a missing
    /// ledger and an empty one are the same `Events` by design.
    pub events_present: bool,
    pub events: usize,
    pub event_parse_failures: usize,
    pub worktrees: Vec<WorktreeReport>,
    /// Distinct `*.jsonl` files under all of this repo's session directories.
    /// No file is opened to count them.
    pub repo_sessions: usize,
}

#[derive(Debug, Serialize)]
pub struct WorktreeReport {
    pub name: String,
    pub path: PathBuf,
    /// Does the checkout still exist on disk? A `false` here is the pruned
    /// worktree `at` refuses on, and doctor is where a human finds that out.
    pub alive: bool,
    pub session_dir: PathBuf,
    pub session_dir_exists: bool,
    pub sessions: usize,
}

pub fn run(args: &DoctorArgs, json: bool) -> Result<u8> {
    let report = report(&args.repo, &connector::registry());
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(EXIT_OK)
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
/// See the module doc — this is `sessions`' rule, and the sinks are arguments
/// for the same reason: a rule nothing tests is a suggestion.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    let text = digest(report);
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors are ignored on purpose: `recount doctor --json | head`
        // closing the pipe is not a failure of the command — and doctor of all
        // commands does not fail.
        let _ = writeln!(out, "{rendered}");
        let _ = write!(err, "{text}");
    } else {
        let _ = write!(out, "{text}");
    }
    Ok(())
}

/// The report, with the connector list passed in rather than read from
/// [`connector::registry`].
///
/// One parameter, not an abstraction: it is what lets the tests drive doctor
/// over a known connector set instead of whatever this machine happens to
/// have installed in `~/.claude`.
fn report(repo: &Path, connectors: &[Box<dyn Connector>]) -> Report {
    let mut findings = Vec::new();

    let repo = repo.canonicalize().unwrap_or_else(|e| {
        findings.push(format!("--repo {}: {e}", repo.display()));
        repo.to_path_buf()
    });

    let connectors = connectors
        .iter()
        .map(|c| connector_report(c.as_ref(), &mut findings))
        .collect();
    let join_readiness = join_readiness(&repo, &mut findings);

    Report {
        command: "doctor",
        schema: SCHEMA,
        repo,
        connectors,
        join_readiness,
        findings,
    }
}

fn connector_report(c: &dyn Connector, findings: &mut Vec<String>) -> ConnectorReport {
    let roots = c.roots();
    let roots: Vec<RootReport> = roots
        .into_iter()
        .map(|path| RootReport {
            exists: path.exists(),
            path,
        })
        .collect();
    for root in roots.iter().filter(|r| !r.exists) {
        findings.push(format!(
            "connector {}: root {} does not exist — that connector can see no sessions on this machine",
            c.name(),
            root.path.display()
        ));
    }

    // Unbounded: doctor asks "everything", and `prune` then makes no stat call
    // at all. Absent roots cost nothing — `candidates` skips them silently.
    let present: Vec<PathBuf> = roots
        .iter()
        .filter(|r| r.exists)
        .map(|r| r.path.clone())
        .collect();
    let sessions = c.discover(&present, &TimeWindow::default());
    let parse_failures: usize = sessions.iter().map(|s| s.parse_failures).sum();
    if parse_failures > 0 {
        let damaged = sessions.iter().filter(|s| s.parse_failures > 0).count();
        findings.push(format!(
            "connector {}: {parse_failures} unparseable lines across {damaged} of {} sessions — those transcripts are damaged, not merely skipped",
            c.name(),
            sessions.len()
        ));
    }

    ConnectorReport {
        name: c.name().to_string(),
        roots,
        sessions: sessions.len(),
        parse_failures,
    }
}

fn join_readiness(repo: &Path, findings: &mut Vec<String>) -> JoinReadiness {
    let events_path = pact_events::events_path(repo);
    let events_present = events_path.exists();
    let events = match pact_events::read(&events_path) {
        Ok(e) => e,
        // Exists but unreadable: a permission bit, a directory where a file
        // should be. Still a finding.
        Err(e) => {
            findings.push(format!("{}: {e:#}", events_path.display()));
            Events::default()
        }
    };
    if !events_present {
        findings.push(format!(
            "no ledger at {} — pact has never run here, so there is nothing to join sessions to",
            events_path.display()
        ));
    } else if events.line_count == 0 {
        findings.push(format!(
            "{} is empty — pact has run here but recorded no events",
            events_path.display()
        ));
    }
    if events.parse_failures > 0 {
        findings.push(format!(
            "{}: {} of {} lines did not parse as events",
            events_path.display(),
            events.parse_failures,
            events.line_count
        ));
    }

    // The W1 decision doctor exists to absorb: not-a-git-repo is an `Err` for
    // every other caller and a finding here.
    let listed = match worktree::worktrees(repo) {
        Ok(w) => w,
        Err(e) => {
            // First line only: git's "not a git repository" spans three lines
            // and a finding is one line. The dropped remainder is git's advice
            // about GIT_DISCOVERY_ACROSS_FILESYSTEM, not information about the
            // repo.
            let e = format!("{e:#}");
            let e = e.lines().next().unwrap_or_default();
            findings.push(format!(
                "{e} — worktree paths cannot be resolved, so the join has no topology to run through"
            ));
            Vec::new()
        }
    };
    let repo_is_git = !listed.is_empty();

    let worktrees: Vec<WorktreeReport> = listed.iter().map(worktree_report).collect();
    for w in &worktrees {
        if !w.alive {
            findings.push(format!(
                "worktree `{}` at {} is listed by git but its directory is gone — `at` refuses on exactly this; `git worktree prune` is the fix",
                w.name,
                w.path.display()
            ));
        }
        if !w.session_dir_exists {
            findings.push(format!(
                "worktree `{}`: no session directory at {} — no transcripts were ever recorded for that checkout",
                w.name,
                w.session_dir.display()
            ));
        }
    }

    // Counted over all the directories at once so the non-injective encoding
    // (two checkouts, one encoded name) cannot double-count a file.
    let dirs: Vec<PathBuf> = listed.into_iter().map(|w| w.session_dir).collect();
    let repo_sessions = connector::candidates(&dirs).len();
    if repo_is_git && repo_sessions == 0 {
        findings.push(format!(
            "no sessions under any of this repo's {} session director{} — there is no testimony to join",
            dirs.len(),
            if dirs.len() == 1 { "y" } else { "ies" }
        ));
    }

    JoinReadiness {
        ready: repo_is_git
            && events_present
            && !events.events.is_empty()
            && worktrees.iter().any(|w| w.alive)
            && repo_sessions > 0,
        repo_is_git,
        events_path,
        events_present,
        events: events.events.len(),
        event_parse_failures: events.parse_failures,
        worktrees,
        repo_sessions,
    }
}

fn worktree_report(w: &Worktree) -> WorktreeReport {
    WorktreeReport {
        name: w.name.clone(),
        path: w.path.clone(),
        alive: w.alive,
        session_dir_exists: w.session_dir.exists(),
        sessions: connector::candidates(std::slice::from_ref(&w.session_dir)).len(),
        session_dir: w.session_dir.clone(),
    }
}

/// The human digest: the same four things, in the spec's order, then the
/// findings. Built into a `String` rather than printed so a test can read it.
fn digest(r: &Report) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(s, "recount doctor — {}", r.repo.display());

    let _ = writeln!(s, "\nconnectors");
    for c in &r.connectors {
        let _ = writeln!(
            s,
            "  {:<14} {} sessions, {} parse failures",
            c.name, c.sessions, c.parse_failures
        );
        if c.roots.is_empty() {
            // generic_jsonl, by design: any directory you point it at.
            let _ = writeln!(s, "  {:<14} roots: none declared", "");
        }
        for root in &c.roots {
            let _ = writeln!(
                s,
                "  {:<14} root: {} ({})",
                "",
                root.path.display(),
                if root.exists { "exists" } else { "MISSING" }
            );
        }
    }

    let j = &r.join_readiness;
    let _ = writeln!(s, "\njoin-readiness");
    let _ = writeln!(
        s,
        "  events         {} ({})",
        j.events_path.display(),
        if j.events_present {
            format!(
                "present, {} events, {} unparseable lines",
                j.events, j.event_parse_failures
            )
        } else {
            "ABSENT".to_string()
        }
    );
    if j.repo_is_git {
        let alive = j.worktrees.iter().filter(|w| w.alive).count();
        let _ = writeln!(
            s,
            "  worktrees      {} listed, {alive} alive, {} sessions under them",
            j.worktrees.len(),
            j.repo_sessions
        );
        for w in &j.worktrees {
            let _ = writeln!(
                s,
                "                 {:<16} {:<5} {:>4} sessions  {}",
                w.name,
                if w.alive { "alive" } else { "GONE" },
                w.sessions,
                w.path.display()
            );
        }
    } else {
        let _ = writeln!(s, "  worktrees      unresolvable: not a git repository");
    }
    let _ = writeln!(
        s,
        "  verdict        {}",
        if j.ready {
            "READY — a join can run here"
        } else {
            "NOT READY — see findings"
        }
    );

    let _ = writeln!(s, "\nfindings");
    if r.findings.is_empty() {
        let _ = writeln!(s, "  none");
    }
    for f in &r.findings {
        let _ = writeln!(s, "  - {f}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// `claude_code`'s methods are `todo!()` until recount-hwk.29 lands, so
    /// every test here drives doctor over the connector that is real. The
    /// connector *section* of the report is therefore exercised only for
    /// `generic_jsonl` (no roots, no sessions); the machine sweep is verified
    /// at the wave barrier.
    fn connectors() -> Vec<Box<dyn Connector>> {
        vec![Box::new(crate::connector::generic_jsonl::GenericJsonl)]
    }

    fn check(repo: &Path) -> Report {
        report(repo, &connectors())
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

    fn repo() -> (TempDir, PathBuf) {
        let td = TempDir::new().unwrap();
        let repo = td.path().canonicalize().unwrap().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "root"]);
        (td, repo)
    }

    fn ledger(repo: &Path, body: &str) {
        let p = pact_events::events_path(repo);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    fn found(r: &Report, needle: &str) -> bool {
        r.findings.iter().any(|f| f.contains(needle))
    }

    /// The headline: nothing exists, everything is reported, nothing panics.
    #[test]
    fn a_bare_directory_is_a_full_report_and_not_an_error() {
        let td = TempDir::new().unwrap();
        let r = check(td.path());

        assert!(!r.join_readiness.ready);
        assert!(!r.join_readiness.repo_is_git);
        assert!(!r.join_readiness.events_present);
        assert_eq!(r.join_readiness.events, 0);
        assert!(r.join_readiness.worktrees.is_empty());
        assert_eq!(r.join_readiness.repo_sessions, 0);
        assert!(found(&r, "not a git repository"), "{:#?}", r.findings);
        assert!(found(&r, "pact has never run here"), "{:#?}", r.findings);
        // git's refusal is three lines; a finding is one.
        assert!(
            r.findings.iter().all(|f| !f.contains('\n')),
            "{:#?}",
            r.findings
        );

        // Both renderings work on the emptiest possible input.
        let d = digest(&r);
        assert!(d.contains("NOT READY"), "{d}");
        assert!(d.contains("not a git repository"), "{d}");
        serde_json::to_string(&r).unwrap();
    }

    /// A `--repo` that does not exist at all is still a report.
    #[test]
    fn a_repo_that_does_not_exist_is_a_finding_not_a_failure() {
        let td = TempDir::new().unwrap();
        let r = check(&td.path().join("nope"));
        assert!(!r.join_readiness.ready);
        assert!(found(&r, "nope"), "{:#?}", r.findings);
    }

    /// The spec's integration obligation: events but NO sessions.
    #[test]
    fn a_git_repo_with_events_and_no_sessions_is_not_ready_and_says_why() {
        let (_td, repo) = repo();
        ledger(
            &repo,
            r#"{"at":"2026-08-19T07:03:12.694912822+00:00","agent":"a","invoked_from":"main"}
"#,
        );
        let r = check(&repo);

        assert!(r.join_readiness.repo_is_git);
        assert!(r.join_readiness.events_present);
        assert_eq!(r.join_readiness.events, 1);
        assert_eq!(r.join_readiness.event_parse_failures, 0);
        assert_eq!(r.join_readiness.worktrees.len(), 1);
        assert!(r.join_readiness.worktrees[0].alive);
        assert_eq!(r.join_readiness.repo_sessions, 0);
        assert!(!r.join_readiness.ready, "no sessions, no join");
        assert!(found(&r, "no testimony to join"), "{:#?}", r.findings);
    }

    /// Present-but-empty and absent are different findings, which is the whole
    /// reason `events_present` asks the filesystem instead of `read`.
    #[test]
    fn an_empty_ledger_is_distinguished_from_a_missing_one() {
        let (_td, repo) = repo();
        ledger(&repo, "");
        let r = check(&repo);
        assert!(r.join_readiness.events_present);
        assert_eq!(r.join_readiness.events, 0);
        assert!(found(&r, "recorded no events"), "{:#?}", r.findings);
        assert!(!found(&r, "pact has never run here"));
    }

    #[test]
    fn a_corrupt_ledger_is_tallied_not_fatal() {
        let (_td, repo) = repo();
        ledger(
            &repo,
            "not json at all\n{\"at\":\"2026-08-19T07:03:12+00:00\",\"agent\":\"a\"}\n{\"at\":\"trunc",
        );
        let r = check(&repo);
        assert_eq!(r.join_readiness.events, 1);
        assert_eq!(r.join_readiness.event_parse_failures, 2);
        assert!(found(&r, "2 of 3 lines did not parse"), "{:#?}", r.findings);
    }

    #[test]
    fn a_pruned_worktree_is_reported_as_the_thing_at_refuses_on() {
        let (_td, repo) = repo();
        let gone = repo.join("wt").join("gone");
        git(
            &repo,
            &["worktree", "add", "-q", "-b", "g", gone.to_str().unwrap()],
        );
        fs::remove_dir_all(&gone).unwrap();

        let r = check(&repo);
        let w = &r.join_readiness.worktrees;
        assert_eq!(w.len(), 2, "the dead worktree must still be listed");
        assert!(w[0].alive);
        assert!(!w[1].alive);
        assert!(found(&r, "its directory is gone"), "{:#?}", r.findings);
        assert!(digest(&r).contains("GONE"));
    }

    /// Sessions are counted from directory entries, and a session directory
    /// that is not there is its own finding.
    #[test]
    fn sessions_under_a_worktrees_encoded_directory_are_counted_without_opening_them() {
        let (_td, repo) = repo();
        let mut r = check(&repo);
        assert_eq!(r.join_readiness.repo_sessions, 0);
        assert!(found(&r, "no session directory at"), "{:#?}", r.findings);

        // Create the encoded directory this repo's transcripts would live in.
        let dir = r.join_readiness.worktrees[0].session_dir.clone();
        let Some(_cleanup) = make_session_dir(&dir) else {
            return; // $HOME is not writable here; the rest is unobservable.
        };
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("a.jsonl"), "").unwrap();
        fs::write(dir.join("nested").join("b.jsonl"), "").unwrap();
        fs::write(dir.join("notes.txt"), "").unwrap();

        r = check(&repo);
        assert_eq!(r.join_readiness.repo_sessions, 2);
        assert_eq!(r.join_readiness.worktrees[0].sessions, 2);
        assert!(r.join_readiness.worktrees[0].session_dir_exists);
        // Still not ready: no ledger.
        assert!(!r.join_readiness.ready);
    }

    /// Everything present at once: the only shape that is `ready`.
    #[test]
    fn ready_needs_a_ledger_a_live_worktree_and_a_session() {
        let (_td, repo) = repo();
        ledger(
            &repo,
            "{\"at\":\"2026-08-19T07:03:12+00:00\",\"agent\":\"a\",\"invoked_from\":\"main\"}\n",
        );
        let dir = check(&repo).join_readiness.worktrees[0].session_dir.clone();
        let Some(_cleanup) = make_session_dir(&dir) else {
            return;
        };
        fs::write(dir.join("a.jsonl"), "").unwrap();

        let r = check(&repo);
        assert!(r.join_readiness.ready, "{:#?}", r.findings);
        assert!(digest(&r).contains("READY — a join can run here"));
    }

    /// The house rule of `crate::cmd::sessions`, which doctor predated: the
    /// document goes to stdout alone, enveloped, and the digest follows it on
    /// stderr rather than being replaced by it.
    #[test]
    fn json_goes_to_stdout_and_the_digest_to_stderr() {
        let td = TempDir::new().unwrap();
        let r = check(td.path());

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, true, &mut out, &mut err).unwrap();
        let doc: serde_json::Value =
            serde_json::from_slice(&out).expect("stdout is JSON and nothing else");
        assert_eq!(doc["command"], "doctor");
        assert_eq!(doc["schema"], 1);
        // The envelope is a shape change, not a content change: everything
        // doctor has ever reported is still there, under the same names.
        for key in ["repo", "connectors", "join_readiness", "findings"] {
            assert!(!doc[key].is_null(), "{key}");
        }
        assert_eq!(doc["join_readiness"]["ready"], false);
        assert!(
            String::from_utf8_lossy(&err).contains("recount doctor"),
            "the digest is second, not gone"
        );

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, false, &mut out, &mut err).unwrap();
        assert!(err.is_empty(), "no --json means nothing on stderr");
        assert!(String::from_utf8_lossy(&out).contains("recount doctor"));
        assert!(
            serde_json::from_slice::<serde_json::Value>(&out).is_err(),
            "the digest is not JSON"
        );
    }

    /// Make a repo's encoded session directory, and take it away again on
    /// drop — even on panic. These live under `$HOME/.claude/projects`, which
    /// is outside any `TempDir`, so no `TempDir` will clean them up. The name
    /// encodes a fresh tempdir path, so it cannot collide with a real one.
    /// `None` where `$HOME` is not writable: a skip, not a failure.
    fn make_session_dir(dir: &Path) -> Option<impl Drop> {
        struct Rm(PathBuf);
        impl Drop for Rm {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fs::create_dir_all(dir).ok()?;
        Some(Rm(dir.to_path_buf()))
    }
}
