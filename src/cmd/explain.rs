//! `recount explain --repo <p> --event-line <N>`
//!
//! Spec: "read event N from .pact/events.jsonl, run `at` on it — one command
//! from finding to testimony."
//!
//! "run `at` on it" is literal: this module reads the event, builds an
//! `AtArgs` from its `agent` and `at`, and calls `crate::cmd::at::run`. It
//! does not reimplement the join, and it inherits `at`'s exit codes and
//! refusal reasons for free. `N` is 1-based, matching `pact_events`; out of
//! range is a clean error naming the line count.

use crate::cli::ExplainArgs;

pub fn run(args: &ExplainArgs, json: bool) -> anyhow::Result<u8> {
    let _ = (args, json);
    crate::cmd::unimplemented("explain")
}
