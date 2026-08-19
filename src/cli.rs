//! The complete command-line surface, in one file, once.
//!
//! This is a coordination contract as much as a parser: no later bead adds a
//! subcommand or a flag, so every command and every flag the spec's "Commands
//! (all --json; human digest second)" section names exists here now, parsing,
//! before the code behind it does. A bead that finds it needs a flag that is
//! not here changes the spec first.
//!
//! `--json` is global rather than per-command because the spec applies it to
//! all five ("all --json"), and a global flag can be written either side of
//! the subcommand, which is what people actually type.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use clap::{Args, Parser, Subcommand};

use crate::model::{parse_duration, parse_ts, parse_when};

#[derive(Debug, Parser)]
#[command(
    name = "recount",
    version,
    about = "Session testimony for pact audits: join a coordination ledger to coding-agent transcripts",
    long_about = None,
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
    /// Sessions whose cwd resolves into the repo or its worktrees, fleet-identity guesses attached
    Sessions(SessionsArgs),
    /// Resolve the session driving an identity at a moment; print the transcript window
    At(AtArgs),
    /// Case-insensitive substring hits with their coordinates
    Grep(GrepArgs),
    /// Read one event from .pact/events.jsonl and run `at` on it
    Explain(ExplainArgs),
    /// Connector roots, session counts, parse-failure tallies, join-readiness
    Doctor(DoctorArgs),
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
pub struct DoctorArgs {
    /// Repository to check join-readiness against. The spec writes `recount
    /// doctor` bare, so this defaults to the current directory rather than
    /// being required.
    #[arg(long, value_name = "PATH", default_value = ".")]
    pub repo: PathBuf,
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
