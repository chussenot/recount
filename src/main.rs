//! recount — session testimony for pact audits.
//!
//! One job: join pact's coordination ledger (WHAT agents did) to coding-agent
//! session transcripts (WHY), so an audit finding can be answered with the
//! agent's own reasoning.
//!
//! This file is the module tree and nothing else. Every `mod` the plan needs
//! is declared here now; no later change adds one. Dispatch is a flat match on
//! the subcommand into `cmd::*`, each of which owns exactly one command.
//!
//! Exit codes, fixed here so every command agrees: `0` answered, `1` no
//! confident match (or a hard error). `2` comes from clap, for a usage error.
//! Only `at` (and `explain`, which calls it) can return `1` for a refusal; the
//! refusal always names its reason.
//!
//! The crate-wide `#![allow(dead_code)]` that stood here through the build is
//! gone, along with the `unimplemented` placeholder it was covering for: every
//! signature the plan declared now has a caller, which is what it was waiting
//! for.

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, Command};

mod cli;
mod cmd;
mod connector;
mod join;
mod model;
mod pact_events;
mod prune;
mod worktree;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Sessions(a) => cmd::sessions::run(a, cli.json),
        Command::At(a) => cmd::at::run(a, cli.json),
        Command::Grep(a) => cmd::grep::run(a, cli.json),
        Command::Explain(a) => cmd::explain::run(a, cli.json),
        Command::Doctor(a) => cmd::doctor::run(a, cli.json),
        Command::Completion(a) => cmd::completion::run(a, cli.json),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("recount: {err:#}");
            ExitCode::from(cmd::EXIT_NO_MATCH)
        }
    }
}
