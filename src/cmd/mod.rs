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
pub mod completion;
pub mod doctor;
pub mod explain;
pub mod grep;
pub mod sessions;

/// Answered.
pub const EXIT_OK: u8 = 0;
/// No confident match — `at` and `explain` only — or a hard error.
pub const EXIT_NO_MATCH: u8 = 1;

// There was a third code here, `EXIT_UNIMPLEMENTED = 2`, and an `unimplemented`
// helper every `run` returned until its own bead landed. All of them have, so
// both are gone: a placeholder nothing can reach is a lie about what the binary
// can do. 2 still occurs — clap exits 2 on a usage error — which is why no
// command returns it deliberately.
