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
//!
//! # Two layouts, one connector (notes §0.1)
//!
//! ```text
//! <encoded>/<session-uuid>.jsonl                              238 files
//! <encoded>/<session-uuid>/subagents/agent-<hex>.jsonl        604 files
//! <encoded>/<session-uuid>/subagents/agent-<hex>.meta.json    the sidecar
//! <encoded>/<session-uuid>/subagents/workflows/wf_*/journal.jsonl   NOT a transcript
//! ```
//!
//! Three quarters of the corpus — and all of the fleet testimony recount
//! exists to retrieve — is in the nested shape. [`crate::connector::candidates`]
//! recurses, so both arrive here; [`discover`](ClaudeCode::discover) drops only
//! `journal.jsonl`, which holds `started`/`result` records with no `cwd`, no
//! `timestamp` and no `message` (§2.4) and would otherwise show up in `doctor`
//! forever as 29 sessions that have no cwd.
//!
//! # What a `Turn`'s `text` contains, per block type (§2.3)
//!
//! `message.content` is a bare string 1,977 times and an array of blocks
//! 132,093 times, and the bare strings are almost entirely the human's own
//! typed prompts. Both are read. A bare string is the text, verbatim and
//! untagged. An array is its blocks, rendered one per entry and joined with
//! newlines:
//!
//! | block | rendered as |
//! |---|---|
//! | `text` | the text, verbatim, no tag |
//! | `thinking` | `[thinking]\n` + the reasoning, in full |
//! | `tool_use` | `[tool_use <name>]\n` + `input` as compact JSON |
//! | `tool_result` | `[tool_result]\n` (or `[tool_result error]`) + its content, recursively |
//! | `image` | `[image]` — the marker only, never the bytes |
//! | anything else | `[<type>]` — `fallback` already exists and is undocumented |
//!
//! **`thinking` is extracted in full and this is the most consequential
//! decision in the module.** It is 24,851 blocks against `text`'s 13,347: the
//! single largest body of an agent's stated reasoning in the corpus, and
//! recount's entire job is answering "WHY". Dropping it to keep transcripts
//! tidy would gut the tool. The tag exists so that a `grep` hit can be
//! attributed — reasoning read differently from speech in an audit — and so
//! the README can warn that `recount grep` searches reasoning the user may not
//! expect to be searchable (§2.3).
//!
//! `tool_result.content` is itself a string 46,004 times and an array 854
//! times, so the same renderer recurses one level into it.
//!
//! # Which lines become turns
//!
//! Nineteen `type` values occur (§2). Four carry `cwd` and content —
//! `assistant`, `user`, `attachment`, `system` — and only three of those
//! become turns:
//!
//! * `assistant`, `user` — testimony, always.
//! * `system` — hooks and compaction, rendered `[system:<subtype>]\n<content>`,
//!   **when it has any content**. The notes' §2 table said a `system` line
//!   carries `content` as a string; re-measured over the whole corpus while
//!   writing this module, 1,618 of 1,963 do not — `turn_duration` (828) and
//!   `stop_hook_summary` (790) carry only `durationMs`/`hookCount` telemetry.
//!   Those are skipped after their `cwd` is counted, exactly like an
//!   attachment, and the notes' table has been corrected. The 345 that do
//!   carry prose (`away_summary`, `local_command`, `compact_boundary`,
//!   `scheduled_task_fire`, …) become turns.
//! * `attachment` — read for its `cwd` and then **skipped** (§2.3, §7.9):
//!   10,309 lines of injected boilerplate (`hook_success`, `task_reminder`,
//!   `total_tokens_reminder`) that is neither human nor model speech and would
//!   flood every `grep` result.
//! * the other fifteen kinds, and any kind that does not exist yet — skipped
//!   silently.
//!
//! **A skipped kind is neither tally.** [`SessionMeta`] carries two numbers
//! and this module is careful about which one it reaches for, because `doctor`
//! shows them to a human:
//!
//! * `parse_failures` means **damage** — a line that is not JSON, a line that
//!   is not a JSON object, a testimony line whose timestamp will not parse.
//!   The reader tried and could not.
//! * `declined` means **this reader chose not to** — today, exactly one thing:
//!   a line over [`MAX_LINE_BYTES`]. The line is intact; recount refused it.
//!   §5.1 is the whole reason the two are separate fields.
//! * a kind this connector does not read is in **neither**. An `attachment`,
//!   a `queue-operation`, one of the other fifteen kinds: that is the format's
//!   structure, not a loss. Tallying 10,309 attachments as "declined" would
//!   drown the twelve lines that actually went unread.
//!
//! A line with no `type` at all was never observed in 169,775 lines, which
//! §2 notes "is not a licence to panic on one" — it is treated as an unknown
//! kind and skipped, not tallied, because the JSON itself was intact.
//!
//! `queue-operation` (2,076 lines) does carry a timestamp and queued user text
//! but no `cwd`, so it can never take part in a topological join; it is left
//! in the skipped set with the rest.
//!
//! # cwd is a set, and `SessionMeta.cwd` is the most frequent member
//!
//! 46 of 871 files hold more than one `cwd` and one holds nine (§4.2), so
//! "the session's cwd" does not exist. [`SessionMeta::cwd`] has room for one
//! value, so the rule is: **the most frequently occurring cwd wins, ties broken
//! by first appearance.** Most-frequent rather than first because the real
//! example in §4.2 — 167 lines at the repo root against 175 in a worktree —
//! resolves to the more specific and more informative of the two, where "first"
//! would have answered with whichever directory the process happened to start
//! in.
//!
//! One value is not enough for a correct `sessions --repo`, which must match
//! if **any** cwd resolves into the repo (§4.2), so the whole set is surfaced
//! as [`Identity::cwds`], ordered by that same rule. A caller that uses
//! `meta.cwd` alone is getting a summary, not the evidence.
//!
//! # What the join gets that `SessionMeta` has no field for
//!
//! [`Identity`], from [`scan`]. `sessionId` is **not** a per-file identity —
//! all 604 subagent files carry their parent's (§3.1) — so `agent_id` (which
//! equals the filename hex in 604 of 604 files) is carried beside it, and the
//! sidecar's `description` ("W0 empirical format-notes bead") is carried too.
//! Under the subagent topology this fleet actually runs (§4.3) every agent
//! shares one `cwd` and the topological chain cannot discriminate between
//! siblings; the sidecar `description` and a transcript mention are the only
//! signals that can, and the sidecar costs one small file read and no
//! transcript. [`sidecar`] is public separately for exactly that reason.
//!
//! Not surfaced, deliberately: `worktree-state` / `relocated` (§4.3 signals 2)
//! are authoritative but only ever appear for Claude Code's own `/worktree`
//! feature, never for the `wt/<agent>` fleets recount audits, and nothing here
//! consumes them. §4.3 says where they live if that changes.
//!
//! # Two things a pact event can ask for by key (§11)
//!
//! A pact 0.11.0 event that carries `harness_session` and `harness_subagent`
//! has already done the hard part. [`keyed`] turns those two ids into a path
//! — **one join and one `stat`, never a scan** — and answers in three ways,
//! because "the file is not there" and "that is not an id" are different news
//! for the human reading the report. The 371 workflow-nested transcripts it
//! cannot name are the documented ceiling; [`keyed`]'s own doc argues why that
//! costs nothing in practice.
//!
//! And the answer to "does a session record which model spoke" is **yes, per
//! turn**: `message.model` is on 85,242 of 85,242 assistant lines, it is a
//! bare slug directly comparable to a `PACT_MODEL` value, and ten files change
//! it mid-transcript, so it is a set on [`Identity::models`] rather than one
//! latched value. Only `<synthetic>` is dropped: see [`SYNTHETIC`], and §11.2
//! for why keeping it would manufacture a discrepancy out of an API timeout.
//!
//! # Reading (§5, §6, §7)
//!
//! * **8 MB line cap, drained not buffered.** Thirty-one real lines exceed
//!   1 MB; **twelve** exceed 10 MB, those twelve are exactly the ones over the
//!   cap, and the largest is 16.4 MB. They are **not** a pasted dump. They are
//!   the human's own bare-string prompt — the testimony §2.3 calls "precisely
//!   what recount exists to find" — wrecked by runaway self-escaping and
//!   retried six times in six minutes until the API answered `400 … the
//!   request is ~8391167 tokens` (§5.1). It is the most audit-worthy line in
//!   the corpus, and it is still skipped: 9.6 MB of backslashes helps nobody
//!   and holding it costs 16 MB. But it is skipped *knowingly*, so it counts
//!   as [`SessionMeta::declined`], never as damage — the file it lives in is
//!   byte-perfect JSON. The rest of the line is consumed straight out of the
//!   `BufReader` without ever being held; one enormous line never aborts a
//!   file. If the cap ever needs a better answer than "drop", §5.1 says the
//!   answer is a truncated excerpt with a marker, not a bigger buffer.
//! * **Bytes, lossily.** A tool result can hold arbitrary program output.
//! * **min/max for the span, never first/last.** 246 files contain timestamp
//!   inversions, worst 12 hours (§6.1). Nothing here binary-searches or stops
//!   early on time.
//! * **A truncated tail is one parse failure at the last line.** §7.4 asks for
//!   it to be reported distinctly as `truncated_tail: true`; [`SessionMeta`]
//!   still has no such field, so a benign half-written tail and a genuinely
//!   broken line remain one number. That is a smaller lie than the one §7.4's
//!   third case caused — recount's own cap reported as corruption — and that
//!   one is fixed: it is [`SessionMeta::declined`] now. "Counted, not fatal" is
//!   satisfied either way; if the benign/damaged split is ever worth acting on,
//!   it is the same shape of change as this one.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::connector::{candidates, Connector};
use crate::model::{parse_ts, Role, SessionMeta, TimeWindow, Turn};
use crate::prune;

