//! `recount doctor`
//!
//! Spec: "connector roots, session counts, parse-failure tallies,
//! join-readiness (events present? worktree paths alive?)."
//!
//! Doctor is the command that must never fail: it is what you run when nothing
//! else works. In a bare directory with no `~/.claude`, no `.pact`, and no git
//! repo it reports four absences and exits 0 — the proof obligation is "calm
//! degradation, never a panic".

use crate::cli::DoctorArgs;

pub fn run(args: &DoctorArgs, json: bool) -> anyhow::Result<u8> {
    let _ = (args, json);
    crate::cmd::unimplemented("doctor")
}
