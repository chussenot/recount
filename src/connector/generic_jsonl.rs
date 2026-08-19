//! Connector 2 of 2: any directory of `*.jsonl`, best-effort.
//!
//! The spec's whole description is one line: "any dir of *.jsonl with
//! {ts|timestamp, role, content|text}; documented best-effort." Both spellings
//! of each pair are accepted because both are in the wild and neither is more
//! correct.
//!
//! What this deliberately does not do, so nobody files a bug about it: it does
//! not sniff which tool wrote the file, does not walk subdirectories, does not
//! reconstruct a `cwd` (there is nowhere in the format that states one, so
//! [`SessionMeta::cwd`] stays `None` and the topological join will correctly
//! refuse these sessions with "no cwd match"), and does not repair records. A
//! line missing a timestamp or a role is a parse failure, counted and skipped.

use std::path::{Path, PathBuf};

use crate::connector::Connector;
use crate::model::{SessionMeta, TimeWindow, Turn};

pub struct GenericJsonl;

impl Connector for GenericJsonl {
    fn name(&self) -> &'static str {
        "generic_jsonl"
    }

    fn roots(&self) -> Vec<PathBuf> {
        todo!("recount-hwk.26: this connector has no fixed root; it reads the dirs it is given")
    }

    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta> {
        todo!("recount-hwk.26: candidates -> prune -> scan {dirs:?} {window:?}")
    }

    fn read(&self, path: &Path) -> Vec<Turn> {
        todo!("recount-hwk.26: accept ts|timestamp, role, content|text in {path:?}")
    }
}