const NAME: &str = "claude_code";

/// §5: 31 real lines exceed 1 MB, 12 exceed 10 MB, and the largest is 16.4 MB.
/// Above this a line is declined rather than held; see the module doc.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// §11.2: what `message.model` says on an assistant line the model did not
/// write — an API error the CLI rendered as a turn (72 of 80 on this machine),
/// or its "No response requested." no-op (8). Never a model, so never a model
/// this connector reports.
const SYNTHETIC: &str = "<synthetic>";

/// The line buffer is reused, so one oversized line would otherwise leave its
/// capacity allocated for the rest of the file.
const LINE_BUFFER_KEEP: usize = 64 * 1024;

/// The root of the encoded per-project session directories.
///
/// Returned whether or not it exists — `doctor` reports the root it looked in,
/// and "the place you expected is not there" is the answer people need.
pub fn projects_root() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join(".claude")
        .join("projects")
}

/// Where a keyed lookup landed (§11).
///
/// Three outcomes, and the last two are not the same news. `Absent` says the
/// path is right and the file is not there — pruned by Claude Code's own
/// 30-day retention (§0.0), or written on another machine — so the answer is
/// "look elsewhere". `Malformed` says the id is not an id, so the answer is
/// "look at what you were handed". Collapsing them into `None` would tell a
/// human to go hunting for a transcript that was never named.
///
/// `dead_code`: the keyed tier of the join is the next bead and the tests are
/// this type's only caller until it lands. The allow comes off with the first
/// real one — it is not a licence for the type to stay unused.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyed {
    /// The transcript is on disk, here.
    Found(PathBuf),
    /// Well-formed ids, and this is the path they name. Nothing is at it.
    Absent(PathBuf),
    /// Not an id. Nothing was joined, nothing was stat'd, no path is offered.
    Malformed(String),
}

/// The transcript a pact event's `harness_session` / `harness_subagent` name,
/// **without scanning**: one path join and one `stat` (§11).
///
/// `project_dir` is an encoded session directory —
/// [`crate::worktree::session_dirs`] produces them, and a caller with several
/// (a repo and its worktrees) asks each in turn; that is still a handful of
/// stats, not a walk. `subagent` is `None` for the parent session itself,
/// whose file is `<session>.jsonl` and whose stem equals its `sessionId` in
/// 238 of 238 files (§3.1):
///
/// ```text
/// <project_dir>/<session>.jsonl                              subagent: None
/// <project_dir>/<session>/subagents/agent-<subagent>.jsonl   subagent: Some
/// ```
///
/// Both ids are treated as **opaque tokens with a checked shape**: non-empty
/// and `[A-Za-z0-9-]` only. That is not a uuid check or a hex check on purpose
/// — the notes' whole thesis is that this format drifts, and a stricter rule
/// would answer `Malformed` to an id a later Claude Code mints perfectly
/// legally. It is exactly strict enough to make the join a join: no `/`, no
/// `..`, no NUL, so a hostile or corrupt ledger cannot walk out of
/// `project_dir`.
///
/// Nothing here reads the file. `agentId` inside it is the authority if the
/// filename is ever doubted (§10.5), and the caller has [`scan`] for that.
///
/// **The ceiling, measured:** 371 of 634 subagent transcripts on this machine
/// live one more level down, in `subagents/workflows/wf_<id>/`, and the `wf_`
/// id is not in the event. Those are `Absent` here and belong to the
/// topological tier. This is not the gap it looks like: the only recipe by
/// which an agent can learn its own id (§10.3) globs the flat directory, so an
/// id that reached a pact event came from a flat file by construction.
#[allow(dead_code)] // Same as [`Keyed`]: the join's keyed tier is the next bead.
pub fn keyed(project_dir: &Path, session: &str, subagent: Option<&str>) -> Keyed {
    if let Some(why) = malformed("harness_session", session) {
        return Keyed::Malformed(why);
    }
    let path = match subagent {
        Some(id) => {
            if let Some(why) = malformed("harness_subagent", id) {
                return Keyed::Malformed(why);
            }
            project_dir
                .join(session)
                .join("subagents")
                .join(format!("agent-{id}.jsonl"))
        }
        None => project_dir.join(format!("{session}.jsonl")),
    };
    // The one stat.
    match path.is_file() {
        true => Keyed::Found(path),
        false => Keyed::Absent(path),
    }
}

/// The shape check behind [`keyed`]. `None` means the token is usable as one
/// path component.
fn malformed(field: &str, id: &str) -> Option<String> {
    if id.is_empty() {
        return Some(format!("{field} is empty"));
    }
    id.chars()
        .find(|c| !c.is_ascii_alphanumeric() && *c != '-')
        .map(|c| format!("{field} {id:?} contains {c:?}, which is outside [A-Za-z0-9-]"))
}

