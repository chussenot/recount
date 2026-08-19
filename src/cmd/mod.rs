//! One module per command, one `run` per module.
//!
//! Every `run` has the same shape — `fn run(args: &XArgs, json: bool) ->
//! anyhow::Result<u8>` — returning the process exit code rather than calling
//! `exit`, so `explain` can literally call `at::run` and inherit its verdict
//! instead of reimplementing the join.
//!
//! The `json` flag is threaded rather than read from a global: the spec's "all
//! --json; human digest second" means both outputs come out of the same code
//! path, and a command that has to ask a static what mode it is in eventually
//! forgets to.

pub mod at;
pub mod doctor;
pub mod explain;
pub mod grep;
pub mod sessions;

/// Answered.
pub const EXIT_OK: u8 = 0;
/// No confident match — `at` and `explain` only — or a hard error.
pub const EXIT_NO_MATCH: u8 = 1;
/// The command parsed but the work behind it has not landed yet.
pub const EXIT_UNIMPLEMENTED: u8 = 2;

/// The seam's placeholder: the flags parsed, and the command says so plainly
/// instead of panicking. Each `run` returns this until its own bead lands.
pub fn unimplemented(command: &str) -> anyhow::Result<u8> {
    eprintln!("recount {command}: unimplemented — flags parsed, no work done yet");
    Ok(EXIT_UNIMPLEMENTED)
}
