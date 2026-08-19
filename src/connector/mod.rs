//! The connector seam: exactly two implementations, and no plugin mechanism.
//!
//! The spec is explicit twice over — "Connectors (exactly two)" and "so a
//! third is one file — but do not add one". So [`registry`] returns a fixed
//! pair, not a discovered set: there is no registration macro, no inventory
//! crate, no `Box<dyn Connector>` factory keyed by string. Adding a third
//! connector means writing one file and adding one line to `registry`, which
//! is cheaper than any mechanism that would make it cheaper.
//!
//! # The tolerance contract, enforced by the return types
//!
//! "one bad line never fails a file" is not a promise in prose here, it is the
//! signature: [`Connector::discover`] and [`Connector::read`] return plain
//! `Vec`s, not `Result`s. A connector has nowhere to put a fatal error, so it
//! cannot invent one. Damage is reported as data instead —
//! [`SessionMeta::parse_failures`] counts the lines that did not parse, and
//! `doctor` shows the tally. A directory that does not exist yields an empty
//! `Vec`, because a machine with no `~/.claude` is a normal machine.
//!
//! # Why `discover` takes the directories
//!
//! The spec writes the trait as `discover() -> Vec<SessionMeta>`, but it also
//! requires that "--repo resolves to the project's encoded session directories
//! (repo + worktrees) before any file opens" and that mtime "prunes files
//! outside the query window unopened". Those two are the same requirement seen
//! from the caller's side: the scope has to reach the connector before the
//! connector opens anything. So the scope is an argument rather than a
//! rediscovery inside every implementation. Pass [`Connector::roots`] and an
//! unbounded [`TimeWindow`] for the whole-machine sweep `doctor` wants.

use std::path::{Path, PathBuf};

use crate::model::{SessionMeta, TimeWindow, Turn};

pub mod claude_code;
pub mod generic_jsonl;

pub trait Connector {
    /// Stable identifier, stored in [`SessionMeta::connector`] and printed by
    /// `doctor`. `&'static str` because there are exactly two and they are
    /// compiled in.
    fn name(&self) -> &'static str;

    /// Where this connector would look, whether or not those places exist.
    /// `doctor` reports these verbatim, including the ones that are absent —
    /// "the root you expected is not there" is the answer people need.
    fn roots(&self) -> Vec<PathBuf>;

    /// Metadata for every session under `dirs` that `window` cannot rule out.
    ///
    /// `dirs` are session directories (see [`crate::worktree::session_dirs`]),
    /// not repo paths. A `dirs` entry that does not exist is skipped
    /// silently. An unbounded `window` prunes nothing.
    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta>;

    /// Every turn in one session file, in file order, with 1-based line
    /// numbers. Unreadable file or unparseable lines: fewer turns, never an
    /// error.
    fn read(&self, path: &Path) -> Vec<Turn>;
}

/// The two connectors, in preference order. Exactly two, forever, on purpose.
pub fn registry() -> Vec<Box<dyn Connector>> {
    todo!("recount-hwk.24: return claude_code and generic_jsonl, and nothing else")
}

/// The `*.jsonl` files directly under each of `dirs`, sorted, opening none of
/// them.
///
/// Shared by both connectors and by `prune`'s callers: this is the "before any
/// file opens" boundary, and it is a free function rather than a trait method
/// so it cannot accidentally differ between the two implementations.
pub fn candidates(dirs: &[PathBuf]) -> Vec<PathBuf> {
    todo!("recount-hwk.24: list *.jsonl under each dir, sorted, no file opened: {dirs:?}")
}
