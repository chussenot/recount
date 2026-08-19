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

use std::fs;
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
    vec![
        Box::new(claude_code::ClaudeCode),
        Box::new(generic_jsonl::GenericJsonl),
    ]
}

/// Every `*.jsonl` anywhere under each of `dirs`, sorted, deduplicated, and
/// opening none of them.
///
/// Shared by both connectors and by `prune`'s callers: this is the "before any
/// file opens" boundary, and it is a free function rather than a trait method
/// so it cannot accidentally differ between the two implementations.
///
/// # Why this recurses
///
/// `docs/format-notes.md` §0 measured two shapes in the same tree, and the
/// nested one is the majority: 238 top-level `<encoded>/<session-uuid>.jsonl`
/// against 604 `<encoded>/<session-uuid>/subagents/agent-<id>.jsonl` (plus 29
/// `…/subagents/workflows/wf_<id>/journal.jsonl`). A single `read_dir` level
/// would silently miss three quarters of the corpus — and the subagent files
/// are precisely the fleet testimony this tool exists to find. Filtering the
/// shapes back apart is the connectors' job; this function's job is to miss
/// nothing. (`generic_jsonl`'s doc says it does not walk subdirectories: over
/// the flat directory of `*.jsonl` that connector describes, the recursion is
/// a no-op, so the two statements only differ on a tree it was never pointed
/// at.)
///
/// Only directory entries are consulted — no file is opened and no content is
/// read — because the mtime prune (`crate::prune`) runs on this list and the
/// spec's cheapness argument depends on the pruned files never being touched.
///
/// A `dirs` entry that does not exist, or that cannot be read, contributes
/// nothing and is not an error: a machine with no `~/.claude` is a normal
/// machine.
pub fn candidates(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        collect_jsonl(dir, &mut out);
    }
    // Sorted for determinism (readdir order is filesystem order, which differs
    // between machines and between runs); deduplicated because the path
    // encoding is not injective (format-notes §1.3), so two worktrees can
    // legitimately hand us the same session directory twice.
    out.sort();
    out.dedup();
    out
}

/// Depth-first over directory entries.
///
/// Symlinks are never descended into, so the walk cannot leave the root or
/// loop. A symlink *to* a `.jsonl` is still a candidate: following that one
/// reads a file, it does not walk a tree.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `DirEntry::file_type` does not follow symlinks — that is what makes
        // the rule above hold, and on Linux it is usually the `d_type` readdir
        // already returned rather than a second syscall.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    /// Create an empty file at `rel` under `dir`, parents included.
    fn touch(dir: &TempDir, rel: &str) -> PathBuf {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn registry_is_exactly_the_two() {
        let names: Vec<&str> = registry().iter().map(|c| c.name()).collect();
        assert_eq!(names, ["claude_code", "generic_jsonl"]);
    }

    /// Both layouts from `docs/format-notes.md`, plus the things that are not
    /// sessions, plus a directory that does not exist.
    #[test]
    fn candidates_finds_both_layouts_and_nothing_else() {
        let d = tempfile::tempdir().unwrap();
        let top = touch(&d, "proj/0000-session.jsonl");
        let nested = touch(&d, "proj/1111-session/subagents/agent-ab.jsonl");
        let journal = touch(
            &d,
            "proj/1111-session/subagents/workflows/wf_1/journal.jsonl",
        );
        touch(&d, "proj/1111-session/subagents/agent-ab.meta.json");
        touch(&d, "proj/notes.txt");
        touch(&d, "proj/jsonl");
        fs::create_dir_all(d.path().join("empty")).unwrap();

        let dirs = vec![
            d.path().join("proj"),
            d.path().join("empty"),
            d.path().join("does-not-exist"),
            // Duplicated on purpose: the encoding is not injective.
            d.path().join("proj"),
        ];
        assert_eq!(candidates(&dirs), vec![top, nested, journal]);
    }

    #[cfg(unix)]
    #[test]
    fn candidates_does_not_follow_symlinked_directories_out_of_the_root() {
        let d = tempfile::tempdir().unwrap();
        let inside = touch(&d, "root/real.jsonl");
        let outside = touch(&d, "elsewhere/escaped.jsonl");
        let root = d.path().join("root");
        std::os::unix::fs::symlink(d.path().join("elsewhere"), root.join("link")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("linked.jsonl")).unwrap();

        // `escaped.jsonl` is absent: the symlinked directory was not entered.
        assert_eq!(
            candidates(std::slice::from_ref(&root)),
            vec![root.join("linked.jsonl"), inside]
        );
    }
}
