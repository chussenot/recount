//! mtime windowing: the files a query provably need not open.
//!
//! recount owns no index — "session files are the only truth" — so scans are
//! made cheap structurally instead. This module is half of that: "sessions are
//! append-only, so mtime prunes files outside the query window unopened".
//!
//! # The argument is one-sided, and that matters
//!
//! An append-only file's mtime is the time of its *last* write, so it bounds
//! the last turn and says nothing about the first. Therefore:
//!
//! * a file whose mtime is **before** `window.since` cannot contain a turn
//!   inside the window — every turn it holds is older than its mtime. It is
//!   skipped, unopened. The lower bound prunes.
//! * a file whose mtime is **after** `window.until` may still contain turns
//!   from before `until` — a session that started this morning and was
//!   appended to a minute ago. It must be opened. The upper bound does not
//!   prune.
//!
//! Getting this backwards would silently lose evidence, which is why it is
//! written down rather than left to the reader of the `if`.
//!
//! An unbounded window prunes nothing. A file whose mtime cannot be read is
//! opened, not skipped: uncertainty resolves toward looking.

use std::path::PathBuf;

use crate::model::TimeWindow;

/// Candidates split by the mtime test. Both halves are kept: `skipped` is not
/// waste, it is what `doctor` and the bench report to show the pruning worked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Partition {
    /// Files that must be read.
    pub opened: Vec<PathBuf>,
    /// Files mtime ruled out. Never opened.
    pub skipped: Vec<PathBuf>,
}

/// Partition `candidates` by whether `window` can rule them out on mtime alone.
///
/// Order within each half follows `candidates`.
pub fn partition(candidates: &[PathBuf], window: &TimeWindow) -> Partition {
    todo!(
        "recount-hwk.32: mtime lower-bound prune of {} candidates against {window:?}",
        candidates.len()
    )
}
