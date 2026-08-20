//! A throwaway `$HOME`, a throwaway git repo, and a way to run the real
//! binary against both.
//!
//! Every integration test in this crate needs the same three things, and one
//! of them is a trap:
//!
//! # `HOME` is set on the child, never on this process
//!
//! `claude_code::projects_root()` reads `$HOME` **live**, on every call. The
//! test harness runs test functions on many threads in one process, so a
//! `std::env::set_var("HOME", …)` here is not a scoped override — it is a
//! process-wide mutation that any other test can observe halfway through its
//! own run, and the resulting flake is one of the worst kinds to chase.
//!
//! So nothing here mutates this process's environment. [`Fixture::run`] spawns
//! `$CARGO_BIN_EXE_recount` with `HOME` set on the child. That is hermetic by
//! construction, and it happens to be the better test anyway: it exercises the
//! real CLI surface, including the exit code, which is half of what the spec's
//! `at` obligation is about.
//!
//! # Fixtures carry `{{REPO}}`, never a literal path
//!
//! A session's `cwd` has to be an absolute path inside a repo that does not
//! exist until the test runs. The committed fixtures under `tests/fixtures/`
//! therefore write `{{REPO}}`, which [`Fixture::install`] replaces with the
//! temporary repo's real path on the way in. A reader of the fixture sees
//! `"cwd":"{{REPO}}/wt/alice"` and knows exactly which worktree the line
//! claims, which is the point — the expected join has to be verifiable by eye.
//!
//! # The encoded directory name is computed, never typed
//!
//! [`encode`] mirrors `src/worktree.rs::encode` (one `-` per non-alphanumeric
//! character, lossy, no collapsing). It is duplicated rather than imported
//! because `recount` is a binary crate with no `lib` target, so an integration
//! test cannot `use` it. Duplicated, not hand-written: a literal
//! `-tmp-xyz-repo-wt-alice` in a test would silently stop matching the day the
//! encoding moved, and the test would report "no sessions" instead of failing.

#![allow(dead_code)] // Shared by several test binaries; each uses a subset.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// The two committed fixture directories under `tests/`.
///
/// Two rather than one because they are owned by different beads and a pact
/// lease is per path: `tests/fixtures/` is the 0.1.0 known-join corpus,
/// `tests/fixtures2/` is 0.2.0's keyed tree, model transcripts and canned
/// audit documents. Nothing distinguishes them to the loader.
const FIXTURES: &str = "fixtures";
const FIXTURES2: &str = "fixtures2";

