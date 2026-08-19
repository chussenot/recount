//! `recount grep <needle> --repo <p> [--agent --since --until]`
//!
//! Spec: "(session, line, ts, role, snippet) hits."
//!
//! Matching is case-insensitive substring and nothing else. There is no
//! `--regex` flag because the README's refusals are the feature: "no fuzzy
//! search (case-insensitive substring only)". `Turn::line` is the 1-based
//! coordinate a reader can jump to.
//!
//! Everything below is that one sentence taken seriously. No regex crate is in
//! the dependency budget and none is wanted: the refusal is what keeps `grep`
//! a scan over turns recount already parses, with no engine, no compilation
//! step and no pattern that can take exponential time on a transcript.
//!
//! # The five-tuple, and the two fields beside it
//!
//! Each hit carries exactly the spec's `(session, line, ts, role, snippet)`.
//! Two more ride along because they cost nothing and answer the first two
//! questions a reader asks; neither replaces a member of the tuple:
//!
//! * `occurrences` — how many times the needle occurs in that turn. **One hit
//!   per turn**, not one per occurrence: the coordinate of a hit is
//!   `(session, line)`, and a second occurrence on the same line has the same
//!   coordinate, so a second row would be a duplicate the reader cannot
//!   navigate to. The count is there so nothing is hidden — "42 occurrences"
//!   is the honest form of what the extra rows would have said. (Overlapping
//!   occurrences are not counted twice: the scan resumes at the end of the
//!   match, so `aa` occurs once in `aaa`.)
//! * `block_guess` — `thinking`, `tool_use Bash`, `tool_result`, … The
//!   `claude_code` connector tags block types into the turn text, and a hit
//!   inside a `[thinking]` block is the most audit-valuable hit there is: it
//!   is the agent's stated reasoning, not its speech. This is read back off
//!   the rendered text — the nearest preceding `[tag]` line — and the
//!   transcript is **not** re-parsed to get it. That makes it a guess and it
//!   is named like one: an untagged `text` block rendered after a `[thinking]`
//!   block carries no marker of its own, so a hit in it is attributed to the
//!   thinking above it. `null` means no tag preceded the hit.
//!
//! # Snippets are bounded on the bytes copied, not on what was found
//!
//! `docs/format-notes.md` §5 measured 31 real lines over 1 MB and one string
//! of **9,610,205 characters** (a prompt that re-quoted itself into 78.6%
//! backslashes). A snippet is at most [`SNIPPET_MAX_BYTES`] bytes taken out of
//! the turn — [`SNIPPET_LEAD_BYTES`] of run-up before the match and the rest
//! from the match onward — so a hit in that 9.6 MB string copies 240 bytes,
//! exactly like a hit in a one-line prompt. The bound is on the copy; finding
//! the match still reads the turn, because that is what searching is.
//!
//! The cut is moved back to a `char` boundary before any slice is taken.
//! Slicing a `&str` mid-codepoint panics, and the spec's integration
//! obligation is "calm degradation, **never a panic**" — a crash on a
//! multi-byte character near a byte bound would be exactly the pathology this
//! tool is meant to survive. The `…` markers are added around the copy and are
//! not counted against its budget.
//!
//! Control characters inside the copied bytes become spaces. A snippet is one
//! line of a search result; a raw newline or an escape sequence out of a
//! `tool_result` would break the digest and, in a terminal, more than that.
//! Every control character is one byte in the ranges recount meets, and the
//! replacement happens after the copy, so neither the bound nor the encoding
//! moves.
//!
//! # Case-insensitive means per-character lowercase, in place
//!
//! The obvious implementation — lowercase the haystack, search that — is wrong
//! twice over here: it allocates a copy of a turn this module has just
//! promised not to copy, and `str::to_lowercase` does not preserve byte
//! offsets (`İ` lowercases into two chars), so offsets found in the copy would
//! address the wrong bytes of the original. [`find_ci`] compares `char` by
//! `char` against the needle's chars instead, and therefore returns offsets
//! into the turn itself. Its one documented limit is the same one that
//! motivates it: case mappings that change character count (`İ`, `ẞ`) do not
//! fold. `é`/`É` and `σ`/`Σ` do.
//!
//! Two ASCII characters take a fast path — [`char::eq_ignore_ascii_case`],
//! which is the same answer for that pair and does not build an iterator. The
//! test is on *both* characters, not one: `K` (U+212A) lowercases to ASCII `k`
//! and must keep folding with it. That one branch was 1.75 s of a 2.8 s
//! unwindowed `grep` over 200 MB (recount-hwk.47) — the old code built two
//! `ToLowercase` iterators per haystack character, for every character of
//! every turn, to reject a first-character test that almost always fails.
//!
//! # `--agent` filters on a guess, and says so
//!
//! There is no record of which fleet agent ran a session — [`crate::cmd::sessions`]
//! infers one, and its output is named `fleet_identity_guess` for that reason.
//! `--agent X` therefore cannot mean "sessions that are X". It means
//! **"sessions some signal names X"**: either the literal `PACT_AGENT=X`
//! assignment appears in the file's bytes, or the session's cwd resolves to
//! the linked worktree `X`. Those are the same two signals `sessions` weighs,
//! taken as a union rather than through its arithmetic — the floor is one
//! signal, which is `sessions`' weakest priced guess (topology alone, 0.50),
//! never lower.
//!
//! That is deliberately wider than "our best guess is X". `grep` is a search,
//! not a verdict: a false include costs a line the reader discards, a false
//! exclude costs testimony they never learn exists. `at` is the command that
//! refuses. Concretely, a transcript naming two identities gets no guess at
//! all from `sessions`; if one of them is X, `grep --agent X` still searches it.
//!
//! **Sessions whose identity no signal names are skipped, and never silently.**
//! They are counted in `sessions_skipped_by_agent`, listed by path in
//! `sessions_skipped_unknown_identity`, and named in the digest. A `--agent`
//! search that quietly dropped the sessions it could not attribute would
//! report "no hits" for a repo full of them, which is the misdiagnosis this
//! tool exists to prevent.
//!
//! # Scope: everything under the repo's session directories
//!
//! `--repo` resolves to the encoded session directories before any file opens
//! ([`crate::worktree::session_dirs`]), and `--since` reaches
//! [`crate::prune::partition`] through [`connector::discover_all`], which is
//! where the spec's cheapness argument lives. What `grep` does **not** do is
//! `sessions`' second filter: a session filed under this repo's directory that
//! states a cwd in another repo is searched here, where `sessions` excludes
//! it. Same reasoning as `--agent` — the encoding is lossy and the exclusion
//! is a judgement call, and a search should not make that call silently. The
//! counts say how many sessions were searched.
//!
//! # No hits is an answer
//!
//! An empty list and exit 0, the same rule `sessions` applies to a repo with
//! no transcripts. A needle that is not there is a fact about the corpus, not
//! a failure of the command. The one input that is an error is an empty
//! needle, which is not a search.
//!
//! # Output: the house envelope, JSON on stdout, digest on stderr
//!
//! [`crate::cmd::sessions`]'s module doc states the rule; this is one of the
//! commands that follows it. Without `--json`, the digest on stdout alone.
//! With `--json`, the document on stdout alone and the same digest on stderr,
//! second. `schema` moves only when a field leaves or changes meaning.
//!
//! ```text
//! { "command": "grep", "schema": 1,
//!   "needle": "arkanoid",
//!   "repo": "/abs/path",
//!   "window": { "since": <rfc3339|null>, "until": <rfc3339|null> },
//!   "agent": <name|null>,
//!   "session_dirs": ["/home/u/.claude/projects/-abs-path", ...],
//!   "sessions_searched": 12,
//!   "sessions_skipped_by_agent": 3,
//!   "sessions_skipped_unknown_identity": ["…/a.jsonl"],   // a subset of the 3
//!   "turns_searched": 1204,
//!   "hits": [ { "session": "…/s.jsonl", "line": 128,
//!               "ts": "2026-08-19T07:03:12Z", "role": "assistant",
//!               "snippet": "…the arkanoid-v1 misdiagnosis…",
//!               "occurrences": 3, "block_guess": "thinking" } ] }
//! ```

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::cli::GrepArgs;
use crate::cmd::EXIT_OK;
use crate::connector::{self, Connector};
use crate::model::{Role, SessionMeta, TimeWindow, Turn};
use crate::worktree::{self, Worktree};