/// The `agent-<hex>.meta.json` beside a subagent transcript (§3.4).
///
/// Every field is optional: 604 of 604 sidecars exist today, but §8 flags that
/// a subagent killed before its sidecar was flushed is plausible, and the
/// observed key set already varies (`model`, `stoppedByUser`). No
/// `deny_unknown_fields`, ever.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sidecar {
    /// e.g. `general-purpose`, `Explore`, `workflow-subagent`.
    pub agent_type: Option<String>,
    /// The spawning task's one-line label — "W0 empirical format-notes bead".
    /// The strongest identity signal a subagent has (§4.3).
    pub description: Option<String>,
    pub tool_use_id: Option<String>,
    pub spawn_depth: Option<u32>,
}

/// What one session file says about who it was, beyond what [`SessionMeta`]
/// has a field for. See the module doc.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// §3.1: the real per-file identity, equal to the filename hex in 604 of
    /// 604 subagent files. Absent on top-level sessions.
    pub agent_id: Option<String>,
    /// §3.1: **not unique across files** — every subagent carries its parent's.
    /// Keying a map on this merges a session with all of its subagents.
    pub session_id: Option<String>,
    /// §4.2: every distinct cwd in the file, most frequent first, ties by first
    /// appearance. `sessions --repo` must match on *any* of these.
    pub cwds: Vec<PathBuf>,
    /// §3.4, the cheap discriminator. `None` for a top-level session or a
    /// subagent whose sidecar is missing.
    pub sidecar: Option<Sidecar>,
    /// §11: every distinct `message.model` the file's assistant lines recorded,
    /// most frequent first, ties by first appearance — the same rule as
    /// [`Identity::cwds`], for the same reason: it is a set, and one value is
    /// a summary of it.
    ///
    /// Verbatim, and it is a **slug**, not a display name: `claude-opus-5`,
    /// `claude-haiku-4-5-20251001`. `<synthetic>` is excluded (§11.2) — it is
    /// the CLI's own stub on an API-error record, not a model, and comparing a
    /// declared `PACT_MODEL` against it would invent a discrepancy.
    ///
    /// Empty for a file with no assistant line, which is a real and unalarming
    /// state. A comparison that has nothing to compare must stay silent.
    pub models: Vec<String>,
}

/// One file, read once: the connector-agnostic metadata plus the
/// claude_code-specific [`Identity`] the join needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scan {
    pub meta: SessionMeta,
    pub identity: Identity,
}

pub struct ClaudeCode;

impl Connector for ClaudeCode {
    fn name(&self) -> &'static str {
        NAME
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![projects_root()]
    }

    fn discover(&self, dirs: &[PathBuf], window: &TimeWindow) -> Vec<SessionMeta> {
        let files: Vec<PathBuf> = candidates(dirs)
            .into_iter()
            .filter(|p| !is_journal(p))
            .collect();
        // `window` is the mtime prune and nothing else. Every file that was
        // opened comes back, in window or not: recognising it IS the claim,
        // and dropping one here would hand a transcript this connector reads
        // perfectly well to the fallback, which reads it as nothing. The span
        // filter lives once, in `connector::discover_all`.
        prune::partition(&files, window)
            .opened
            .iter()
            .map(|path| scan(path).meta)
            .collect()
    }

    fn read(&self, path: &Path) -> Vec<Turn> {
        pass(path).turns
    }
}

/// Read one file and report both what it holds and who it was.
///
/// Public because [`Connector::discover`] can only return [`SessionMeta`], and
/// the join needs the [`Identity`] that does not fit in one.
pub fn scan(path: &Path) -> Scan {
    let p = pass(path);
    // min/max, never first/last: 246 of 842 files contain timestamp
    // inversions, the worst 12 hours wide (§6.1).
    let meta = SessionMeta {
        path: path.to_path_buf(),
        cwd: p.cwds.first().map(|(c, _)| c.clone()),
        first_ts: p.turns.iter().map(|t| t.ts).min(),
        last_ts: p.turns.iter().map(|t| t.ts).max(),
        turn_count: p.turns.len(),
        parse_failures: p.failures,
        declined: p.declined,
        connector: NAME.to_string(),
    };
    let identity = Identity {
        agent_id: p.agent_id,
        session_id: p.session_id,
        cwds: p.cwds.into_iter().map(|(c, _)| c).collect(),
        sidecar: sidecar(path),
        models: p.models.into_iter().map(|(m, _)| m).collect(),
    };
    Scan { meta, identity }
}