/// A temporary `$HOME` beside a temporary git repo.
pub struct Fixture {
    /// Kept alive for the drop; the tree goes when the test does.
    _root: TempDir,
    home: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    /// An empty `$HOME` and a git repo with one commit, no worktrees, no
    /// sessions and no ledger.
    pub fn new() -> Fixture {
        let root = TempDir::new().expect("tempdir");
        // Canonical: `worktree::worktrees` canonicalizes `--repo` and git
        // reports real paths, so a symlinked TMPDIR would otherwise make the
        // encoded directory names disagree with the ones recount looks in.
        let base = root.path().canonicalize().expect("canonicalize tempdir");
        let home = base.join("home");
        let repo = base.join("repo");
        std::fs::create_dir_all(&home).expect("mkdir home");
        std::fs::create_dir_all(&repo).expect("mkdir repo");

        let fixture = Fixture {
            _root: root,
            home,
            repo,
        };
        fixture.git(&["init", "-q", "-b", "main"]);
        std::fs::write(fixture.repo.join("README.md"), "fixture repo\n").expect("write README");
        fixture.git(&["add", "README.md"]);
        fixture.git(&["commit", "-qm", "root commit"]);
        fixture
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// `git worktree add wt/<name>`, returning its path. The directory name is
    /// what pact's `invoked_from` and `worktree::worktrees` both call it.
    pub fn worktree(&self, name: &str) -> PathBuf {
        let path = self.repo.join("wt").join(name);
        self.git(&["worktree", "add", "-q", "--detach", &path.to_string_lossy()]);
        path
    }

    /// Delete a worktree's directory while leaving git's record of it — which
    /// is exactly the state a pruned or hand-deleted worktree is in: still
    /// listed by `git worktree list`, gone from the filesystem.
    pub fn prune_worktree(&self, name: &str) {
        std::fs::remove_dir_all(self.repo.join("wt").join(name)).expect("rm -rf worktree");
    }

    /// Install `tests/fixtures/<fixture>` as this repo's `.pact/events.jsonl`.
    pub fn events(&self, fixture: &str) -> PathBuf {
        let path = self.repo.join(".pact").join("events.jsonl");
        self.install(FIXTURES, fixture, &path);
        path
    }

    /// Install `tests/fixtures/<fixture>` as a session file in the encoded
    /// session directory `cwd` maps to — i.e. where Claude Code would have put
    /// a transcript of a session that ran in `cwd`.
    pub fn session(&self, cwd: &Path, name: &str, fixture: &str) -> PathBuf {
        let path = self
            .home
            .join(".claude")
            .join("projects")
            .join(encode(cwd))
            .join(name);
        self.install(FIXTURES, fixture, &path);
        path
    }

    /// The same placement as [`Fixture::session`], from a string rather than a
    /// committed fixture — for content a test generates.
    pub fn write_session(&self, cwd: &Path, name: &str, body: &str) -> PathBuf {
        let path = self
            .home
            .join(".claude")
            .join("projects")
            .join(encode(cwd))
            .join(name);
        write(&path, &self.subst(body));
        path
    }

    /// [`Fixture::events`], from [`FIXTURES2`].
    pub fn events2(&self, fixture: &str) -> PathBuf {
        let path = self.repo().join(".pact").join("events.jsonl");
        self.install(FIXTURES2, fixture, &path);
        path
    }

    /// [`Fixture::session`], from [`FIXTURES2`]. `name` may contain separators
    /// — `<session>/subagents/agent-<id>.jsonl` is the nested layout the keyed
    /// tier joins to, and the parent directories are created.
    pub fn session2(&self, cwd: &Path, name: &str, fixture: &str) -> PathBuf {
        let path = self
            .home
            .join(".claude")
            .join("projects")
            .join(encode(cwd))
            .join(name);
        self.install(FIXTURES2, fixture, &path);
        path
    }

    /// Install [`FIXTURES2`]`/<fixture>` at `<repo>/<name>` and return the
    /// path, for a document a command reads with `--file` rather than
    /// discovers.
    pub fn file2(&self, name: &str, fixture: &str) -> PathBuf {
        let path = self.repo.join(name);
        self.install(FIXTURES2, fixture, &path);
        path
    }

    /// Run the built binary against this fixture: `HOME` overridden, cwd set
    /// to the repo, stdout/stderr/exit code captured.
    pub fn run(&self, args: &[&str]) -> Run {
        let out = Command::new(env!("CARGO_BIN_EXE_recount"))
            .args(args)
            .env("HOME", &self.home)
            .current_dir(&self.repo)
            .output()
            .expect("running recount");
        Run {
            code: out.status.code().expect("recount exited by signal"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn install(&self, dir: &str, fixture: &str, dest: &Path) {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(dir)
            .join(fixture);
        let body = std::fs::read_to_string(&src)
            .unwrap_or_else(|e| panic!("reading fixture {}: {e}", src.display()));
        write(dest, &self.subst(&body));
    }

    fn subst(&self, body: &str) -> String {
        body.replace("{{REPO}}", &self.repo.to_string_lossy())
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            // Hermetic: no ~/.gitconfig, no committer identity from the host.
            .env("HOME", &self.home)
            .args(["-c", "user.name=fixture", "-c", "user.email=fixture@test"])
            .args(args)
            .output()
            .expect("running git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// One run of the binary.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// The `--json` document on stdout. Panics with both streams attached,
    /// because a test that mis-parses this needs to see what was printed.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e})\n--- stdout ---\n{}\n--- stderr ---\n{}",
                self.stdout, self.stderr
            )
        })
    }
}

/// A path -> the `~/.claude/projects` directory name Claude Code encodes it
/// as. Mirrors `src/worktree.rs::encode`; see the module doc for why it is
/// copied.
pub fn encode(path: &Path) -> String {
    let s = path.to_string_lossy();
    let trimmed = s.trim_end_matches('/');
    let trimmed = if trimmed.is_empty() { &s } else { trimmed };
    trimmed
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir -p");
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
}
