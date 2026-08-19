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
//! # NEVER DECODE A DIRECTORY NAME. Encode forward and look the name up.
//!
//! This is a prohibition, not a preference. The encoding ([`encode`], observed
//! rule in `docs/format-notes.md` §1) maps every non-`[A-Za-z0-9]` character to
//! a single `-`, so `/`, `.`, `_`, `+` and a literal `-` are all the same `-`
//! in the result. It is **lossy and non-injective**: `-a-b` is `/a/b` or `/a.b`
//! or `/a_b` or `/a-b` or `/a/.b`, with nothing in the name to tell them apart,
//! and two distinct real paths can own one directory (this machine has
//! `.../blv_claude_marketplace`, which shares its encoded name with a
//! `.../blv-claude-marketplace` that could be created tomorrow).
//!
//! A decoder is therefore not a missing feature, it is a wrong answer with a
//! plausible shape — the arkanoid-v1 failure mode the spec was written against.
//! Everything here goes one way: take paths you already know (the repo root and
//! the checkouts `git worktree list` names), encode them, look those names up.
//! Because the encoding is not injective, a directory that matches is
//! **evidence, not proof**; the `cwd` fields inside the files are the only
//! disambiguator, and they have their own problems (§4 of the notes).
//!
//! # A pruned worktree is reported, not dropped
//!
//! `git worktree list` can name a directory that no longer exists on disk
//! (someone `rm -rf`ed it and never ran `git worktree prune`). Its transcripts
//! usually still exist under `~/.claude/projects`. Dropping the entry silently
//! would turn "the worktree is gone" into "no sessions found", which is
//! exactly the misdiagnosis this tool exists to prevent — so [`Worktree::alive`]
//! records it and `at` refuses with a reason that names it.
//!
//! # What [`for_cwd`] can and cannot tell you
//!
//! Under a subagent fleet (`docs/format-notes.md` §4.3, and the topology this
//! very run uses) a transcript records the *parent session process's* cwd, not
//! the worktree the agent worked in: all 206 cwd-bearing lines of the three
//! wave-0 agents say the repo root, none says `wt/*`. So [`for_cwd`] will map
//! many sessions to the repo root and none to a worktree. That is the truth
//! about the data, and it is returned as such — no heuristic here guesses a
//! worktree the cwd does not support. The join detects the degeneracy and
//! leans on the transcript-mention signal instead.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::connector::claude_code::projects_root;

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
/// A `repo` that is not a git repository is a clean error naming the path, not
/// a panic and not an empty result: `--repo` is the one argument the whole
/// resolution hangs on, and "no sessions found" is the wrong way to say "that
/// is not a repo".
pub fn worktrees(repo: &Path) -> Result<Vec<Worktree>> {
    let repo = repo
        .canonicalize()
        .with_context(|| format!("--repo {}: cannot resolve path", repo.display()))?;
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("running `git worktree list` (is git on PATH?)")?;
    if !out.status.success() {
        bail!(
            "--repo {} is not a git repository: {}",
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Porcelain format: stanzas separated by blank lines, each opening with
    // `worktree <abs path>`. The other keys (HEAD, branch, bare, detached,
    // prunable, locked) are not needed — `prunable` in particular is git's
    // opinion, while `alive` is the filesystem's, and the filesystem is what
    // `at` has to refuse on.
    let listed = String::from_utf8_lossy(&out.stdout);
    let mut wts: Vec<Worktree> = listed
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(|p| describe(PathBuf::from(p)))
        .collect();
    // git lists the main checkout first, and pact's `invoked_from` calls it
    // "main" (observed in .pact/events.jsonl) rather than by its directory.
    if let Some(main) = wts.first_mut() {
        main.name = "main".to_string();
    }
    Ok(wts)
}

fn describe(path: PathBuf) -> Worktree {
    Worktree {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        alive: path.exists(),
        session_dir: projects_root().join(encode(&path)),
        path,
    }
}

/// The session directories to search for `repo` — one per worktree, existing
/// or not. Opens no session file.
///
/// Deduplicated: because [`encode`] is not injective, two checkouts can share
/// one directory (`wt/a-b` and `wt/a_b`), and scanning it twice would double
/// every session in it.
pub fn session_dirs(repo: &Path) -> Result<Vec<PathBuf>> {
    let mut seen = HashSet::new();
    Ok(worktrees(repo)?
        .into_iter()
        .map(|w| w.session_dir)
        .filter(|d| seen.insert(d.clone()))
        .collect())
}

/// A filesystem path -> the directory name Claude Code encodes it as under
/// `~/.claude/projects`. The encoding is the one observed in
/// `docs/format-notes.md` §1, not a guess: every character that is not
/// `[A-Za-z0-9]` becomes a single `-`, one dash per character, no collapsing,
/// case preserved.
///
/// Read the module doc before using this: the result cannot be turned back
/// into a path.
///
/// Two unproven corners, both flagged in the notes' §8: a trailing separator
/// (stripped here, since no observed name ends in a dash that a path did not
/// put there) and non-ASCII characters (one dash per `char`, untested — no path
/// on the machine the notes were taken from has any).
pub fn encode(path: &Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { &s } else { trimmed };
    trimmed
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Reverse direction: which of `worktrees` does this session `cwd` belong to?
///
/// A cwd inside a worktree's subdirectory still belongs to that worktree; the
/// longest matching prefix wins, so a worktree nested under another resolves
/// to the inner one.
///
/// That rule is what settles the real ambiguity: the repo root is a path prefix
/// of `<repo>/wt/agent`, so a cwd inside a worktree matches *both* the root and
/// the worktree. The worktree wins, always — the root's match is the trivially
/// true one and answering with it would erase the only distinction the join is
/// trying to make. Matching is purely lexical over `Path` components (so `/a/b`
/// never matches `/a/bc`) and touches no filesystem, which is why a worktree
/// whose directory is gone still claims its old sessions instead of vanishing.
pub fn for_cwd<'a>(worktrees: &'a [Worktree], cwd: &Path) -> Option<&'a Worktree> {
    worktrees
        .iter()
        .filter(|w| cwd.starts_with(&w.path))
        .max_by_key(|w| w.path.components().count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .expect("git on PATH");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repo with one commit, so `git worktree add` has a HEAD to branch from.
    fn repo() -> (TempDir, PathBuf) {
        let td = TempDir::new().unwrap();
        let repo = td.path().canonicalize().unwrap().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "root"]);
        (td, repo)
    }

    #[test]
    fn repo_with_worktrees_lists_main_first_then_each_checkout() {
        let (_td, repo) = repo();
        let wt = repo.join("wt").join("agent");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt/agent",
                wt.to_str().unwrap(),
            ],
        );

        let found = worktrees(&repo).unwrap();
        assert_eq!(found.len(), 2, "{found:#?}");
        assert_eq!(found[0].path, repo);
        assert_eq!(found[0].name, "main");
        assert_eq!(found[1].path, wt);
        assert_eq!(found[1].name, "agent");
        assert!(found.iter().all(|w| w.alive));
        assert_eq!(found[1].session_dir, projects_root().join(encode(&wt)));
        assert_eq!(
            session_dirs(&repo).unwrap(),
            vec![found[0].session_dir.clone(), found[1].session_dir.clone()]
        );
    }

    #[test]
    fn repo_with_no_worktrees_is_just_the_main_checkout() {
        let (_td, repo) = repo();
        let found = worktrees(&repo).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "main");
        assert!(found[0].alive);
        assert_eq!(session_dirs(&repo).unwrap().len(), 1);
    }

    #[test]
    fn pruned_worktree_is_reported_not_dropped() {
        let (_td, repo) = repo();
        let wt = repo.join("wt").join("gone");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt/gone",
                wt.to_str().unwrap(),
            ],
        );
        fs::remove_dir_all(&wt).unwrap();

        let found = worktrees(&repo).unwrap();
        assert_eq!(found.len(), 2, "the dead worktree must still be listed");
        assert!(found[0].alive);
        assert!(!found[1].alive, "`at` refuses on exactly this");
        // Its transcripts outlive its directory, so its session dir is still
        // resolved and still scanned.
        assert_eq!(found[1].session_dir, projects_root().join(encode(&wt)));
        assert_eq!(session_dirs(&repo).unwrap().len(), 2);
    }

    #[test]
    fn non_git_repo_is_a_clean_error() {
        let td = TempDir::new().unwrap();
        let err = worktrees(td.path()).unwrap_err().to_string();
        assert!(err.contains("not a git repository"), "{err}");
        assert!(err.contains(&td.path().canonicalize().unwrap().display().to_string()));

        let missing = td.path().join("nope");
        assert!(worktrees(&missing).is_err());
    }

    #[test]
    fn encode_is_one_dash_per_non_alphanumeric_character() {
        assert_eq!(encode(Path::new("/home/chussenot")), "-home-chussenot");
        // `.` is a character like any other: hence the doubled dash.
        assert_eq!(
            encode(Path::new("/home/chussenot/.dotfiles")),
            "-home-chussenot--dotfiles"
        );
        // Case and digits survive.
        assert_eq!(
            encode(Path::new("/tmp/tmp.5GCCO7j3pI")),
            "-tmp-tmp-5GCCO7j3pI"
        );
        // Unproven corners, pinned so a change is deliberate.
        assert_eq!(encode(Path::new("/a/b/")), "-a-b");
        assert_eq!(encode(Path::new("/")), "-");
    }

    #[test]
    fn encode_is_lossy_which_is_why_nothing_here_decodes() {
        // Five distinct paths, one directory name. This is the reason the
        // module doc forbids a decoder.
        for p in ["/a/b", "/a.b", "/a_b", "/a-b", "/a+b"] {
            assert_eq!(encode(Path::new(p)), "-a-b", "{p}");
        }
    }

    /// The rule was derived from this machine's real directory names; check it
    /// still reproduces them. Skips (does not fail) where the corpus is absent,
    /// so CI on a fresh box stays green.
    #[test]
    fn encode_reproduces_real_project_directory_names() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let root = PathBuf::from(home).join(".claude").join("projects");
        let Ok(entries) = fs::read_dir(&root) else {
            return;
        };
        let names: HashSet<String> = entries
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        if names.is_empty() {
            return;
        }

        // The encoding's alphabet is closed: no real name may contain a
        // character `encode` can never emit.
        for n in &names {
            assert!(
                n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "real directory name outside the encoding's alphabet: {n}"
            );
        }

        // Path -> name pairs derived in docs/format-notes.md §1.2 by reading
        // each directory's own `cwd` fields. Check every pair whose directory
        // is still on this machine; a machine with none just passes the
        // alphabet check above.
        let pairs = [
            "/home/chussenot",
            "/home/chussenot/.dotfiles",
            "/home/chussenot/Documents/blv_claude_marketplace",
            "/home/chussenot/Documents/codefactory.believe.tech/idp/run-plane/observability/terrasync",
            "/tmp/tmp.5GCCO7j3pI",
            "/home/chussenot/Documents/cost-reallocator/.claude/worktrees/fix-mise-python-tool",
            "/home/chussenot/Documents/datadog-templator/.claude/worktrees/chore+pdm-to-uv",
        ];
        // Only assert the pairs on the machine they were observed on — the
        // presence of the first one identifies it. Elsewhere this is a skip,
        // not a failure.
        if names.contains(&encode(Path::new(pairs[0]))) {
            for p in pairs {
                let encoded = encode(Path::new(p));
                assert!(
                    names.contains(&encoded),
                    "encode({p}) = {encoded}, which is not a directory under {}",
                    root.display()
                );
            }
        }

        // And the live repo: whatever checkout this test runs from, its
        // encoded name is how its own transcripts are filed.
        let here = std::env::current_dir().unwrap();
        assert!(encode(&here).starts_with('-'), "absolute paths lead with /");
    }

    fn wt(path: &str) -> Worktree {
        describe(PathBuf::from(path))
    }

    #[test]
    fn for_cwd_takes_the_longest_prefix_so_a_worktree_beats_the_repo_root() {
        let wts = vec![wt("/r"), wt("/r/wt/a"), wt("/r/wt/a/inner")];

        // The root is a prefix of every worktree; it must not win.
        assert_eq!(
            for_cwd(&wts, Path::new("/r/wt/a")).unwrap().path,
            wts[1].path
        );
        assert_eq!(
            for_cwd(&wts, Path::new("/r/wt/a/src/deep")).unwrap().path,
            wts[1].path
        );
        assert_eq!(
            for_cwd(&wts, Path::new("/r/wt/a/inner/x")).unwrap().path,
            wts[2].path
        );
        // The repo-root cwd every subagent transcript actually records.
        assert_eq!(for_cwd(&wts, Path::new("/r")).unwrap().path, wts[0].path);
        assert_eq!(
            for_cwd(&wts, Path::new("/r/src")).unwrap().path,
            wts[0].path
        );
        // Component-wise, not textual: /r/wtx is not inside /r/wt/a.
        assert_eq!(
            for_cwd(&wts, Path::new("/r/wtx")).unwrap().path,
            wts[0].path
        );
        assert!(for_cwd(&wts, Path::new("/elsewhere")).is_none());
        assert!(for_cwd(&[], Path::new("/r")).is_none());
    }
}
