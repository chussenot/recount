//! `recount grep <needle> --repo <p> [--agent --since --until]`
//!
//! Spec: "(session, line, ts, role, snippet) hits."
//!
//! Matching is case-insensitive substring and nothing else. There is no
//! `--regex` flag because the README's refusals are the feature: "no fuzzy
//! search (case-insensitive substring only)". `Turn::line` is the 1-based
//! coordinate a reader can jump to.

use crate::cli::GrepArgs;

pub fn run(args: &GrepArgs, json: bool) -> anyhow::Result<u8> {
    let _ = (args, json);
    crate::cmd::unimplemented("grep")
}