/// Bumped only when a field leaves or changes meaning. See the module doc.
const SCHEMA: u32 = 1;

/// The most bytes one snippet copies out of a turn. The `…` markers are added
/// around the copy and do not count against it.
const SNIPPET_MAX_BYTES: usize = 240;

/// How much of that budget is spent on the run-up before the match, so a hit
/// reads in context instead of starting flush against the needle.
const SNIPPET_LEAD_BYTES: usize = 60;

/// The literal assignment that tells a pact agent who it is. Same constant,
/// same reasoning, as `sessions`.
const PACT_AGENT: &str = "PACT_AGENT=";

pub fn run(args: &GrepArgs, json: bool) -> Result<u8> {
    if args.needle.is_empty() {
        bail!("empty needle: every line contains the empty string, so that is not a search");
    }
    // The spec's performance clause, before any file opens: `--repo` becomes
    // the encoded directories, and the ones that are not there are dropped.
    let worktrees = worktree::worktrees(&args.repo)?;
    let dirs: Vec<PathBuf> = worktree::session_dirs(&args.repo)?
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let repo = worktrees
        .first()
        .map_or_else(|| args.repo.clone(), |w| w.path.clone());

    let report = search(
        &connector::registry(),
        &Query {
            needle: &args.needle,
            agent: args.agent.as_deref(),
            window: args.window.window(),
        },
        &repo,
        &worktrees,
        &dirs,
    );
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(EXIT_OK)
}

/// What was asked for, as opposed to where to look for it.
pub struct Query<'a> {
    pub needle: &'a str,
    pub agent: Option<&'a str>,
    pub window: TimeWindow,
}

