//! `recount at --repo <p> --agent <name> --ts <rfc3339> [--window 5m]`
//!
//! Spec: "resolve the session driving that identity at that moment; print the
//! transcript window. Exit 0 found / 1 no-confident-match with the reason (no
//! cwd match vs no overlap)."
//!
//! Both exits are answers. `EXIT_NO_MATCH` is only ever returned alongside a
//! printed `join::Refusal`, and `EXIT_OK` is only ever returned alongside the
//! match's confidence AND evidence — there is no path here that prints a bare
//! answer, in either output mode.

use crate::cli::AtArgs;

pub fn run(args: &AtArgs, json: bool) -> anyhow::Result<u8> {
    let _ = (args, json);
    crate::cmd::unimplemented("at")
}
