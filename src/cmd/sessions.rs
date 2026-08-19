//! `recount sessions --repo <p> [--since --until]`
//!
//! Spec: "sessions whose cwd resolves into the repo or its worktrees,
//! fleet-identity guesses attached."
//!
//! The shape of the work, so the bead that fills this in does not re-derive
//! it: `worktree::session_dirs(repo)` resolves the encoded directories before
//! any file opens; `connector::candidates(&dirs)` lists `*.jsonl` without
//! opening them; `prune::partition` drops the ones mtime rules out of the
//! window; only what survives is read. A repo with no sessions is an empty
//! list and exit 0, not an error.

use crate::cli::SessionsArgs;

pub fn run(args: &SessionsArgs, json: bool) -> anyhow::Result<u8> {
    let _ = (args, json);
    crate::cmd::unimplemented("sessions")
}