/// One turn that contains the needle. The first five fields are the spec's
/// tuple; the last two are documented in the module doc.
#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub session: PathBuf,
    /// 1-based line of the record inside its `.jsonl` — `sed -n <line>p` it.
    pub line: usize,
    pub ts: DateTime<Utc>,
    pub role: Role,
    pub snippet: String,
    /// Occurrences in this turn; the snippet shows the first.
    pub occurrences: usize,
    /// The `[tag]` the hit sits under, read back off the rendered text. A guess.
    pub block_guess: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub command: &'static str,
    pub schema: u32,
    pub needle: String,
    pub repo: PathBuf,
    pub window: TimeWindow,
    pub agent: Option<String>,
    pub session_dirs: Vec<PathBuf>,
    pub sessions_searched: usize,
    /// Discovered, but `--agent` named someone else or named no one.
    pub sessions_skipped_by_agent: usize,
    /// The subset of those that named *no one*: skipped because we could not
    /// guess, not because we guessed against them. Listed, never just counted.
    pub sessions_skipped_unknown_identity: Vec<PathBuf>,
    pub turns_searched: usize,
    pub hits: Vec<Hit>,
}

/// Discover, filter by identity, read, scan.
///
/// `dirs` and `worktrees` are arguments rather than rediscovered here so the
/// resolution happens once, in [`run`], before anything opens a file — and so
/// this is testable without a `$HOME`.
fn search(
    connectors: &[Box<dyn Connector>],
    query: &Query,
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
) -> Report {
    let mut report = Report {
        command: "grep",
        schema: SCHEMA,
        needle: query.needle.to_string(),
        repo: repo.to_path_buf(),
        window: query.window,
        agent: query.agent.map(str::to_string),
        session_dirs: dirs.to_vec(),
        sessions_searched: 0,
        sessions_skipped_by_agent: 0,
        sessions_skipped_unknown_identity: Vec::new(),
        turns_searched: 0,
        hits: Vec::new(),
    };

    // No directory to look in is a question with a known answer, so no
    // connector is asked it. `discover_all`, not the registry directly: both
    // connectors accept `*.jsonl` and running both would search every file
    // twice (see `connector`'s module doc).
    let discovered: Vec<SessionMeta> = if dirs.is_empty() {
        Vec::new()
    } else {
        connector::discover_all(connectors, dirs, &query.window)
    };

    let needle: Vec<char> = query.needle.chars().collect();
    for meta in &discovered {
        if let Some(agent) = query.agent {
            match names_agent(meta, worktrees, agent) {
                Identified::Named => {}
                Identified::Someone => {
                    report.sessions_skipped_by_agent += 1;
                    continue;
                }
                Identified::NoOne => {
                    report.sessions_skipped_by_agent += 1;
                    report
                        .sessions_skipped_unknown_identity
                        .push(meta.path.clone());
                    continue;
                }
            }
        }
        // A meta whose connector is not in this list cannot be read back; it
        // cannot happen through `discover_all`, which produced it from one of
        // them, and skipping is the only non-panicking answer if it ever does.
        let Some(connector) = connectors.iter().find(|c| c.name() == meta.connector) else {
            continue;
        };
        report.sessions_searched += 1;
        for turn in connector.read(&meta.path) {
            // mtime and the file's span got the file here; only the turn knows
            // whether the turn is in the window.
            if !query.window.contains(turn.ts) {
                continue;
            }
            report.turns_searched += 1;
            if let Some(hit) = hit_in(&turn, &needle, &meta.path) {
                report.hits.push(hit);
            }
        }
    }

    // Chronological: an audit reads across sessions in time order, and the
    // path/line tie-break keeps one file's hits together and jumpable.
    report.hits.sort_by(|a, b| {
        a.ts.cmp(&b.ts)
            .then_with(|| a.session.cmp(&b.session))
            .then_with(|| a.line.cmp(&b.line))
    });
    report
}

/// What `--agent` decided about one session. Read the module doc: `Named` is
/// "a signal names it", not "it is".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Identified {
    /// A signal names the identity asked for.
    Named,
    /// A signal names an identity, and it is not the one asked for.
    Someone,
    /// No signal names anyone. Skipped, and said out loud.
    NoOne,
}

fn names_agent(meta: &SessionMeta, worktrees: &[Worktree], agent: &str) -> Identified {
    let named = pact_agent_names(&meta.path);
    // The main checkout names no one: every agent of a pact run reports it
    // (docs/format-notes.md §4.3), so only a linked worktree is a signal.
    let main = worktrees.first().map(|w| w.path.as_path());
    let from_cwd = meta
        .cwd
        .as_deref()
        .and_then(|cwd| worktree::for_cwd(worktrees, cwd))
        .filter(|w| Some(w.path.as_path()) != main)
        .map(|w| w.name.as_str());

    if named.iter().any(|n| n == agent) || from_cwd == Some(agent) {
        Identified::Named
    } else if named.is_empty() && from_cwd.is_none() {
        Identified::NoOne
    } else {
        Identified::Someone
    }
}

