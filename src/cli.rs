//! The complete command-line surface, in one file, once.
//!
//! This is a coordination contract as much as a parser: every command and every
//! flag the spec's "Commands (all --json; human digest second)" section names
//! exists here, parsing, before the code behind it does. A bead that finds it
//! needs a flag that is not here changes the spec first.
//!
//! 0.2.0 adds one command the 0.1.0 spec did not name — `testify`, which reads
//! `pact audit --json` and annotates it. That is a *new* obligation from the
//! release contract rather than a flag some bead wanted mid-build, and it went
//! through the same gate: `--help`, `docs/cli.md` and the generated completions
//! in the bead that added it.
//!
//! `--json` is global rather than per-command because the spec applies it to
//! all of them ("all --json"), and a global flag can be written either side of
//! the subcommand, which is what people actually type.
//!
//! `completion` is the one command the spec does not name, and the one that
//! does NOT follow the "all --json" rule — its stdout is a shell program, so
//! `--json` is accepted and ignored. See [`crate::cmd::completion`] for why,
//! and the README's "Deviations from the spec's literal text".

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::model::{parse_duration, parse_ts, parse_when};

#[derive(Debug, Parser)]
#[command(
    name = "recount",
    version,
    about = "Session testimony for pact audits: join a coordination ledger to coding-agent transcripts",
    long_about = "\
recount answers `why did this agent do that?` by joining pact's coordination \
ledger (.pact/events.jsonl, the WHAT) to coding-agent session transcripts \
(the WHY), so an audit finding can be answered with the agent's own reasoning.

It owns no database. Session files are the only truth, and every scan starts \
by resolving --repo to the encoded session directories of the repo and its \
worktrees, before any file is opened.

A match is always scored, and always shows the evidence that scored it. \
`recount` would rather refuse with a reason than answer confidently from a \
signal that could not tell two sessions apart — see `Where recount is weak` \
in the README for the case where it does exactly that.",
    after_help = "Run `recount help <command>` for the long form, including exit codes.",
    after_long_help = "\
EXIT CODES
  0  answered
  1  no confident match (`at`, `explain`, `testify`), or a hard error
  2  usage error from the parser

EXAMPLES
  recount doctor
      Can a join even run here? Connector roots, session counts, and both
      join-readiness questions: is there a ledger, are the worktrees alive.

  recount sessions --repo . --since 2h
      The transcripts belonging to this repo in the last two hours, each with
      a fleet identity: stated where the ledger names the transcript, and
      marked as a guess everywhere else.

  recount explain --repo . --event-line 1
      Take event 1 out of .pact/events.jsonl and show the transcript behind
      it. One command from an audit finding to testimony.

  pact audit --check double-win --json | recount testify --repo .
      Every finding of an audit, annotated with the transcript behind it and
      the tier that named it. Exit 1 if any finding went unanswered, and the
      output says which.

  recount at --repo . --agent skeleton --ts 2026-08-19T07:13:57Z
      The session driving that identity at that moment, or exit 1: either a
      refusal naming which of the two reasons applies (no cwd match, no
      overlap), or a candidate that scored below the confidence bar, shown
      with the evidence that fell short.

  recount grep arkanoid --repo .
      Case-insensitive substring hits as (session, line, ts, role, snippet).
      Not a regex, by design.

  recount completion zsh > \"${ZDOTDIR:-$HOME}\"/.zsh/completions/_recount
      Generate the completion script for your shell."
)]
pub struct Cli {
    /// Machine-readable JSON output. The human digest is printed second.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sessions whose cwd resolves into the repo or its worktrees, each with a fleet identity attached: stated where the ledger names the transcript, guessed elsewhere
    Sessions(SessionsArgs),
    /// Resolve the session driving an identity at a moment; print the transcript window
    #[command(long_about = "\
Resolve the session driving a fleet identity at a moment, and print the
transcript window around it.

The join key is topological: the event's worktree, then the session cwd that
resolves into it, then a REQUIRED time-window overlap, then a transcript
mention as tiebreaker. Every answer carries a confidence and the evidence that
produced it; there is no code path that prints a bare answer.

EXIT CODES
  0  a confident match — the transcript window follows
  1  no confident match. Either a structural refusal naming which of the two
     reasons applies (`no cwd match` / `no overlap`), or a candidate that
     scored below the confidence bar, printed WITH its evidence so you can see
     what fell short. Both are answers, not failures.")]
    At(AtArgs),
    /// Case-insensitive substring hits with their coordinates
    Grep(GrepArgs),
    /// Read one event from .pact/events.jsonl and run `at` on it
    #[command(long_about = "\
Read one event out of .pact/events.jsonl and run `at` on it: one command from
an audit finding to the agent's own words.

The agent and the timestamp come from the event, and so does the worktree it
was invoked from and any bead id in its detail field — signals `at` has no
flags for, which is why `explain` resolves more than `at` can from the command
line alone.

Events are numbered 1-based over physical lines, so --event-line 5 is the line
`sed -n 5p` shows, whether or not line 5 parsed.

EXIT CODES: inherited from `at` unchanged — 0 confident match, 1 otherwise.
An --event-line past the end is a clean error naming the ledger's length.")]
    Explain(ExplainArgs),
    /// Read `pact audit --json` and annotate every finding with the transcript behind it
    #[command(long_about = "\
Read a `pact audit --json` document from stdin or --file, and for every finding
that carries an agent and a timestamp, run the join and attach the transcript
window and the tier that produced it.