/// The `.meta.json` beside `path`, if there is one (§3.4).
///
/// One small file read and no transcript — the cheapest fleet-identity signal
/// there is. `None` for a top-level session, for a missing sidecar, and for a
/// sidecar that is not readable JSON: none of those is an error.
pub fn sidecar(path: &Path) -> Option<Sidecar> {
    let bytes = fs::read(path.with_extension("meta.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// §2.4: `…/subagents/workflows/wf_*/journal.jsonl` holds `started`/`result`
/// records with no cwd, no timestamp and no message. It is not a transcript.
fn is_journal(path: &Path) -> bool {
    path.file_name().is_some_and(|n| n == "journal.jsonl")
}

/// One pass over one file. Never fails; an unreadable file is empty.
#[derive(Default)]
struct Pass {
    turns: Vec<Turn>,
    /// Lines that did not parse. Damage.
    failures: usize,
    /// Lines over the cap. Not damage — see the module doc.
    declined: usize,
    /// Distinct cwds with their line counts, most frequent first (§4.2).
    cwds: Vec<(PathBuf, usize)>,
    /// Distinct assistant models with their line counts, same rule (§11).
    models: Vec<(String, usize)>,
    agent_id: Option<String>,
    session_id: Option<String>,
}

fn pass(path: &Path) -> Pass {
    let mut out = Pass::default();
    let Ok(file) = fs::File::open(path) else {
        return out;
    };
    let mut reader = BufReader::new(file);
    let mut raw = Vec::new();
    let mut number = 0usize;

    loop {
        match read_capped(&mut reader, &mut raw) {
            Read1::End => break,
            Read1::Oversized => {
                number += 1;
                // Content *was* lost, and on this machine it was the human's
                // own prompt (§5.1) — so it is tallied, but as a line recount
                // declined to read, never as a line that failed to parse. The
                // file around it is intact.
                out.declined += 1;
                continue;
            }
            Read1::Line => number += 1,
        }

        // §7.10: read as bytes, decode lossily. A tool result can carry
        // arbitrary program output and one bad byte must not cost a file.
        let text = String::from_utf8_lossy(&raw);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            out.failures += 1;
            continue;
        };
        let Some(obj) = value.as_object() else {
            // Every one of 169,775 lines was a JSON object; a bare scalar or
            // array is damage, not a line kind.
            out.failures += 1;
            continue;
        };

        // Identity first: these keys ride on lines of several kinds, including
        // ones that never become turns.
        if out.agent_id.is_none() {
            out.agent_id = str_field(obj.get("agentId"));
        }
        if out.session_id.is_none() {
            // §7.1: both spellings occur, same value. Prefer the camelCase one.
            out.session_id =
                str_field(obj.get("sessionId")).or_else(|| str_field(obj.get("session_id")));
        }

        // §7.2: match `type` as a string with a catch-all arm. An unknown kind
        // — or, unobserved but not impossible, no kind at all — is a skip.
        let kind = obj.get("type").and_then(Value::as_str).unwrap_or_default();
        if !matches!(kind, "assistant" | "user" | "attachment" | "system") {
            continue;
        }

        // §4.1: cwd is on 100% of these four kinds and 0% of the other fifteen.
        if let Some(cwd) = str_field(obj.get("cwd")) {
            tally(&mut out.cwds, PathBuf::from(cwd));
        }
        // §2.3, §7.9: injected context, not speech. Its cwd counted above.
        if kind == "attachment" {
            continue;
        }

        // §2: `timestamp` is on 100% of these kinds. One that is missing or
        // unparseable is damage — a turn with a guessed time could be silently
        // matched by the join, which is worse than a counted loss.
        let Some(ts) = str_field(obj.get("timestamp")).and_then(|t| parse_ts(&t).ok()) else {
            out.failures += 1;
            continue;
        };

        let (role, body) = match kind {
            "assistant" => {
                // §11: `message.model` is on 85,242 of 85,242 assistant lines
                // and on no other kind. It is per-turn and it does change
                // mid-file, so it is tallied like `cwd` rather than latched.
                if let Some(model) = str_field(obj.get("message").and_then(|m| m.get("model")))
                    .filter(|m| m != SYNTHETIC)
                {
                    tally(&mut out.models, model);
                }
                (Role::Assistant, content_text(obj.get("message")))
            }
            "user" => (Role::User, content_text(obj.get("message"))),
            // §2.2: `system` has no `message`; `subtype` says which hook or
            // boundary it is, and prose — when there is any — is a top-level
            // string `content`. 1,618 of 1,963 system lines have none: see the
            // module doc, and §2 of the notes, which this corrected.
            _ => match system_text(obj) {
                Some(body) => (Role::System, body),
                None => continue,
            },
        };
        out.turns.push(Turn {
            ts,
            role,
            text: body,
            line: number,
        });
    }

    // Most frequent first, ties by first appearance — `sort_by_key` is stable,
    // so insertion order survives the sort (module doc, §4.2).
    out.cwds.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out.models.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out
}

/// `.message.content`, which is `String | Vec<Block>` (§2.3).
fn content_text(message: Option<&Value>) -> String {
    blocks_text(message.and_then(|m| m.get("content")))
}

/// The same shape one level down, because `tool_result.content` is also
/// string-or-array (§2.3).
fn blocks_text(content: Option<&Value>) -> String {
    match content {
        // The human's own typed prompt, 1,977 times. Verbatim, untagged.
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => {
            let rendered: Vec<String> = blocks
                .iter()
                .map(block_text)
                .filter(|s| !s.is_empty())
                .collect();
            rendered.join("\n")
        }
        _ => String::new(),
    }
}

/// One content block. See the table in the module doc.
fn block_text(block: &Value) -> String {
    let kind = block
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "text" => block
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        // The point of the whole tool. 24,851 blocks of stated reasoning.
        "thinking" => tagged(
            "thinking",
            block.get("thinking").and_then(Value::as_str).unwrap_or(""),
        ),
        "tool_use" => {
            let name = block.get("name").and_then(Value::as_str).unwrap_or("?");
            // `input` is tool-specific and arbitrary (§2.3); compact JSON keeps
            // it greppable — a Bash command, an edited path — without pretending
            // to know its shape.
            let input = block
                .get("input")
                .map(|i| serde_json::to_string(i).unwrap_or_default())
                .unwrap_or_default();
            tagged(&format!("tool_use {name}"), &input)
        }
        "tool_result" => {
            let tag = if block.get("is_error").and_then(Value::as_bool) == Some(true) {
                "tool_result error"
            } else {
                "tool_result"
            };
            tagged(tag, &blocks_text(block.get("content")))
        }
        // Never the bytes.
        "image" => "[image]".to_string(),
        // `fallback` already exists and is undocumented (§2.3); the next
        // unknown block type lands here too, and neither crashes.
        other => format!("[{}]", if other.is_empty() { "block" } else { other }),
    }
}

/// §2.3: `system` lines carry `.subtype` (`turn_duration`, `stop_hook_summary`,
/// `compact_boundary`, …), and prose in a top-level string `content` — but only
/// sometimes.
///
/// `None` for a system line with no `content`, which is 1,618 of the 1,963 in
/// the corpus: `turn_duration` (828) carries `durationMs`/`messageCount` and
/// `stop_hook_summary` (790) carries `hookCount`/`hookInfos`, and neither says
/// anything a person testified to. The notes' §2 table said `system` carries
/// `content` as a string full stop; it is true only of `away_summary` (276),
/// `local_command` (41), `scheduled_task_fire` (14), `compact_boundary` (12),
/// `model_refusal_fallback` and `informational`. The table has been corrected.
fn system_text(obj: &serde_json::Map<String, Value>) -> Option<String> {
    let body = str_field(obj.get("content"))?;
    Some(match obj.get("subtype").and_then(Value::as_str) {
        Some(sub) => tagged(&format!("system:{sub}"), &body),
        None => body,
    })
}

fn tagged(tag: &str, body: &str) -> String {
    if body.is_empty() {
        format!("[{tag}]")
    } else {
        format!("[{tag}]\n{body}")
    }
}

/// A field that is a non-empty string. `null` and a wrong type both read as
/// absent — §2.2's field inventory drifts by producer version, so a key that
/// is present with the wrong shape is normal, not an error.
fn str_field(v: Option<&Value>) -> Option<String> {
    match v.and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// Increment `item`'s tally, appending it if new. At most 9 distinct cwds were
/// observed in one file (§4.2) and at most 3 distinct models (§11.1), so a
/// linear scan beats a map and keeps insertion order for the tie-break.
fn tally<T: PartialEq>(seen: &mut Vec<(T, usize)>, item: T) {
    match seen.iter_mut().find(|(s, _)| *s == item) {
        Some((_, n)) => *n += 1,
        None => seen.push((item, 1)),
    }
}

enum Read1 {
    Line,
    Oversized,
    End,
}

/// One line into `buf`, newline included, never allocating past the cap.
///
/// An oversized line is drained straight out of the `BufReader` rather than
/// buffered (§5, §7.5) — a 16.4 MB line exists and holding it would be the
/// pathology the notes warn about. An I/O error mid-file ends the read with
/// whatever was already parsed, because "one bad line never fails a file"
/// makes even less sense as "one bad sector loses the session".
fn read_capped<R: BufRead>(reader: &mut R, buf: &mut Vec<u8>) -> Read1 {
    buf.clear();
    buf.shrink_to(LINE_BUFFER_KEEP);
    match reader
        .by_ref()
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', buf)
    {
        Ok(0) => return Read1::End,
        Ok(_) => {}
        Err(_) => return Read1::End,
    }
    if buf.len() <= MAX_LINE_BYTES {
        return Read1::Line;
    }
    if buf.last() != Some(&b'\n') {
        // Consume the tail of the oversized line without holding any of it.
        loop {
            // Decided before consuming, so the `fill_buf` borrow ends first.
            let (eat, done) = match reader.fill_buf() {
                Ok(chunk) if !chunk.is_empty() => match chunk.iter().position(|&b| b == b'\n') {
                    Some(i) => (i + 1, true),
                    None => (chunk.len(), false),
                },
                // EOF, or an I/O error: the line has no end to find.
                _ => break,
            };
            reader.consume(eat);
            if done {
                break;
            }
        }
    }
    Read1::Oversized
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    /// A fixture tree in both layouts of §0.1, carrying every shape the notes
    /// found plus the four pathologies they did *not* (§5: zero bad lines, zero
    /// truncated tails, zero empty files in the wild — the tolerance is
    /// required by the spec, so nature not supplying a specimen is not an
    /// excuse for leaving it untested).
    fn fixture() -> TempDir {
        let d = tempfile::tempdir().unwrap();
        let proj = d.path().join("-tmp-repo");
        let sub = proj.join("11111111-2222-3333-4444-555555555555/subagents");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(sub.join("workflows/wf_1")).unwrap();

        // Layout 1: a top-level session. Bare-string user content (the human's
        // typed prompt), an assistant array with thinking + text + tool_use, a
        // user array with a tool_result whose content is an array, a system
        // line, an attachment (cwd only, no turn), and three metadata kinds
        // that must be skipped without being called corruption.
        write(
            &proj.join("11111111-2222-3333-4444-555555555555.jsonl"),
            &[
                r#"{"type":"user","timestamp":"2026-08-19T07:00:00.000Z","cwd":"/tmp/repo","sessionId":"11111111-2222-3333-4444-555555555555","message":{"role":"user","content":"why did you delete it"}}"#,
                r#"{"type":"mode","mode":"normal","sessionId":"11111111-2222-3333-4444-555555555555"}"#,
                r#"{"type":"assistant","timestamp":"2026-08-19T07:02:00.000Z","cwd":"/tmp/repo","unknownFieldFromANewerVersion":42,"message":{"role":"assistant","model":"claude-sonnet-5","content":[{"type":"thinking","thinking":"the file was generated","signature":"sig"},{"type":"text","text":"It was generated."},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"rm -f gen.txt"}}]}}"#,
                r#"{"type":"attachment","timestamp":"2026-08-19T07:02:30.000Z","cwd":"/tmp/repo","attachment":{"type":"hook_success"}}"#,
                r#"{"type":"user","timestamp":"2026-08-19T07:03:00.000Z","cwd":"/tmp/repo","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"content":[{"type":"text","text":"ok"},{"type":"image"},{"type":"tool_reference"}]}]},"toolUseResult":{"stdout":"ok"}}"#,
                r#"{"type":"system","timestamp":"2026-08-19T07:04:00.000Z","cwd":"/tmp/repo","subtype":"away_summary","content":"hook ran"}"#,
                // A system line with no `content` at all: telemetry, not
                // testimony, and 1,618 of the 1,963 in the corpus.
                r#"{"type":"system","timestamp":"2026-08-19T07:04:30.000Z","cwd":"/tmp/repo","subtype":"turn_duration","durationMs":806352,"messageCount":156}"#,
                // §11.1: a model switch mid-file — 10 real files do this, 3
                // of them subagent transcripts. `claude-opus-5` ends up on two
                // lines against `claude-sonnet-5`'s one, so most-frequent-first
                // is distinguishable from first-seen here too.
                r#"{"type":"assistant","timestamp":"2026-08-19T07:03:20.000Z","cwd":"/tmp/repo","message":{"role":"assistant","model":"claude-opus-5","content":[{"type":"text","text":"switched model mid-session"}]}}"#,
                // §11.2: not a model. An API error the CLI rendered as an
                // assistant turn. It is testimony (it says why the run
                // stalled) and it must never reach `Identity::models`.
                r#"{"type":"assistant","timestamp":"2026-08-19T07:03:40.000Z","cwd":"/tmp/repo","isApiErrorMessage":true,"apiErrorStatus":429,"message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"API Error: 429"}]}}"#,
                // §6.1: timestamps are not monotonic. This one is 12 hours
                // BEFORE the file's first line and must widen the span.
                r#"{"type":"assistant","timestamp":"2026-08-18T19:00:00.000Z","cwd":"/tmp/repo","message":{"role":"assistant","model":"claude-opus-5","content":[{"type":"text","text":"resumed from yesterday"},{"type":"fallback"}]}}"#,
                r#"{"type":"ai-title","aiTitle":"deleting things"}"#,
                r#"{"type":"brand-new-kind-from-2027","whatever":true}"#,
            ],
        );

        // Layout 2: a subagent, with its sidecar. Carries the PARENT's
        // sessionId (§3.1) and its own agentId.
        write(
            &sub.join("agent-a34d3106441943c34.jsonl"),
            &[
                r#"{"type":"user","timestamp":"2026-08-19T07:10:00.000Z","cwd":"/tmp/repo","isSidechain":true,"sessionId":"11111111-2222-3333-4444-555555555555","agentId":"a34d3106441943c34","message":{"role":"user","content":"write the notes"}}"#,
                r#"{"type":"assistant","timestamp":"2026-08-19T07:11:00.000Z","cwd":"/tmp/repo","agentId":"a34d3106441943c34","message":{"role":"assistant","model":"claude-opus-5","content":[{"type":"thinking","thinking":"cwd will not name my worktree"}]}}"#,
            ],
        );
        fs::write(
            sub.join("agent-a34d3106441943c34.meta.json"),
            r#"{"agentType":"general-purpose","description":"W0 empirical format-notes bead","toolUseId":"toolu_01x","spawnDepth":1,"model":"opus","stoppedByUser":false}"#,
        )
        .unwrap();

        // §11: a real subagent transcript one level deeper, in a workflow
        // directory whose `wf_` id is in no pact event. 371 of 634 on this
        // machine look like this. `discover` reads it; `keyed` cannot name it.
        write(
            &sub.join("workflows/wf_1/agent-b0000000000000001.jsonl"),
            &[
                r#"{"type":"user","timestamp":"2026-08-19T07:12:00.000Z","cwd":"/tmp/repo","agentId":"b0000000000000001","message":{"role":"user","content":"nested in a workflow"}}"#,
            ],
        );

        // §2.4: not a transcript. discover() must not return it.
        write(
            &sub.join("workflows/wf_1/journal.jsonl"),
            &[
                r#"{"type":"started","agentId":"wf"}"#,
                r#"{"type":"result","summary":"did the thing"}"#,
            ],
        );

        // §4.2: one file, three cwds — 3 x b, 2 x a, 1 x c. Insertion order
        // puts `a` first, so a most-frequent rule is distinguishable from a
        // first-seen one, and the tie-break is exercised by `a` vs `c`. Two of
        // b's three are kinds that never become turns, which is how a cwd
        // counted before the skip is told apart from one counted after.
        let multi: Vec<String> = [
            ("/tmp/repo/a", "07:20", "user"),
            ("/tmp/repo/b", "07:21", "user"),
            ("/tmp/repo/a", "07:22", "user"),
            ("/tmp/repo/b", "07:23", "attachment"),
            ("/tmp/repo/c", "07:24", "user"),
            ("/tmp/repo/b", "07:25", "system"),
        ]
        .iter()
        .map(|(cwd, hm, kind)| {
            format!(
                r#"{{"type":"{kind}","timestamp":"2026-08-19T{hm}:00.000Z","cwd":"{cwd}","subtype":"turn_duration","attachment":{{"type":"task_reminder"}},"message":{{"role":"user","content":"x"}}}}"#
            )
        })
        .collect();
        write(
            &proj.join("22222222-2222-3333-4444-555555555555.jsonl"),
            &multi.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        // Damage: garbage, a non-object line, a timeless testimony line, and a
        // truncated tail (last line cut mid-write) — around two good turns.
        fs::write(
            proj.join("33333333-2222-3333-4444-555555555555.jsonl"),
            concat!(
                r#"{"type":"user","timestamp":"2026-08-19T07:30:00.000Z","cwd":"/tmp/repo","message":{"role":"user","content":"before"}}"#,
                "\n",
                "not json at all\n",
                "[1,2,3]\n",
                r#"{"type":"assistant","cwd":"/tmp/repo","message":{"role":"assistant","content":[{"type":"text","text":"no timestamp"}]}}"#,
                "\n",
                "\n",
                r#"{"type":"assistant","timestamp":"2026-08-19T07:31:00.000Z","cwd":"/tmp/repo","message":{"role":"assistant","content":[{"type":"text","text":"after"}]}}"#,
                "\n",
                r#"{"type":"user","timestamp":"2026-08-19T07:32:00.000Z","cwd":"/tmp/re"#,
            ),
        )
        .unwrap();

        // Empty file (§5: zero observed, tolerated anyway).
        fs::write(proj.join("44444444-2222-3333-4444-555555555555.jsonl"), "").unwrap();

        d
    }

    fn write(path: &Path, lines: &[&str]) {
        fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();
    }

    fn project(d: &TempDir) -> Vec<PathBuf> {
        vec![d.path().join("-tmp-repo")]
    }

    fn session(d: &TempDir, name: &str) -> PathBuf {
        d.path().join("-tmp-repo").join(name)
    }

    #[test]
    fn roots_is_the_projects_directory_whether_or_not_it_exists() {
        let root = projects_root();
        assert_eq!(ClaudeCode.roots(), vec![root.clone()]);
        assert!(root.ends_with(".claude/projects"), "{}", root.display());
    }

    /// The block table in the module doc, asserted.
    #[test]
    fn turn_text_renders_every_block_type_the_notes_found() {
        let d = fixture();
        let turns = ClaudeCode.read(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl"));

        let texts: Vec<&str> = turns.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                // A bare string is the human's prompt, verbatim and untagged.
                "why did you delete it",
                // thinking first and in full, then text, then the tool call.
                concat!(
                    "[thinking]\nthe file was generated\n",
                    "It was generated.\n",
                    "[tool_use Bash]\n{\"command\":\"rm -f gen.txt\"}"
                ),
                // tool_result recursing into an array of blocks.
                "[tool_result]\nok\n[image]\n[tool_reference]",
                "[system:away_summary]\nhook ran",
                "switched model mid-session",
                // An error the model did not write is still a turn: it is the
                // reason the run stopped. Only its `model` is disowned (§11.2).
                "API Error: 429",
                // An unknown block type is a marker, not a crash.
                "resumed from yesterday\n[fallback]",
            ]
        );
        assert_eq!(
            turns.iter().map(|t| t.role.clone()).collect::<Vec<_>>(),
            [
                Role::User,
                Role::Assistant,
                Role::User,
                Role::System,
                Role::Assistant,
                Role::Assistant,
                Role::Assistant
            ]
        );
        // 1-based line numbers into the real file: the attachment on line 4,
        // the contentless `system` on line 7 and the metadata kinds do not
        // shift them.
        assert_eq!(
            turns.iter().map(|t| t.line).collect::<Vec<_>>(),
            [1, 3, 5, 6, 8, 9, 10]
        );
    }

    /// Neither tally. Fifteen line kinds, an attachment and a kind that does
    /// not exist yet are structure, not loss.
    #[test]
    fn skipped_line_kinds_are_not_parse_failures() {
        let d = fixture();
        let s = scan(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(s.meta.parse_failures, 0, "nothing in that file is damaged");
        assert_eq!(s.meta.declined, 0, "nor did we refuse to read anything");
        assert_eq!(
            s.meta.turn_count, 7,
            "attachment, telemetry `system` and metadata kinds are not turns"
        );
    }

    /// §6.1: the last line is 12 hours before the first, and the span says so.
    #[test]
    fn span_is_min_and_max_not_first_and_last() {
        let d = fixture();
        let s = scan(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(
            s.meta.first_ts.map(|t| t.to_rfc3339()),
            Some("2026-08-18T19:00:00+00:00".to_string()),
            "the inverted line is the earliest turn"
        );
        assert_eq!(
            s.meta.last_ts.map(|t| t.to_rfc3339()),
            Some("2026-08-19T07:04:00+00:00".to_string())
        );
    }

    /// §4.2 and the rule this module picked: most frequent wins, and the whole
    /// set is surfaced because one value cannot answer `sessions --repo`.
    #[test]
    fn multi_cwd_file_resolves_to_the_most_frequent_and_keeps_the_set() {
        let d = fixture();
        let s = scan(&session(&d, "22222222-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(s.meta.cwd, Some(PathBuf::from("/tmp/repo/b")), "3 beats 2");
        assert_eq!(
            s.identity.cwds,
            [
                PathBuf::from("/tmp/repo/b"),
                // `a` appears twice and `c` once, so `a` is second on count —
                // and it would have been FIRST under a first-seen rule, which
                // is exactly the rule this module did not choose.
                PathBuf::from("/tmp/repo/a"),
                PathBuf::from("/tmp/repo/c"),
            ]
        );
        // Two of b's three lines are an attachment and a contentless `system`:
        // a cwd counts even when its line never becomes a turn (§4.1).
        assert_eq!(s.meta.turn_count, 4);
    }

    /// The spec's tolerance contract, on damage nature did not supply.
    #[test]
    fn damage_is_counted_and_never_fatal() {
        let d = fixture();
        let s = scan(&session(&d, "33333333-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(
            ClaudeCode
                .read(&s.meta.path)
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>(),
            ["before", "after"],
            "the good turns on both sides of the damage survive"
        );
        // Garbage, a non-object line, a testimony line with no timestamp, and
        // the truncated tail. The blank line is free.
        assert_eq!(s.meta.parse_failures, 4);
        assert_eq!(s.meta.declined, 0, "damage is not a policy skip");
        assert_eq!(s.meta.turn_count, 2);
    }

    #[test]
    fn an_empty_file_is_a_session_with_nothing_in_it() {
        let d = fixture();
        let s = scan(&session(&d, "44444444-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(s.meta.turn_count, 0);
        assert_eq!(s.meta.parse_failures, 0);
        assert_eq!(s.meta.first_ts, None);
        assert_eq!(s.meta.cwd, None);
        assert_eq!(s.identity, Identity::default());
        // A file that does not exist is the same answer, not a panic.
        let s = scan(&session(&d, "nope.jsonl"));
        assert_eq!(s.meta.turn_count, 0);
        assert_eq!(s.meta.parse_failures, 0);
    }

    /// §3.1 and §3.4: the subagent layout, and the identity signals the join
    /// needs because `cwd` cannot tell two siblings apart (§4.3).
    #[test]
    fn subagent_surfaces_agent_id_and_its_sidecar_description() {
        let d = fixture();
        let path = d
            .path()
            .join("-tmp-repo/11111111-2222-3333-4444-555555555555/subagents/agent-a34d3106441943c34.jsonl");
        let s = scan(&path);

        // agentId equals the filename hex in 604 of 604 files.
        assert_eq!(s.identity.agent_id.as_deref(), Some("a34d3106441943c34"));
        // ...while sessionId is the PARENT's, shared with the top-level file.
        assert_eq!(
            s.identity.session_id,
            scan(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl"))
                .identity
                .session_id,
            "keying on sessionId would merge a session with its subagents"
        );
        let side = s.identity.sidecar.clone().expect("sidecar exists");
        assert_eq!(
            side.description.as_deref(),
            Some("W0 empirical format-notes bead")
        );
        assert_eq!(side.agent_type.as_deref(), Some("general-purpose"));
        assert_eq!(side.spawn_depth, Some(1));
        // Unknown sidecar keys (`model`, `stoppedByUser`) were ignored, not fatal.
        assert_eq!(sidecar(&path), s.identity.sidecar);
        // No sidecar for a top-level session, and that is not an error.
        assert_eq!(
            sidecar(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl")),
            None
        );
    }

    /// **The bead.** A line we declined is not a line that was damaged.
    ///
    /// One oversized line and one genuinely malformed line in the same file
    /// must come back as `declined=1, damaged=1` — never as 2 of either. On
    /// the machine recount was written on the real answer is 12 and 0 (§5.1),
    /// and `doctor` reported it as 12 lines of damage in transcripts that are
    /// byte-perfect. This fixture is that bug, small enough to run.
    #[test]
    fn an_oversized_line_is_declined_a_broken_one_is_damaged() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("huge.jsonl");
        let good = r#"{"type":"user","timestamp":"2026-08-19T07:00:00.000Z","cwd":"/tmp/repo","message":{"role":"user","content":"survivor"}}"#;
        // §5.1: not a binary dump — the human's own prompt, self-escaped until
        // it would not fit. The file around it is perfectly good JSON.
        let huge = format!(
            r#"{{"type":"user","timestamp":"2026-08-19T07:01:00.000Z","cwd":"/tmp/repo","message":{{"role":"user","content":"{}"}}}}"#,
            "x".repeat(MAX_LINE_BYTES + 1024)
        );
        let broken = r#"{"type":"user","timestamp":"2026-08-19T07:02:0"#;
        fs::write(&path, format!("{huge}\n{good}\n{broken}\n")).unwrap();

        let s = scan(&path);
        assert_eq!(
            s.meta.declined, 1,
            "the oversized line: we refused to read it"
        );
        assert_eq!(
            s.meta.parse_failures, 1,
            "the truncated line: it is damaged"
        );
        assert_eq!(s.meta.turn_count, 1);
        assert_eq!(ClaudeCode.read(&path)[0].text, "survivor");
        assert_eq!(
            ClaudeCode.read(&path)[0].line,
            2,
            "the oversized line still occupies its line number"
        );

        // And two oversized lines are two declines, not a rounding of "some".
        fs::write(&path, format!("{huge}\n{good}\n{huge}")).unwrap();
        let s = scan(&path);
        assert_eq!((s.meta.declined, s.meta.parse_failures), (2, 0));
    }

    /// Both layouts arrive, `journal.jsonl` does not, and the mtime window is
    /// honoured. §0.1 and §2.4.
    #[test]
    fn discover_finds_both_layouts_and_refuses_the_journal() {
        let d = fixture();
        let found = ClaudeCode.discover(&project(&d), &TimeWindow::default());
        let names: Vec<String> = found
            .iter()
            .map(|m| m.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert!(names.contains(&"agent-a34d3106441943c34.jsonl".to_string()));
        assert!(
            names.contains(&"agent-b0000000000000001.jsonl".to_string()),
            "§11: a workflow-nested subagent is still a transcript"
        );
        assert!(names.contains(&"11111111-2222-3333-4444-555555555555.jsonl".to_string()));
        assert!(
            !names.contains(&"journal.jsonl".to_string()),
            "§2.4: not a transcript, and it would be a permanent no-cwd report"
        );
        assert!(
            found.iter().all(|m| m.connector == "claude_code"),
            "every session says who read it"
        );
        // The empty file survives discovery: its span is unbounded, and
        // `doctor` wants to see it.
        assert_eq!(found.len(), 6, "{names:?}");

        // A `since` past every file's mtime prunes the whole tree unopened —
        // the structural half of the spec's cheapness argument, exercised
        // through this connector rather than only in `prune`.
        let later = TimeWindow {
            since: crate::model::parse_ts("2099-01-01T00:00:00Z").ok(),
            until: None,
        };
        assert!(ClaudeCode.discover(&project(&d), &later).is_empty());

        // A window inside the corpus: mtime cannot prune (every fixture was
        // written just now), and `discover` does NOT prune on the parsed span
        // either — recognising a file is this connector's claim on it, and a
        // claim released here is a claim `generic_jsonl` picks up. The span
        // filter belongs to `discover_all` and is tested there.
        let inside = TimeWindow {
            since: crate::model::parse_ts("2026-08-19T07:19:00Z").ok(),
            until: crate::model::parse_ts("2026-08-19T07:26:00Z").ok(),
        };
        let claimed: Vec<String> = ClaudeCode
            .discover(&project(&d), &inside)
            .iter()
            .map(|m| m.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(claimed.contains(&"22222222-2222-3333-4444-555555555555.jsonl".to_string()));
        assert!(
            claimed.contains(&"11111111-2222-3333-4444-555555555555.jsonl".to_string()),
            "out of window is still recognised, and so still claimed: {claimed:?}"
        );
        // …and the window then rules it out, once, centrally.
        let kept: Vec<String> =
            crate::connector::discover_all(&crate::connector::registry(), &project(&d), &inside)
                .iter()
                .map(|m| m.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
        assert!(kept.contains(&"22222222-2222-3333-4444-555555555555.jsonl".to_string()));
        assert!(!kept.contains(&"11111111-2222-3333-4444-555555555555.jsonl".to_string()));

        // Nothing anywhere near the tree is not a panic.
        assert!(ClaudeCode
            .discover(&[d.path().join("nope")], &TimeWindow::default())
            .is_empty());
        assert!(ClaudeCode.discover(&[], &TimeWindow::default()).is_empty());
    }

    const SESSION: &str = "11111111-2222-3333-4444-555555555555";

    /// §11: one join, one stat, three answers — and the ids never escape the
    /// directory they are joined into.
    #[test]
    fn keyed_resolution_tells_found_from_absent_from_nonsense() {
        let d = fixture();
        let proj = d.path().join("-tmp-repo");

        // Resolved: the subagent transcript §10's recipe would have minted an
        // id for.
        assert_eq!(
            keyed(&proj, SESSION, Some("a34d3106441943c34")),
            Keyed::Found(proj.join(format!("{SESSION}/subagents/agent-a34d3106441943c34.jsonl")))
        );
        // §10.4's guard leaves `harness_subagent` unset when the match is not
        // unique, so an event with only a session is the designed case, not an
        // edge one: it names the parent transcript, whose stem is its
        // `sessionId` in 238 of 238 files (§3.1).
        assert_eq!(
            keyed(&proj, SESSION, None),
            Keyed::Found(proj.join(format!("{SESSION}.jsonl")))
        );

        // Absent: well-formed, correctly joined, not on disk. Pruned (§0.0) or
        // another machine's ledger. The path comes back because that is what a
        // human needs told.
        assert_eq!(
            keyed(&proj, SESSION, Some("aaaaaaaaaaaaaaaaa")),
            Keyed::Absent(proj.join(format!("{SESSION}/subagents/agent-aaaaaaaaaaaaaaaaa.jsonl")))
        );
        assert!(matches!(
            keyed(&proj, "99999999-9999-9999-9999-999999999999", None),
            Keyed::Absent(_)
        ));
        // A directory is not a transcript, and neither is a project directory
        // that does not exist at all.
        assert!(matches!(
            keyed(&proj, SESSION, Some("")),
            Keyed::Malformed(_)
        ));
        assert!(matches!(
            keyed(&d.path().join("-tmp-nope"), SESSION, None),
            Keyed::Absent(_)
        ));
        // §11: the workflow-nested transcript exists, `discover` reads it, and
        // the keyed tier still cannot name it — the `wf_` id is in no event.
        // A documented Absent, not a silent wrong answer.
        assert!(matches!(
            keyed(&proj, SESSION, Some("b0000000000000001")),
            Keyed::Absent(_)
        ));

        // Malformed: nonsense, not absence. Nothing is joined and nothing is
        // stat'd — every one of these would otherwise be a path component.
        for bad in [
            "",
            "..",
            "../../etc/passwd",
            "a/b",
            "a\\b",
            ".",
            "agent.jsonl",
            "a b",
            "id\0",
            "/absolute",
            "~",
            "$SESSION",
        ] {
            assert!(
                matches!(keyed(&proj, bad, None), Keyed::Malformed(_)),
                "session {bad:?} must be malformed"
            );
            assert!(
                matches!(keyed(&proj, SESSION, Some(bad)), Keyed::Malformed(_)),
                "subagent {bad:?} must be malformed"
            );
        }
        // …and the reason names the field, because a ledger with a bad id in
        // it is a bug in whoever wrote the ledger.
        let Keyed::Malformed(why) = keyed(&proj, SESSION, Some("../x")) else {
            panic!("traversal must not resolve");
        };
        assert!(why.contains("harness_subagent"), "{why}");

        // A dash-only id is legal in shape and simply absent — the shape check
        // exists to contain the join, not to guess Anthropic's id grammar.
        assert!(matches!(keyed(&proj, "-", Some("-")), Keyed::Absent(_)));
    }

    /// §11.1: the model is recorded per turn, it changes within one file, and
    /// §11.2's `<synthetic>` is not one.
    #[test]
    fn models_are_a_set_per_file_and_synthetic_is_not_a_model() {
        let d = fixture();
        let s = scan(&session(&d, "11111111-2222-3333-4444-555555555555.jsonl"));
        assert_eq!(
            s.identity.models,
            ["claude-opus-5", "claude-sonnet-5"],
            "two lines of opus beat one of sonnet, and sonnet came first"
        );
        assert!(
            !s.identity.models.iter().any(|m| m == "<synthetic>"),
            "an API-error record would be a fabricated discrepancy"
        );
        // A subagent records it too, which is the case the comparison needs.
        let sub = scan(&d.path().join(format!(
            "-tmp-repo/{SESSION}/subagents/agent-a34d3106441943c34.jsonl"
        )));
        assert_eq!(sub.identity.models, ["claude-opus-5"]);
        // Nothing to compare is silence, never an error: a file with no
        // assistant line has no model and that is not a finding.
        assert!(
            scan(&session(&d, "22222222-2222-3333-4444-555555555555.jsonl"))
                .identity
                .models
                .is_empty()
        );
        assert!(
            scan(&session(&d, "44444444-2222-3333-4444-555555555555.jsonl"))
                .identity
                .models
                .is_empty()
        );
    }

    /// The claim is that this resolution is exact on the corpus it was measured
    /// on, so it is checked there: every flat subagent transcript on this
    /// machine must be re-namable from the two ids alone. Read-only, and a
    /// **skip** where `~/.claude/projects` is absent.
    #[test]
    fn keyed_resolution_round_trips_the_real_corpus() {
        let root = projects_root();
        if !root.is_dir() {
            return;
        }
        let mut files: Vec<PathBuf> = candidates(&[root])
            .into_iter()
            // The flat layout only: §11 says the nested 371 are out of scope
            // and this test does not get to pretend otherwise.
            .filter(|p| p.parent().is_some_and(|d| d.ends_with("subagents")))
            .collect();
        if files.is_empty() {
            return;
        }
        files.sort_by_key(|p| std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok()));
        files.truncate(40);

        let mut with_model = 0usize;
        for path in &files {
            let session_dir = path.parent().and_then(|p| p.parent()).unwrap();
            let project_dir = session_dir.parent().unwrap();
            let session = session_dir.file_name().unwrap().to_string_lossy();
            // §10.5: the id is read out of `agentId`, never parsed off the
            // filename — which is exactly what a pact event carries.
            let s = scan(path);
            let Some(agent) = s.identity.agent_id.as_deref() else {
                continue;
            };
            assert_eq!(
                keyed(project_dir, &session, Some(agent)),
                Keyed::Found(path.clone()),
                "the two ids must re-name {}",
                path.display()
            );
            // The parent session file the same event's `harness_session`
            // names, when it has not been pruned out from under its children.
            assert!(
                !matches!(keyed(project_dir, &session, None), Keyed::Malformed(_)),
                "a real session id is not malformed: {session}"
            );
            if !s.identity.models.is_empty() {
                with_model += 1;
                for m in &s.identity.models {
                    assert!(
                        m.starts_with("claude-"),
                        "§11.1 says a recorded model is a bare slug, got {m:?} in {}",
                        path.display()
                    );
                }
            }
        }
        assert!(
            with_model > 0,
            "§11.1: `message.model` is on 100% of assistant lines — 40 real \
             subagent transcripts and not one model recorded means it moved"
        );
    }

    /// The notes were measured on this machine, so the connector is checked
    /// against the same corpus — read-only, and a **skip** (never a failure)
    /// where `~/.claude/projects` is absent, because CI has no transcripts.
    #[test]
    fn reads_the_real_corpus_on_this_machine() {
        let root = projects_root();
        if !root.is_dir() {
            return;
        }
        let mut files = candidates(&[root]);
        files.retain(|p| !is_journal(p));
        if files.is_empty() {
            return;
        }

        // Newest first, so the sample is current-format rather than whatever
        // sorted first, and bounded so the test stays a test.
        files.sort_by_key(|p| std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok()));
        files.truncate(40);

        let mut turns_seen = 0usize;
        let mut thinking_seen = 0usize;
        let mut with_cwd = 0usize;
        let mut multi_cwd = 0usize;
        let mut sidecars = 0usize;
        for path in &files {
            let s = scan(path);
            assert_eq!(s.meta.path, *path);
            if let (Some(a), Some(b)) = (s.meta.first_ts, s.meta.last_ts) {
                assert!(a <= b, "min/max inverted in {}", path.display());
            }
            assert_eq!(s.meta.turn_count, ClaudeCode.read(path).len());
            if s.meta.cwd.is_some() {
                with_cwd += 1;
                assert_eq!(
                    s.meta.cwd.as_deref(),
                    s.identity.cwds.first().map(|p| p.as_path()),
                    "the summary must be the head of the set"
                );
            }
            if s.identity.cwds.len() > 1 {
                multi_cwd += 1;
            }
            if s.identity.sidecar.is_some() {
                sidecars += 1;
                // §3.1: agentId equals the filename hex.
                let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
                if let Some(id) = &s.identity.agent_id {
                    assert_eq!(stem, format!("agent-{id}"), "{}", path.display());
                }
            }
            turns_seen += s.meta.turn_count;
            thinking_seen += ClaudeCode
                .read(path)
                .iter()
                .filter(|t| t.text.contains("[thinking]"))
                .count();
        }

        // The corpus is 87.9% timestamped testimony; a sample of 40 real files
        // that yields nothing means the parser stopped matching the format.
        assert!(turns_seen > 0, "40 real files and not one turn parsed");
        assert!(with_cwd > 0, "§4.1 says cwd is on 100% of testimony lines");
        // Not asserted, only reported by `cargo test -- --nocapture`: these
        // vary with what this machine did lately.
        println!(
            "real corpus: {} files, {turns_seen} turns, {thinking_seen} with thinking, \
             {with_cwd} with a cwd, {multi_cwd} multi-cwd, {sidecars} sidecars",
            files.len()
        );
    }
}