/// Every distinct `<name>` in a `PACT_AGENT=<name>` assignment anywhere in the
/// file's bytes.
///
/// A substring scan, not a parse: the name reads the same in a JSON string as
/// in a raw line, and this must work for a connector whose reader has its own
/// opinions. An unreadable file names no one, which is not an error.
///
/// ponytail: this is `sessions::pact_agent_names`, which is private to that
/// module and cannot be called from here. The duplicate is ~10 lines and the
/// two beads own separate files; the fix, when a third caller appears, is to
/// lift it into `connector` or `model` — not to make one command depend on
/// another command's internals.
fn pact_agent_names(path: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = String::from_utf8_lossy(&bytes)
        .split(PACT_AGENT)
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The first occurrence in this turn, plus the count of all of them. `None`
/// when the needle is not there.
fn hit_in(turn: &Turn, needle: &[char], session: &Path) -> Option<Hit> {
    let (start, end) = find_ci(&turn.text, needle, 0)?;
    let mut occurrences = 1;
    let mut from = end;
    while let Some((_, next)) = find_ci(&turn.text, needle, from) {
        occurrences += 1;
        from = next;
    }
    Some(Hit {
        session: session.to_path_buf(),
        line: turn.line,
        ts: turn.ts,
        role: turn.role.clone(),
        snippet: snippet(&turn.text, start),
        occurrences,
        block_guess: block_tag(&turn.text, start).map(str::to_string),
    })
}

/// Case-insensitive substring search returning byte offsets **into `hay`**.
///
/// See the module doc for why this is not `to_lowercase().find()`. `from` must
/// be a `char` boundary; every offset this function returns is one.
///
/// Worst case is O(needle · haystack), which is what a substring search costs
/// without a regex engine or a preprocessing pass over a turn recount has
/// promised not to copy. The first-character test rejects almost every start.
fn find_ci(hay: &str, needle: &[char], from: usize) -> Option<(usize, usize)> {
    let first = *needle.first()?;
    for (offset, c) in hay[from..].char_indices() {
        if !eq_ci(c, first) {
            continue;
        }
        let start = from + offset;
        let mut end = start;
        let mut chars = hay[start..].chars();
        let matched = needle.iter().all(|&want| match chars.next() {
            Some(got) if eq_ci(got, want) => {
                end += got.len_utf8();
                true
            }
            _ => false,
        });
        if matched {
            return Some((start, end));
        }
    }
    None
}

/// Equal ignoring case. `char::to_lowercase` because `é`/`É` must fold and
/// `eq_ignore_ascii_case` would not; the mappings that change character count
/// (`İ`, `ẞ`) do not fold, which the module doc states.
///
/// The ASCII pair is answered without it, and only when **both** sides are
/// ASCII: `K` (U+212A) is not, and lowercases to ASCII `k`, so a one-sided test
/// would stop folding a pair that folds today. See the module doc for what the
/// iterator cost.
fn eq_ci(a: char, b: char) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(&b);
    }
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// At most [`SNIPPET_MAX_BYTES`] bytes of `text` around the match at `start`,
/// cut on `char` boundaries, control characters flattened to spaces, and `…`
/// wherever bytes were left out.
fn snippet(text: &str, start: usize) -> String {
    let lead = floor_boundary(text, start.saturating_sub(SNIPPET_LEAD_BYTES));
    // Floor, not ceil, so the budget is a ceiling on the bytes copied even
    // when the last character straddles it.
    let stop = floor_boundary(text, lead.saturating_add(SNIPPET_MAX_BYTES).min(text.len()));
    let mut out = String::with_capacity(SNIPPET_MAX_BYTES + 8);
    if lead > 0 {
        out.push('…');
    }
    out.extend(
        text[lead..stop]
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c }),
    );
    if stop < text.len() {
        out.push('…');
    }
    out
}

/// The greatest `char` boundary `<= i`. `i` may be past the end.
///
/// `str::floor_char_boundary` is the same function and is not stable on every
/// toolchain this has to build on; three lines is cheaper than an MSRV note.
fn floor_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The `claude_code` block tag the hit sits under: the nearest `[tag]` line at
/// or before the hit's own line. A guess — see the module doc.
///
/// Scans backwards and stops at the first tag, so the common case reads a few
/// lines rather than the turn. Nothing is allocated and nothing is re-parsed.
fn block_tag(text: &str, start: usize) -> Option<&str> {
    let line_end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    text[..line_end].rsplit('\n').find_map(|line| {
        let inner = line.strip_prefix('[')?.strip_suffix(']')?;
        (!inner.is_empty() && !inner.contains('[')).then_some(inner)
    })
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
///
/// The sinks are arguments so the split itself can be asserted — it is the
/// house rule `sessions` set, and a rule nothing tests is a suggestion.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors are ignored on purpose: `recount grep --json | head`
        // closing the pipe is not a failure of the command.
        let _ = writeln!(out, "{rendered}");
        let _ = digest(report, err);
    } else {
        let _ = digest(report, out);
    }
    Ok(())
}