  pact audit --check double-win --json | recount testify --repo .

The findings come back out ANNOTATED, not summarised: each one is echoed
verbatim under `finding` with the testimony beside it, so this is a stage in a
pipeline rather than the end of one.

pact's document has no `findings` key — each check writes its own array, and
the element shapes differ per check. testify reads every top-level array that
is not one of the four statistics arrays (`agents`, `annotations`, `per_agent`,
`top_contended`), and finds the agent and the moment inside an element by name
shape (`agent`, `*_agent`; `at`, `*_at`), so a check pact adds tomorrow is
testified without a change here.

A finding carrying no agent/timestamp pair — a topology mismatch names a
checkout, not an agent — is passed through untouched and counted, not failed.

EXIT CODES
  0  every finding that could be testified was, INCLUDING an empty findings
     set: a clean audit is not a failure
  1  some finding had an agent and a moment and still got no confident match.
     `untestified` lists exactly which, and why, on both streams
  2  usage error from the parser. Malformed input is not one of these: it is a
     hard error, one sentence on stderr and no document at all")]
    Testify(TestifyArgs),
    /// Connector roots, session counts, parse-failure tallies, join-readiness
    #[command(long_about = "\
Report whether a join can run here at all, and if not, why not: connector
roots, session counts, declined-versus-damaged line tallies, and both
join-readiness questions — is there a ledger, are the worktrees still alive.

ALWAYS EXITS 0, on every input, including a --repo that is not a git
repository or does not exist. Everything is reported as a finding rather than
raised as an error: a doctor that exits non-zero gets wrapped in `|| true` by
every script that calls it, and then its exit code means nothing. The
machine-readable verdict is `.join_readiness.ready` under --json.")]
    Doctor(DoctorArgs),
    /// Print a shell completion script for recount's commands and flags
    #[command(long_about = "\
Print a shell completion script for recount's commands and flags, generated
from the same clap command tree the parser uses — so it cannot offer a
subcommand or a flag the binary does not have.

The script goes to stdout alone, so it can be redirected straight into a
completions directory:

  recount completion zsh > \"${ZDOTDIR:-$HOME}\"/.zsh/completions/_recount

This is the one command that does NOT follow the spec's `all --json; human
digest second` rule. Its output is a shell program, and a digest printed
beside it would be sourced as code, so --json is accepted and ignored here
rather than refused.")]
    Completion(CompletionArgs),
}

/// The window edges shared by `sessions` and `grep`.
///
/// Each takes RFC 3339, or a duration meaning "that long ago" (`--since 2h`).
#[derive(Debug, Args)]
pub struct WindowArgs {
    /// Ignore turns before this instant (RFC 3339, or a duration ago: 2h, 1d)
    #[arg(long, value_name = "WHEN", value_parser = parse_when)]
    pub since: Option<DateTime<Utc>>,

    /// Ignore turns after this instant (RFC 3339, or a duration ago: 2h, 1d)
    #[arg(long, value_name = "WHEN", value_parser = parse_when)]
    pub until: Option<DateTime<Utc>>,
}

impl WindowArgs {
    pub fn window(&self) -> crate::model::TimeWindow {
        crate::model::TimeWindow {
            since: self.since,
            until: self.until,
        }
    }
}

#[derive(Debug, Args)]
pub struct SessionsArgs {
    /// Repository whose encoded session directories to resolve (repo + worktrees)
    #[arg(long, value_name = "PATH")]
    pub repo: PathBuf,

    #[command(flatten)]
    pub window: WindowArgs,
}

#[derive(Debug, Args)]
pub struct AtArgs {
    /// Repository whose encoded session directories to resolve (repo + worktrees)
    #[arg(long, value_name = "PATH")]
    pub repo: PathBuf,

    /// Fleet identity to resolve, as it appears in the pact ledger
    #[arg(long, value_name = "NAME")]
    pub agent: String,

    /// The moment to resolve, RFC 3339
    #[arg(long, value_name = "RFC3339", value_parser = parse_ts)]
    pub ts: DateTime<Utc>,

    /// Half-width of the match window either side of --ts
    #[arg(long, value_name = "DURATION", default_value = "5m", value_parser = parse_duration)]
    pub window: Duration,
}

#[derive(Debug, Args)]
pub struct GrepArgs {
    /// Case-insensitive substring to look for. Not a regex, by design.
    #[arg(value_name = "NEEDLE")]
    pub needle: String,

    /// Repository whose encoded session directories to resolve (repo + worktrees)
    #[arg(long, value_name = "PATH")]
    pub repo: PathBuf,

    /// Only sessions attributed to this fleet identity
    #[arg(long, value_name = "NAME")]
    pub agent: Option<String>,

    #[command(flatten)]
    pub window: WindowArgs,
}

#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// Repository whose .pact/events.jsonl to read, and whose sessions to search
    #[arg(long, value_name = "PATH")]
    pub repo: PathBuf,

    /// Which event to explain, 1-based, as `pact log` numbers them
    #[arg(long, value_name = "N")]
    pub event_line: usize,

    /// Half-width of the match window either side of the event's timestamp
    #[arg(long, value_name = "DURATION", default_value = "5m", value_parser = parse_duration)]
    pub window: Duration,
}

#[derive(Debug, Args)]
pub struct TestifyArgs {
    /// Repository whose sessions — and whose .pact/events.jsonl — the findings are joined against
    #[arg(long, value_name = "PATH")]
    pub repo: PathBuf,

    /// Read the audit document from this file instead of stdin
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,

    /// Half-width of the match window either side of each finding's moment
    #[arg(long, value_name = "DURATION", default_value = "5m", value_parser = parse_duration)]
    pub window: Duration,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Repository to check join-readiness against. The spec writes `recount
    /// doctor` bare, so this defaults to the current directory rather than
    /// being required.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub repo: PathBuf,
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    /// Shell to generate for
    #[arg(value_name = "SHELL")]
    pub shell: Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_surface_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// The spec's five commands and their full flag surface. If a bead needs a
    /// flag that is not asserted here, the spec changes before the code does.
    #[test]
    fn every_documented_flag_parses() {
        let c = Cli::try_parse_from([
            "recount",
            "sessions",
            "--repo",
            "/r",
            "--since",
            "2026-08-19T07:00:00Z",
            "--until",
            "2026-08-19T08:00:00Z",
            "--json",
        ])
        .unwrap();
        assert!(c.json);
        assert!(matches!(c.command, Command::Sessions(_)));

        let c = Cli::try_parse_from([
            "recount",
            "at",
            "--repo",
            "/r",
            "--agent",
            "skeleton",
            "--ts",
            "2026-08-19T07:03:12+00:00",
            "--window",
            "5m",
        ])
        .unwrap();
        match c.command {
            Command::At(a) => {
                assert_eq!(a.agent, "skeleton");
                assert_eq!(a.window, Duration::minutes(5));
            }
            other => panic!("wrong command: {other:?}"),
        }

        let c = Cli::try_parse_from([
            "recount", "grep", "arkanoid", "--repo", "/r", "--agent", "skeleton", "--since", "2h",
        ])
        .unwrap();
        match c.command {
            Command::Grep(g) => {
                assert_eq!(g.needle, "arkanoid");
                assert_eq!(g.agent.as_deref(), Some("skeleton"));
                assert!(g.window.since.is_some());
            }
            other => panic!("wrong command: {other:?}"),
        }

        let c = Cli::try_parse_from(["recount", "explain", "--repo", "/r", "--event-line", "7"])
            .unwrap();
        match c.command {
            Command::Explain(e) => assert_eq!(e.event_line, 7),
            other => panic!("wrong command: {other:?}"),
        }

        let c = Cli::try_parse_from([
            "recount",
            "testify",
            "--repo",
            "/r",
            "--file",
            "/a/audit.json",
            "--window",
            "10m",
        ])
        .unwrap();
        match c.command {
            Command::Testify(t) => {
                assert_eq!(t.file, Some(PathBuf::from("/a/audit.json")));
                assert_eq!(t.window, Duration::minutes(10));
            }
            other => panic!("wrong command: {other:?}"),
        }

        // ... and `--file` is optional, because stdin is the pipeline case.
        let c = Cli::try_parse_from(["recount", "testify", "--repo", "/r"]).unwrap();
        match c.command {
            Command::Testify(t) => {
                assert_eq!(t.file, None);
                assert_eq!(t.window, Duration::minutes(5));
            }
            other => panic!("wrong command: {other:?}"),
        }

        // `recount doctor` takes nothing, per the spec.
        let c = Cli::try_parse_from(["recount", "doctor"]).unwrap();
        match c.command {
            Command::Doctor(d) => assert_eq!(d.repo, PathBuf::from(".")),
            other => panic!("wrong command: {other:?}"),
        }
    }

    #[test]
    fn window_defaults_to_five_minutes() {
        let c = Cli::try_parse_from([
            "recount",
            "at",
            "--repo",
            "/r",
            "--agent",
            "a",
            "--ts",
            "2026-08-19T07:00:00Z",
        ])
        .unwrap();
        match c.command {
            Command::At(a) => assert_eq!(a.window, Duration::minutes(5)),
            other => panic!("wrong command: {other:?}"),
        }
    }

    #[test]
    fn a_bad_ts_is_rejected_not_guessed() {
        assert!(Cli::try_parse_from([
            "recount",
            "at",
            "--repo",
            "/r",
            "--agent",
            "a",
            "--ts",
            "yesterday"
        ])
        .is_err());
    }
}
