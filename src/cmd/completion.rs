//! `recount completion <shell>` — the completion script, generated, never written.
//!
//! Generated from the same clap command tree [`crate::cli::Cli`] gives the
//! parser, so the script cannot offer a subcommand or a flag the binary does
//! not have. That is the entire reason this is a subcommand rather than five
//! checked-in files: a checked-in completion is a hand-written copy of the CLI,
//! and a hand-written copy of anything drifts from it the first time the
//! original changes.
//!
//! The script goes to stdout alone, so it can be redirected into a completions
//! directory. This is the one command that does NOT follow the spec's "all
//! --json; human digest second" rule, and deliberately: its output is a shell
//! program for a shell to read, and a digest printed beside it would be
//! sourced as code. `--json` is accepted and ignored, because the flag is
//! global and refusing it here would be a usage error for something harmless.

use std::io;

use clap::CommandFactory;
use clap_complete::generate;

use crate::cli::{Cli, CompletionArgs};

pub fn run(args: &CompletionArgs, _json: bool) -> anyhow::Result<u8> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, name, &mut io::stdout());
    Ok(super::EXIT_OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    /// Every shell generates something, and that something mentions every
    /// subcommand the binary exposes. The subcommand list is read out of the
    /// command tree rather than written here: a hardcoded list would be the
    /// same drift this module exists to prevent, one level down.
    #[test]
    fn every_shell_generates_a_script_naming_every_subcommand() {
        let subcommands: Vec<String> = Cli::command()
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect();
        assert!(
            subcommands.len() >= 6,
            "expected the spec's five commands plus completion, got {subcommands:?}"
        );

        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::Elvish,
            Shell::PowerShell,
        ] {
            let mut out = Vec::new();
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "recount", &mut out);
            let script = String::from_utf8(out).expect("completion scripts are utf-8");
            assert!(!script.is_empty(), "{shell} generated nothing");
            for sub in &subcommands {
                assert!(
                    script.contains(sub.as_str()),
                    "{shell} completion never mentions the `{sub}` subcommand"
                );
            }
        }
    }
}