fn digest(r: &Report, w: &mut dyn Write) -> io::Result<()> {
    let stamp = |t: Option<DateTime<Utc>>| match t {
        Some(t) => t.to_rfc3339_opts(SecondsFormat::Secs, true),
        None => "unbounded".to_string(),
    };

    writeln!(w, "recount grep {:?} — {}", r.needle, r.repo.display())?;
    writeln!(
        w,
        "  window   since {} · until {}",
        stamp(r.window.since),
        stamp(r.window.until)
    )?;
    if let Some(agent) = &r.agent {
        writeln!(
            w,
            "  agent    `{agent}` — sessions a signal NAMES, which is a guess, not a record"
        )?;
    }
    writeln!(
        w,
        "  scanned  {} session dir(s) · {} session(s) · {} turn(s)",
        r.session_dirs.len(),
        r.sessions_searched,
        r.turns_searched
    )?;
    if r.sessions_skipped_by_agent > 0 {
        writeln!(
            w,
            "  skipped  {} session(s) by --agent, {} of them because no signal names \
             any identity at all{}",
            r.sessions_skipped_by_agent,
            r.sessions_skipped_unknown_identity.len(),
            // The unattributable ones are named, not merely counted: a search
            // that dropped them silently would report "no hits" for a repo
            // full of testimony.
            if r.sessions_skipped_unknown_identity.is_empty() {
                ""
            } else {
                ":"
            }
        )?;
        for path in &r.sessions_skipped_unknown_identity {
            writeln!(w, "             {}", path.display())?;
        }
    }

    if r.hits.is_empty() {
        writeln!(
            w,
            "  no hits — a normal answer, not a failure: the needle is not in what was searched"
        )?;
        return Ok(());
    }
    writeln!(w, "  hits     {}", r.hits.len())?;

    for h in &r.hits {
        let mut facts = vec![stamp(Some(h.ts)), h.role.to_string()];
        if let Some(block) = &h.block_guess {
            facts.push(format!("[{block}]?"));
        }
        if h.occurrences > 1 {
            facts.push(format!("{} occurrences", h.occurrences));
        }
        writeln!(
            w,
            "\n  {}:{}  {}",
            h.session.display(),
            h.line,
            facts.join(" · ")
        )?;
        writeln!(w, "    {}", h.snippet)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::process::Command;

    use serde_json::Value;
    use tempfile::TempDir;

    use crate::connector::generic_jsonl::GenericJsonl;
    use crate::model::parse_ts;

    fn wt(path: &str, name: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive: true,
            session_dir: PathBuf::from("/nowhere"),
        }
    }

    /// `generic_jsonl` alone: both connectors accept `*.jsonl`, and these
    /// fixtures are in its dialect, not Claude Code's.
    fn only_generic() -> Vec<Box<dyn Connector>> {
        vec![Box::new(GenericJsonl)]
    }

    /// `{ts, role, content}` lines, one per argument.
    fn write_session(dir: &Path, name: &str, lines: &[(&str, &str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let body: String = lines
            .iter()
            .map(|(ts, role, text)| {
                format!("{{\"ts\":\"{ts}\",\"role\":\"{role}\",\"content\":{text:?}}}\n")
            })
            .collect();
        fs::write(&path, body).unwrap();
        path
    }

    fn query<'a>(needle: &'a str, agent: Option<&'a str>, window: TimeWindow) -> Query<'a> {
        Query {
            needle,
            agent,
            window,
        }
    }

    /// The spec's tuple, found without regard to case, with the coordinates a
    /// reader can jump to.
    #[test]
    fn a_hit_is_found_case_insensitively_and_carries_the_five_tuple() {
        let td = TempDir::new().unwrap();
        let session = write_session(
            td.path(),
            "s.jsonl",
            &[
                ("2026-08-19T07:00:00Z", "user", "why did gen.txt appear?"),
                (
                    "2026-08-19T07:01:00Z",
                    "assistant",
                    "[thinking]\nThe ARKANOID-v1 misdiagnosis said arkanoid was hand-written",
                ),
                ("2026-08-19T07:02:00Z", "user", "nothing to see"),
            ],
        );
        let worktrees = [wt("/r", "main")];
        let r = search(
            &only_generic(),
            &query("arkanoid", None, TimeWindow::default()),
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
        );

        assert_eq!(r.sessions_searched, 1);
        assert_eq!(r.turns_searched, 3);
        assert_eq!(r.hits.len(), 1, "one hit per turn, not per occurrence");
        let h = &r.hits[0];
        assert_eq!(h.session, session);
        assert_eq!(h.line, 2, "1-based line of the record in the file");
        assert_eq!(h.ts, parse_ts("2026-08-19T07:01:00Z").unwrap());
        assert_eq!(h.role, Role::Assistant);
        assert!(h.snippet.contains("ARKANOID-v1"), "{}", h.snippet);
        assert_eq!(h.occurrences, 2, "both spellings counted");
        assert_eq!(
            h.block_guess.as_deref(),
            Some("thinking"),
            "a hit in the reasoning is attributed to the reasoning"
        );
        // …and the snippet is one line: the `[thinking]\n` tag's newline is not
        // allowed to break the digest.
        assert!(!h.snippet.contains('\n'), "{}", h.snippet);

        // The documented JSON shape, key by key.
        let doc: Value = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(doc["command"], "grep");
        assert_eq!(doc["schema"], 1);
        assert_eq!(doc["needle"], "arkanoid");
        assert_eq!(doc["agent"], Value::Null);
        assert_eq!(doc["sessions_skipped_by_agent"], 0);
        assert_eq!(doc["hits"][0]["role"], "assistant");
        assert_eq!(doc["hits"][0]["line"], 2);
        assert_eq!(doc["hits"][0]["ts"], "2026-08-19T07:01:00Z");
    }

    /// The house rule the other commands inherit.
    #[test]
    fn json_goes_to_stdout_and_the_digest_to_stderr() {
        let r = search(
            &only_generic(),
            &query("x", None, TimeWindow::default()),
            Path::new("/r"),
            &[wt("/r", "main")],
            &[],
        );

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, true, &mut out, &mut err).unwrap();
        serde_json::from_slice::<Value>(&out).expect("stdout is JSON and nothing else");
        assert!(String::from_utf8_lossy(&err).contains("recount grep"));

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&r, false, &mut out, &mut err).unwrap();
        assert!(err.is_empty(), "no --json means nothing on stderr");
        assert!(String::from_utf8_lossy(&out).contains("recount grep"));
    }

    /// A needle that is not there is an empty list and exit 0 — the same rule
    /// `sessions` applies to a repo with no transcripts. Driven through the
    /// real `run`, real registry and real `$HOME`; hermetic because a fresh
    /// `TempDir`'s encoded session directories cannot exist on any machine.
    #[test]
    fn a_needle_that_matches_nothing_is_an_empty_list_and_exit_0() {
        let td = TempDir::new().unwrap();
        let repo = td.path().canonicalize().unwrap().join("repo");
        fs::create_dir(&repo).unwrap();
        for args in [
            ["init", "-q"].as_slice(),
            ["commit", "-q", "--allow-empty", "-m", "root"].as_slice(),
        ] {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .output()
                .expect("git on PATH")
                .status
                .success();
            assert!(ok, "git {args:?}");
        }

        let args = GrepArgs {
            needle: "no-such-needle-anywhere".to_string(),
            repo: repo.clone(),
            agent: None,
            window: crate::cli::WindowArgs {
                since: None,
                until: None,
            },
        };
        assert_eq!(run(&args, true).expect("no hits is not an error"), EXIT_OK);

        // And a needle that is not there in a directory that does have
        // transcripts is the same answer, inspected.
        let sessions = TempDir::new().unwrap();
        write_session(
            sessions.path(),
            "s.jsonl",
            &[("2026-08-19T07:00:00Z", "user", "a real turn")],
        );
        let r = search(
            &only_generic(),
            &query("absent", None, TimeWindow::default()),
            &repo,
            &[wt("/r", "main")],
            &[sessions.path().to_path_buf()],
        );
        assert!(r.hits.is_empty());
        assert_eq!(r.sessions_searched, 1, "it looked, and it did not find");
        let mut text = Vec::new();
        digest(&r, &mut text).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("no hits"), "{text}");
        assert!(!text.to_lowercase().contains("error"), "{text}");

        // The one input that *is* an error: there is no such thing as a search
        // for the empty string.
        let empty = GrepArgs {
            needle: String::new(),
            repo,
            agent: None,
            window: crate::cli::WindowArgs {
                since: None,
                until: None,
            },
        };
        assert!(run(&empty, false).is_err());
    }

    /// `--since` prunes files on mtime inside `discover`; both edges then
    /// filter turn by turn, because one file spans both sides of a window.
    #[test]
    fn since_and_until_filter_the_hits() {
        let td = TempDir::new().unwrap();
        let old = write_session(
            td.path(),
            "old.jsonl",
            &[("2026-08-01T00:00:00Z", "user", "needle in august the first")],
        );
        let new = write_session(
            td.path(),
            "new.jsonl",
            &[
                ("2026-08-19T00:00:00Z", "user", "needle in august the ninth"),
                ("2026-08-20T00:00:00Z", "user", "needle the day after"),
            ],
        );
        // mtimes match the content, which is what an append-only file gives.
        for (path, ts) in [
            (&old, "2026-08-01T00:00:00Z"),
            (&new, "2026-08-20T00:00:00Z"),
        ] {
            let f = fs::File::options().write(true).open(path).unwrap();
            f.set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::from(parse_ts(ts).unwrap())),
            )
            .unwrap();
        }
        let worktrees = [wt("/r", "main")];
        let dirs = [td.path().to_path_buf()];
        let hits = |window: TimeWindow| -> Vec<(PathBuf, usize)> {
            search(
                &only_generic(),
                &query("needle", None, window),
                Path::new("/r"),
                &worktrees,
                &dirs,
            )
            .hits
            .into_iter()
            .map(|h| (h.session, h.line))
            .collect()
        };

        assert_eq!(hits(TimeWindow::default()).len(), 3, "unbounded takes all");
        assert_eq!(
            hits(TimeWindow {
                since: parse_ts("2026-08-10T00:00:00Z").ok(),
                until: None,
            }),
            vec![(new.clone(), 1), (new.clone(), 2)],
            "--since prunes the old file on mtime"
        );
        assert_eq!(
            hits(TimeWindow {
                since: None,
                until: parse_ts("2026-08-19T12:00:00Z").ok(),
            }),
            vec![(old, 1), (new, 1)],
            "--until cannot prune on mtime, so the turn's own ts does it"
        );
        assert!(hits(TimeWindow {
            since: parse_ts("2027-01-01T00:00:00Z").ok(),
            until: None,
        })
        .is_empty());
    }

    /// `--agent` keeps what a signal names, and never drops an unattributable
    /// session silently.
    #[test]
    fn agent_keeps_what_a_signal_names_and_says_what_it_dropped() {
        let td = TempDir::new().unwrap();
        let mine = write_session(
            td.path(),
            "mine.jsonl",
            &[(
                "2026-08-19T07:00:00Z",
                "assistant",
                "export PACT_AGENT=cmdgrep — needle here",
            )],
        );
        write_session(
            td.path(),
            "theirs.jsonl",
            &[(
                "2026-08-19T07:00:00Z",
                "assistant",
                "export PACT_AGENT=cmdsessions — needle here too",
            )],
        );
        let anonymous = write_session(
            td.path(),
            "anon.jsonl",
            &[("2026-08-19T07:00:00Z", "assistant", "needle, unattributed")],
        );

        let worktrees = [wt("/r", "main"), wt("/r/wt/cmdgrep", "cmdgrep")];
        let dirs = [td.path().to_path_buf()];
        let r = search(
            &only_generic(),
            &query("needle", Some("cmdgrep"), TimeWindow::default()),
            Path::new("/r"),
            &worktrees,
            &dirs,
        );

        assert_eq!(r.sessions_searched, 1);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].session, mine);
        assert_eq!(r.sessions_skipped_by_agent, 2);
        assert_eq!(
            r.sessions_skipped_unknown_identity,
            vec![anonymous.clone()],
            "the one no signal names is listed, not merely counted"
        );

        // And the digest says so in words.
        let mut text = Vec::new();
        digest(&r, &mut text).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("2 session(s) by --agent"), "{text}");
        assert!(text.contains(&anonymous.display().to_string()), "{text}");
        assert!(text.contains("guess, not a record"), "{text}");

        // Without --agent the same corpus gives all three.
        let all = search(
            &only_generic(),
            &query("needle", None, TimeWindow::default()),
            Path::new("/r"),
            &worktrees,
            &dirs,
        );
        assert_eq!(all.hits.len(), 3);
        assert_eq!(all.sessions_skipped_by_agent, 0);
    }

    /// The topology half of the same filter, which needs a connector that
    /// states a cwd — `generic_jsonl` never does.
    #[test]
    fn agent_also_matches_the_worktree_a_cwd_resolves_into() {
        let td = TempDir::new().unwrap();
        let line = |cwd: &str, text: &str| {
            format!(
                "{{\"type\":\"assistant\",\"timestamp\":\"2026-08-19T07:00:00.000Z\",\
                 \"cwd\":\"{cwd}\",\"message\":{{\"role\":\"assistant\",\
                 \"content\":\"{text}\"}}}}\n"
            )
        };
        fs::write(
            td.path().join("in-worktree.jsonl"),
            line("/r/wt/cmdgrep", "needle, and no assignment anywhere"),
        )
        .unwrap();
        fs::write(
            td.path().join("at-root.jsonl"),
            line("/r", "needle, from the shared repo root"),
        )
        .unwrap();

        let worktrees = [wt("/r", "main"), wt("/r/wt/cmdgrep", "cmdgrep")];
        let r = search(
            &connector::registry(),
            &query("needle", Some("cmdgrep"), TimeWindow::default()),
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
        );

        assert_eq!(r.hits.len(), 1);
        assert!(r.hits[0].session.ends_with("in-worktree.jsonl"));
        // The repo root names no one (format-notes §4.3), so that session is
        // reported as unattributable rather than as someone else's.
        assert_eq!(r.sessions_skipped_unknown_identity.len(), 1);
    }

    /// The one most likely to panic in the wild: a multi-byte character
    /// straddling the snippet bound. `&str` slicing panics mid-codepoint and
    /// the spec says "never a panic".
    #[test]
    fn a_multibyte_boundary_near_the_snippet_bound_never_panics() {
        // 1-, 2-, 3- and 4-byte characters, so every residue mod 4 occurs at
        // some cut point.
        let filler = "aé→𝄞";
        let text = format!("{}NEEDLE{}", filler.repeat(60), filler.repeat(60));
        for (start, _) in text.char_indices() {
            let s = snippet(&text, start);
            // The bound is on the bytes copied; the two `…` are 3 bytes each.
            assert!(
                s.len() <= SNIPPET_MAX_BYTES + 6,
                "{} bytes at offset {start}",
                s.len()
            );
        }

        // The real path, end to end, with the match itself against the bound.
        let needle: Vec<char> = "𝄞needle".chars().collect();
        let text = format!("{}𝄞NEEDLE𝄞", "é".repeat(200));
        let (start, end) = find_ci(&text, &needle, 0).expect("found across cases");
        assert_eq!(&text[start..end], "𝄞NEEDLE");
        let s = snippet(&text, start);
        assert!(s.contains("𝄞NEEDLE"), "{s}");
        assert!(s.starts_with('…'), "{s}");

        // A needle longer than the whole budget still copies no more than it.
        let long: String = "é".repeat(400);
        let s = snippet(&long, 0);
        assert!(s.len() <= SNIPPET_MAX_BYTES + 6);
        assert!(!s.starts_with('…'), "nothing was cut from the front");
        assert!(s.ends_with('…'));
    }

    /// `docs/format-notes.md` §5: one real string is 9,610,205 characters. The
    /// snippet is bounded on the bytes copied, not on what was found.
    #[test]
    fn a_hit_in_a_very_large_turn_is_bounded() {
        let td = TempDir::new().unwrap();
        let huge = format!(
            "{}NEEDLE{}needle",
            "x".repeat(1_000_000),
            "y".repeat(1_000_000)
        );
        write_session(
            td.path(),
            "huge.jsonl",
            &[("2026-08-19T07:00:00Z", "user", huge.as_str())],
        );

        let r = search(
            &only_generic(),
            &query("needle", None, TimeWindow::default()),
            Path::new("/r"),
            &[wt("/r", "main")],
            &[td.path().to_path_buf()],
        );

        assert_eq!(r.hits.len(), 1, "one hit per turn");
        let h = &r.hits[0];
        assert_eq!(h.occurrences, 2);
        assert!(
            h.snippet.len() <= SNIPPET_MAX_BYTES + 6,
            "{} bytes out of a 2 MB turn",
            h.snippet.len()
        );
        assert!(h.snippet.contains("NEEDLE"), "the match itself is in it");
        assert!(h.snippet.starts_with('…') && h.snippet.ends_with('…'));
    }

    #[test]
    fn find_ci_folds_case_but_invents_nothing() {
        fn ci<'a>(hay: &'a str, needle: &str) -> Option<&'a str> {
            find_ci(hay, &needle.chars().collect::<Vec<_>>(), 0).map(|(s, e)| &hay[s..e])
        }
        assert_eq!(ci("The ARKANOID case", "arkanoid"), Some("ARKANOID"));
        assert_eq!(ci("café", "CAFÉ"), Some("café"));
        assert_eq!(ci("ΣΙΓΜΑ", "σιγμα"), Some("ΣΙΓΜΑ"));
        // The half-ASCII pair the fast path must not swallow: `K` is U+212A,
        // the Kelvin sign, and it lowercases to plain ASCII `k`. Only a test on
        // *both* sides keeps this folding.
        assert_eq!(ci("100 K", "k"), Some("K"));
        assert_eq!(ci("kelvin", "K"), Some("k"));
        // Still deliberately unfolded, fast path or not: a mapping that changes
        // the character count has no offset in the haystack to return. See the
        // module doc.
        assert_eq!(ci("İstanbul", "i̇stanbul"), None);
        assert_eq!(ci("STRASSE", "straße"), None);
        assert_eq!(ci("abc", "d"), None);
        // Not a regex, by design: the metacharacters are literals.
        assert_eq!(ci("a.c", "a.c"), Some("a.c"));
        assert_eq!(ci("abc", "a.c"), None);
        assert_eq!(ci("x", ""), None, "the empty needle matches nothing here");
        // Occurrences do not overlap: the scan resumes after the match.
        let n: Vec<char> = "aa".chars().collect();
        let (_, end) = find_ci("aaa", &n, 0).unwrap();
        assert_eq!(end, 2);
        assert_eq!(find_ci("aaa", &n, end), None);
    }

    #[test]
    fn block_tag_reads_the_rendered_tag_and_admits_what_it_cannot_see() {
        let text = "[thinking]\nthe reasoning\n[tool_use Bash]\n{\"command\":\"ls\"}";
        assert_eq!(block_tag(text, 0), Some("thinking"));
        assert_eq!(
            block_tag(text, text.find("reasoning").unwrap()),
            Some("thinking")
        );
        assert_eq!(
            block_tag(text, text.find("command").unwrap()),
            Some("tool_use Bash")
        );
        // An untagged turn is `null`, not a fabricated tag.
        assert_eq!(block_tag("just a typed prompt", 4), None);
        // The documented limit: an untagged `text` block after a `[thinking]`
        // one has no marker, so it inherits the tag above it. A guess.
        let lossy = "[thinking]\nreasoning\nplain speech";
        assert_eq!(
            block_tag(lossy, lossy.find("speech").unwrap()),
            Some("thinking")
        );
    }
}
