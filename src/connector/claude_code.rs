//! Connector 1 of 2: Claude Code sessions under
//! `~/.claude/projects/<encoded-path>/*.jsonl`.
//!
//! The layout is reverse-engineered from this machine, not documented by
//! anyone — see `docs/format-notes.md` for the observed schema, the date it
//! was observed, and its "expect drift" warning. Every field this module
//! reaches for cites a section of those notes; if the code and the notes
//! disagree, the notes change first and deliberately.
//!
//! Tolerance, per the spec: unknown fields ignored, truncated tails counted
//! not fatal, one bad line never fails a file. A `.jsonl` written by a process
//! that is still running will have a half-line at the end; that is the normal
//! case, not corruption.

use std::path::{Path, PathBuf};

use crate::connector::Connector;
use crate::model::{SessionMeta, TimeWindow, Turn};

/// The root of the encoded per-project session directories.
pub fn projects_root() -> PathBuf {
    todo!("recount-hwk.29: $HOME/.claude/projects, per docs/format-notes.md")
}

pub struct ClaudeCode;

impl Connector for ClaudeCode {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    fn roots(&self) -> Vec<PathBuf> {
        todo!("recount-hwk.29: vec![projects_root()]")
    }

    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta> {
        todo!("recount-hwk.29: candidates -> prune -> scan, per format-notes: {dirs:?} {window:?}")
    }

    fn read(&self, path: &Path) -> Vec<Turn> {
        todo!("recount-hwk.29: tolerant per-line parse of {path:?}")
    }
}
