//! Repo path -> the set of encoded session directories, and back again.
//!
//! This is the cheap half of the spec's structural argument: "--repo resolves
//! to the project's encoded session directories (repo + worktrees) before any
//! file opens". A fleet works in worktrees, so a repo's transcripts are spread
//! across one encoded directory per worktree, and finding them is a `git
//! worktree list` plus a path encoding — no scanning, no reading.
//!
//! Both directions are needed and both live here:
//!
//! * forward, [`session_dirs`]: `--repo` -> the directories to look in;
//! * reverse, [`for_cwd`]: a session's `cwd` -> which worktree it was, which
//!   is the middle link of the join's chain `invoked_from -> worktree path ->
//!   session cwd`.
//!
//! # A pruned worktree is reported, not dropped
//!
//! `git worktree list` can name a directory that no longer exists on disk
//! (someone `rm -rf`ed it and never ran `git worktree prune`). Its transcripts
//! usually still exist under `~/.claude/projects`. Dropping the entry silently
//! would turn "the worktree is gone" into "no sessions found", which is
//! exactly the misdiagnosis this tool exists to prevent — so [`Worktree::alive`]
//! records it and `at` refuses with a reason that names it.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// One checkout of a repo: the main worktree or a linked one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    /// Absolute path git reports for this checkout.
    pub path: PathBuf,
    /// The name pact's `invoked_from` uses — the checkout's directory name,
    /// or `"main"`-ish for the primary worktree.
    pub name: String,
    /// Does `path` still exist on disk? See the module doc.
    pub alive: bool,
    /// The `~/.claude/projects/<encoded>` directory for `path`, whether or not
    /// it exists.
    pub session_dir: PathBuf,
}

/// Every checkout of `repo`, main first, including ones whose directory is
/// gone.
///
/// A `repo` that is not a git repository is not an error: it is treated as a
/// single directory with no linked worktrees, so `sessions --repo /some/dir`
/// still works.
pub fn worktrees(repo: &Path) -> Result<Vec<Worktree>> {
    todo!("recount-hwk.30: git worktree list --porcelain for {repo:?}, plus the main checkout")
}

/// The session directories to search for `repo` — one per worktree, existing
/// or not. Opens no session file.
pub fn session_dirs(repo: &Path) -> Result<Vec<PathBuf>> {
    todo!("recount-hwk.30: worktrees(repo).map(session_dir) for {repo:?}")
}

/// A filesystem path -> the directory name Claude Code encodes it as under
/// `~/.claude/projects`. The encoding is the one observed in
/// `docs/format-notes.md`, not a guess.
pub fn encode(path: &Path) -> String {
    todo!("recount-hwk.30: encode {path:?} per docs/format-notes.md")
}

/// Reverse direction: which of `worktrees` does this session `cwd` belong to?
///
/// A cwd inside a worktree's subdirectory still belongs to that worktree; the
/// longest matching prefix wins, so a worktree nested under another resolves
/// to the inner one.
pub fn for_cwd<'a>(worktrees: &'a [Worktree], cwd: &Path) -> Option<&'a Worktree> {
    todo!(
        "recount-hwk.30: longest-prefix match of {cwd:?} against {} worktrees",
        worktrees.len()
    )
}
