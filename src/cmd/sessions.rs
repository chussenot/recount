//! `recount sessions --repo <p> [--since --until]`
//!
//! Spec: "sessions whose cwd resolves into the repo or its worktrees,
//! fleet-identity guesses attached."
//!
//! This is the first command to land, so the two shapes below are the house
//! style the other four follow. Both are decisions, not accidents.
//!
//! # Output: JSON on stdout, digest on stderr
//!
//! "all --json; human digest second" has to survive `recount sessions --json |
//! jq`, so the two outputs go to different streams:
//!
//! * **without `--json`** — the digest, on stdout, and nothing else.
//! * **with `--json`** — the JSON document on stdout, the same digest on
//!   stderr, printed second. A pipe gets clean JSON; a person watching the
//!   terminal still gets the readable version; `2>/dev/null` is not required
//!   but works.
//!
//! Neither stream is optional and neither is a summary of the other: the JSON
//! is the contract, the digest is the reading.
//!
//! # The JSON contract
//!
//! Audit scripts consume this, so it is versioned and it is stable. Every
//! command emits the same envelope — `command`, `schema`, then its payload —
//! and `schema` only ever moves when a field is removed or its meaning
//! changes. Added fields do not bump it; readers must ignore what they do not
//! know.
//!
//! ```text
//! { "command": "sessions", "schema": 2,
//!   "repo": "/abs/path",                          // canonical
//!   "window": { "since": <rfc3339|null>, "until": <rfc3339|null> },
//!   "session_dirs": ["/home/u/.claude/projects/-abs-path", ...],
//!   "discovered": 12,                             // before the cwd filter
//!   "excluded_by_cwd": 1,                         // said a cwd, outside the repo
//!   "sessions": [ {
//!       "path": "…/subagents/agent-<hex>.jsonl",
//!       "connector": "claude_code",
//!       "cwd": <path|null>,                    // the most frequent one (§4.2)
//!       "first_ts": <rfc3339|null>, "last_ts": <rfc3339|null>,
//!       "turn_count": 128, "parse_failures": 2, "declined": 12,
//!       "worktree": <name|null>,                  // null: the cwd did not say
//!       "repo_match": "cwd" | "session_dir",
//!       "fleet_identity": {
//!           "agent": <name|null>,                 // null: we cannot tell
//!           "tier": "keyed",       "confidence": null,     // the ledger said so
//!           "tier": "topological", "confidence": 0.90,     // ...or we inferred it
//!           "evidence": [ { "signal": "...", "detail": "..." }, ... ]
//!       } } ],
//!   "warnings": ["worktree `gone` is listed by git but its directory is gone"] }
//! ```
//!
//! # A guess is shaped like a guess, and a fact is not shaped like one
//!
//! Most of this output is inference and says so. Some of it is not, and that
//! difference is the only thing here worth getting right.
//!
//! **When the ledger names the transcript.** pact 0.11.0 stamps
//! `harness_subagent` on every event it writes: the id of the transcript the
//! acting agent was running in. A transcript whose id appears there is not
//! being guessed at — `.pact/events.jsonl` line N *states* that agent `X` ran
//! `agent-<id>.jsonl`, and the answer is [`Tier::Keyed`], evidenced by that
//! line number. No score, and the guess machinery below is not consulted at
//! all: this is [`join`](crate::join)'s two-tier rule, applied to the same
//! ledger from the transcript's side. The id comes from the filename stem,
//! which §3.1 measured equal to the file's own `agentId` in 604 of 604 files.
//!
//! **When it does not** — which is most of the corpus, because 0.10.7 wrote no
//! chain and no later pact can go back and key those rows — the two inference
//! signals below run exactly as they always have and the answer stays
//! `topological` with its number.
//!
//! **Why the key was renamed, and the schema bumped.** `fleet_identity_guess`
//! was an honest name while everything under it was a guess; carrying a stated
//! fact under a key called `guess` is the same category error inverted, and
//! this file's whole design is that the shape of the answer tells you what kind
//! of answer it is. So the key is `fleet_identity`, the tier says how it was
//! reached, and `schema` moves 1 → 2 — which is precisely what the envelope's
//! version exists for.
//!
//! The alternative — keep the old key for inferred rows and add a second one
//! for keyed rows — was rejected: an existing `jq '.fleet_identity_guess'`
//! reader would go on working and silently drop exactly the rows whose identity
//! is certain, which is a biased sample of the audit and the worst of the three
//! failure modes. Renaming breaks every row the same way, at a version number a
//! reader can test. Inside, `confidence` follows [`Tier`]'s own rule: written,
//! and explicitly `null` for a keyed answer, so a script thresholding it fails
//! the comparison instead of quietly ranking an identity against a guess.
//!
//! One thing the rename does not change: `agent` is still nullable, `evidence`
//! is still never empty, and when `agent` is null the evidence says *why*.
//!
//! The two inference signals are the ones this tree can actually support, and
//! `docs/format-notes.md` §4.3 measured both:
//!
//! 1. **Topology** — the session's `cwd` resolved through
//!    [`worktree::for_cwd`] to a linked worktree, whose name is the pact
//!    `invoked_from`. Correct when it fires, and §4.3 measured that for a pact
//!    fleet it usually will not: `cwd` is the Claude Code *process's*
//!    directory, and a pact agent enters its worktree with `cd` inside a Bash
//!    call, so every agent of one run reports the repo root.
//! 2. **Transcript mention** — the literal `PACT_AGENT=<name>` assignment in
//!    the file's bytes. §4.3 again: for pact worktrees this is not a
//!    tiebreaker, it is the only signal that separates sibling agents. It is a
//!    substring scan rather than a parse, because the name reads the same in a
//!    JSON string as in a raw line and this must work for a connector whose
//!    reader has its own opinions. Two different names found means *no* guess
//!    — an agent quoting another agent's assignment is not that agent.
//!
//! A better signal slots in without changing the shape: a connector that
//! surfaces the `.meta.json` `description` sidecar (§3.4) adds one
//! [`Evidence`] to the same vector and one arm to the arithmetic in
//! [`guess_identity`]. The output type does not move.
//!
//! The digest says the same three things in words: "`X` — KNOWN, the ledger
//! names this transcript", "probably `X` — a GUESS (confidence 0.90)", or
//! "unknown — no guess".
//!
//! # One session, one connector
//!
//! Discovery goes through [`connector::discover_all`], not over the registry
//! directly: both connectors accept `*.jsonl`, so running them both over one
//! set of directories would list every transcript twice — the second time as
//! the fallback's "0 turns, N lines declined". `connector`'s module doc holds
//! the ownership rule; `discovered` counts sessions, not readings of them.
//!
//! # No sessions is an answer
//!
//! A repo with no transcripts prints an empty list and exits 0. Not an error,
//! not a warning wearing an error's clothes: recount runs from repos that have
//! never seen a coding agent, and "none" is the honest result.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::cli::SessionsArgs;
use crate::cmd::EXIT_OK;
use crate::connector::{self, Connector};
use crate::model::{Confidence, Evidence, SessionMeta, Signal, Tier, TimeWindow};
use crate::pact_events::{self, Events};
use crate::worktree::{self, Worktree};

/// Bumped only when a field leaves or changes meaning. See the module doc.
///
/// 2: `fleet_identity_guess` became `fleet_identity` and its `confidence`
/// became the flattened [`Tier`] — a field left and a field changed meaning,
/// which is both halves of the rule at once.
const SCHEMA: u32 = 2;

pub fn run(args: &SessionsArgs, json: bool) -> Result<u8> {
    let window = args.window.window();
    // The spec's performance clause, in two lines and before any file opens:
    // `--repo` becomes the encoded directories, and the ones that are not
    // there are dropped. A directory that does not exist has nothing in it, so
    // a repo no agent has ever run in reaches no connector at all — at a cost
    // of one `stat` per worktree.
    let worktrees = worktree::worktrees(&args.repo)?;
    let dirs: Vec<PathBuf> = worktree::session_dirs(&args.repo)?
        .into_iter()
        .filter(|d| d.is_dir())
        .collect();
    let repo = worktrees
        .first()
        .map_or_else(|| args.repo.clone(), |w| w.path.clone());

    let report = collect(&connector::registry(), &repo, &worktrees, &dirs, &window);
    emit(
        &report,
        json,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )?;
    Ok(EXIT_OK)
}

/// How a session was tied to the repo. Both are reported rather than one being
/// silently treated as the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoMatch {
    /// The session stated a `cwd` and it resolved into the repo or a worktree.
    Cwd,
    /// The session stated no `cwd`; it is here because it is filed under one
    /// of this repo's encoded session directories. Weaker — the encoding is
    /// not injective (`worktree`'s module doc) — so it is labelled, not hidden.
    SessionDir,
}

impl RepoMatch {
    fn label(self) -> &'static str {
        match self {
            RepoMatch::Cwd => "cwd",
            RepoMatch::SessionDir => "session dir",
        }
    }
}

/// Which fleet agent ran a session, and on what footing. Read the module doc:
/// [`Self::tier`] is the field that says whether this is a record or an
/// inference, and it is not optional to look at.
#[derive(Debug, Clone, Serialize)]
pub struct FleetIdentity {
    /// The identity, or `None` when we cannot tell. Never `None` on a keyed
    /// answer — the ledger named it.
    pub agent: Option<String>,
    /// `keyed` with a `null` confidence, or `topological` with the score.
    /// Flattened, so the wire shape is the two fields [`Tier`] documents.
    #[serde(flatten)]
    pub tier: Tier,
    /// Never empty. When `agent` is `None` this is why.
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionEntry {
    pub path: PathBuf,
    pub connector: String,
    pub cwd: Option<PathBuf>,
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub turn_count: usize,
    /// Lines the connector tried to parse and could not. Damage.
    pub parse_failures: usize,
    /// Lines the connector refused to read on purpose. Not damage — see
    /// [`SessionMeta`], where the two were one field until it made `doctor`
    /// lie.
    pub declined: usize,
    /// The checkout the `cwd` resolved into, `None` when the cwd did not say.
    pub worktree: Option<String>,
    pub repo_match: RepoMatch,
    pub fleet_identity: FleetIdentity,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub command: &'static str,
    pub schema: u32,
    pub repo: PathBuf,
    pub window: TimeWindow,
    pub session_dirs: Vec<PathBuf>,
    /// Sessions the connectors returned, before the cwd filter.
    pub discovered: usize,
    /// Sessions dropped because they named a cwd outside the repo.
    pub excluded_by_cwd: usize,
    pub sessions: Vec<SessionEntry>,
    pub warnings: Vec<String>,
}

/// Everything after the resolution: discover, filter to the repo, guess.
///
/// `dirs` and `worktrees` are arguments rather than rediscovered here so the
/// resolution happens once, in [`run`], before anything opens a file — and so
/// this is testable without a `$HOME`.
fn collect(
    connectors: &[Box<dyn Connector>],
    repo: &Path,
    worktrees: &[Worktree],
    dirs: &[PathBuf],
    window: &TimeWindow,
) -> Report {
    // No directory to look in is a question with a known answer, so no
    // connector is asked it.
    let discovered: Vec<SessionMeta> = if dirs.is_empty() {
        Vec::new()
    } else {
        // Not `flat_map` over the registry: both connectors accept `*.jsonl`,
        // so that discovers every file once as itself and once as the
        // fallback's garbage. `discover_all` is where the ownership rule lives.
        connector::discover_all(connectors, dirs, window)
    };

    // The ledger, once, before the loop: 216 lines on the repo this was
    // written in, and every session is asked the same question of it.
    let mut warnings = Vec::new();
    // A `.pact/` that exists and cannot be read is not fatal — every identity
    // below simply falls back to the guess — but silence here would look
    // exactly like a fleet that never keyed anything.
    let events = pact_events::read(&pact_events::events_path(repo)).unwrap_or_else(|e| {
        warnings.push(format!(
            "could not read {}: {e:#}. Every identity below is a guess, including any the \
             ledger could have stated outright",
            pact_events::events_path(repo).display()
        ));
        Events::default()
    });
    let named = ledger_identities(&events);

    let mut excluded_by_cwd = 0;
    let mut sessions: Vec<SessionEntry> = Vec::new();
    for meta in &discovered {
        let Some((repo_match, worktree, matched_cwd)) = tie_to_repo(worktrees, meta) else {
            excluded_by_cwd += 1;
            continue;
        };
        sessions.push(SessionEntry {
            fleet_identity: identify(meta, &named, worktrees, worktree, matched_cwd.as_deref()),
            path: meta.path.clone(),
            connector: meta.connector.clone(),
            cwd: meta.cwd.clone(),
            first_ts: meta.first_ts,
            last_ts: meta.last_ts,
            turn_count: meta.turn_count,
            parse_failures: meta.parse_failures,
            declined: meta.declined,
            worktree: worktree.map(|w| w.name.clone()),
            repo_match,
        });
    }
    // Oldest first, ties broken by path. Sessions with no parsed timestamp
    // sort first (`None < Some`), which is where a reader wants them: an
    // unstamped transcript is the one worth looking at.
    sessions.sort_by(|a, b| {
        a.first_ts
            .cmp(&b.first_ts)
            .then_with(|| a.path.cmp(&b.path))
    });

    Report {
        command: "sessions",
        schema: SCHEMA,
        repo: repo.to_path_buf(),
        window: *window,
        session_dirs: dirs.to_vec(),
        discovered: discovered.len(),
        excluded_by_cwd,
        sessions,
        warnings: {
            warnings.extend(worktrees.iter().filter(|w| !w.alive).map(|w| {
                format!(
                    "worktree `{}` is listed by git but {} is gone from disk; its transcripts outlive it",
                    w.name,
                    w.path.display()
                )
            }));
            warnings
        },
    }
}

/// What the repo's ledger STATES about the transcripts it names, keyed by
/// `harness_subagent`.
///
/// Only `harness_subagent` gates this, never `harness.is_some()` as a proxy:
/// a top-level agent's 0.11.0 row carries the harness, the session, the model
/// and the branch and names no transcript, because it is not a subagent of
/// anything. Every orchestrator row in this repo's ledger looks like that.
///
/// The agents are a set, not the first name seen. One transcript ran one
/// agent, so a second name for the same id is a ledger that contradicts
/// itself, and [`identify`] must not read a contradiction as a fact.
fn ledger_identities(events: &Events) -> BTreeMap<&str, Named<'_>> {
    let mut out: BTreeMap<&str, Named> = BTreeMap::new();
    for e in &events.events {
        let Some(id) = e.harness_subagent.as_deref() else {
            continue;
        };
        let named = out.entry(id).or_insert(Named {
            agents: BTreeSet::new(),
            line: e.line,
        });
        named.agents.insert(e.agent.as_str());
    }
    out
}

/// One transcript id, as the ledger describes it.
struct Named<'a> {
    /// Every distinct `agent` that wrote an event from this transcript. One,
    /// unless the ledger contradicts itself.
    agents: BTreeSet<&'a str>,
    /// The first line that named it — the one a human is sent to read.
    line: usize,
}

/// Does this session belong to the repo, and to which checkout?
///
/// `None` means exclude. The spec filters on "sessions whose cwd resolves into
/// the repo or its worktrees", and the three cases are not the same:
///
/// * a cwd that resolves — kept, [`RepoMatch::Cwd`], worktree named;
/// * a cwd that resolves *elsewhere* — dropped. It is a real contradiction:
///   the file sits in this repo's encoded directory but says it ran somewhere
///   else, which the lossy encoding makes possible (`worktree`'s module doc)
///   and `docs/format-notes.md` §4.2 measured happening;
/// * **no cwd at all** — kept, [`RepoMatch::SessionDir`]. `generic_jsonl` never
///   states one and `journal.jsonl` has none, and the encoded directory is
///   still real evidence. Dropping these would turn "the file does not say"
///   into "no sessions found", which is the misdiagnosis this tool exists to
///   prevent. The weaker basis is labelled instead.
///
/// # A cwd is a set, so this matches on any of them
///
/// [`SessionMeta::cwd`] holds one path, and `claude_code` fills it
/// most-frequent-wins — a summary, not the evidence. `docs/format-notes.md`
/// §4.2 measured 46 of 871 real files stating more than one cwd and one
/// stating nine, so a session that did some of its work here and more of it
/// elsewhere would be excluded on a summary that named the elsewhere. The rule
/// is **any** cwd resolving, so when the summary does not resolve the whole set
/// is asked for ([`all_cwds`]) before the session is dropped. The third element
/// of the return is the cwd that actually resolved, which is what the topology
/// evidence must quote — the summary would name a directory in another repo.
fn tie_to_repo<'a>(
    worktrees: &'a [Worktree],
    meta: &SessionMeta,
) -> Option<(RepoMatch, Option<&'a Worktree>, Option<PathBuf>)> {
    let Some(cwd) = meta.cwd.clone() else {
        return Some((RepoMatch::SessionDir, None, None));
    };
    if let Some(w) = worktree::for_cwd(worktrees, &cwd) {
        return Some((RepoMatch::Cwd, Some(w), Some(cwd)));
    }
    all_cwds(meta).into_iter().find_map(|cwd| {
        worktree::for_cwd(worktrees, &cwd).map(|w| (RepoMatch::Cwd, Some(w), Some(cwd)))
    })
}

/// Every cwd the file states, most frequent first, for the connector that can
/// say. Empty for one that cannot — `generic_jsonl` states none at all, by
/// design, so there is nothing to ask it.
///
/// This re-reads the transcript, which is why it is only reached for a session
/// whose summary cwd already failed to resolve: the cost falls on the sessions
/// that were about to be excluded, not on every session. Should that ever
/// matter, the fix is a `SessionMeta` that carries the set — which is the
/// change this function exists to avoid making across three other beads' files.
fn all_cwds(meta: &SessionMeta) -> Vec<PathBuf> {
    if meta.connector == connector::claude_code::ClaudeCode.name() {
        connector::claude_code::scan(&meta.path).identity.cwds
    } else {
        Vec::new()
    }
}

/// The identity of the agent that ran this session: stated, if the ledger
/// states it, and inferred otherwise.
///
/// The two tiers do not mix. A transcript the ledger names gets one
/// [`Signal::Keyed`] evidence line and nothing else — the same rule
/// [`crate::join`] applies from the event's side, for the same reason: there
/// is no candidate set and no tie to break, so weighing topology and a byte
/// scan beside a statement would invite a reader to compare them. It is also
/// the cheaper path, since the keyed answer never opens the transcript.
///
/// A ledger that names two different agents for one transcript is not a
/// statement of identity. Those fall through to the guess, carrying one
/// [`Signal::Keyed`] line that says the ledger contradicted itself — because
/// "the ledger disagrees with the ledger" is news, and demoting it to silence
/// would present the inference as if nothing else had been tried.
fn identify(
    meta: &SessionMeta,
    named: &BTreeMap<&str, Named<'_>>,
    worktrees: &[Worktree],
    worktree: Option<&Worktree>,
    cwd: Option<&Path>,
) -> FleetIdentity {
    let found = transcript_id(&meta.path).and_then(|id| named.get(id).map(|n| (id, n)));
    match found {
        Some((id, n)) if n.agents.len() == 1 => {
            let agent = n.agents.iter().next().expect("len 1");
            FleetIdentity {
                agent: Some((*agent).to_string()),
                tier: Tier::Keyed,
                evidence: vec![Evidence::new(
                    Signal::Keyed,
                    format!(
                        "`.pact/events.jsonl` line {} states that agent `{agent}` acted from \
                         harness_subagent `{id}`, which is this file: agent-{id}.jsonl. The \
                         ledger names the transcript, so this identity is not inferred and \
                         carries no confidence — read the line and see for yourself",
                        n.line
                    ),
                )],
            }
        }
        contradicted => {
            let mut guess = guess_identity(meta, worktrees, worktree, cwd);
            if let Some((id, n)) = contradicted {
                guess.evidence.insert(
                    0,
                    Evidence::new(
                        Signal::Keyed,
                        format!(
                        "`.pact/events.jsonl` names {} different agents ({}) for harness_subagent \
                         `{id}`, first at line {}. One transcript ran one agent, so this is the \
                         ledger contradicting itself and not a statement of identity; the guess \
                         below stands in for it, and the thing to look at is the ledger",
                        n.agents.len(),
                        n.agents.iter().copied().collect::<Vec<_>>().join(", "),
                        n.line
                    ),
                    ),
                );
            }
            guess
        }
    }
}

/// The transcript's own id: the `<id>` of `agent-<id>.jsonl`, which §3.1
/// measured equal to the file's `agentId` in 604 of 604 subagent files.
///
/// The filename, not a re-read of the file: this runs for every session and
/// the ledger's key is the same string either way. `None` for anything not
/// named that way — a top-level `<uuid>.jsonl`, or any other connector's file
/// — which is the same answer as "the ledger does not name it".
fn transcript_id(path: &Path) -> Option<&str> {
    path.file_stem()?.to_str()?.strip_prefix("agent-")
}

/// The literal `PACT_AGENT=` assignment, which is how a pact agent is told who
/// it is.
const PACT_AGENT: &str = "PACT_AGENT=";

/// Which fleet agent ran this session, when nothing STATED it? Read the module
/// doc first — this is an inference, it always returns
/// [`Tier::Topological`], and its caller is [`identify`], which tries the
/// ledger before it gets here.
///
/// Both signals always contribute exactly one [`Evidence`], including when
/// they name no one — which is what keeps `evidence` non-empty even for a
/// guess that came out empty, and why "unknown" is an answer with reasons
/// rather than a shrug.
///
/// `worktree` is the checkout the cwd resolved into and `cwd` is the cwd that
/// resolved into it — which is not always [`SessionMeta::cwd`], see
/// [`tie_to_repo`].
fn guess_identity(
    meta: &SessionMeta,
    worktrees: &[Worktree],
    worktree: Option<&Worktree>,
    cwd: Option<&Path>,
) -> FleetIdentity {
    let mut evidence = Vec::new();

    // A session that matched on a cwd other than its most frequent one would
    // otherwise read as a contradiction: the JSON's `cwd` field is the summary
    // and would name another repo entirely.
    let summary = match (&meta.cwd, cwd) {
        (Some(s), Some(c)) if s.as_path() != c => format!(
            " (the file's most frequent cwd is {} — §4.2: a session can state several)",
            s.display()
        ),
        _ => String::new(),
    };

    // Signal 1: topology. The main checkout is not an identity — every agent
    // of a pact run reports it (format-notes §4.3) — so only a linked worktree
    // names anyone.
    let main = worktrees.first().map(|w| w.path.as_path());
    let from_cwd = match (worktree, cwd) {
        (Some(w), Some(cwd)) if Some(w.path.as_path()) != main => {
            evidence.push(Evidence::new(
                Signal::Topology,
                format!("cwd {} is worktree `{}`{summary}", cwd.display(), w.name),
            ));
            Some(w.name.clone())
        }
        (Some(_), Some(cwd)) => {
            evidence.push(Evidence::new(
                Signal::Topology,
                format!(
                    "cwd {} is the repo root, which every agent of a pact run shares \
                     (docs/format-notes.md §4.3) — it names no one{summary}",
                    cwd.display()
                ),
            ));
            None
        }
        _ => {
            evidence.push(Evidence::new(
                Signal::Topology,
                "the session states no cwd, so topology names no one".to_string(),
            ));
            None
        }
    };

    // Signal 2: the transcript names an identity out loud.
    let named = pact_agent_names(&meta.path);
    let mentioned: Option<String> = match named.len() {
        0 => {
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!("no `{PACT_AGENT}` assignment appears in the transcript"),
            ));
            None
        }
        1 => {
            let name = named.iter().next().expect("len 1").clone();
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!("`{PACT_AGENT}{name}` appears in the transcript"),
            ));
            Some(name)
        }
        n => {
            evidence.push(Evidence::new(
                Signal::TranscriptMention,
                format!(
                    "the transcript names {n} identities ({}) — an agent quoting \
                     another agent's assignment is not that agent",
                    named.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            ));
            // One of them being the worktree we are standing in does break the
            // tie: the other names are being discussed, this one is where the
            // work happened.
            from_cwd.as_ref().filter(|w| named.contains(*w)).cloned()
        }
    };

    // The arithmetic, in one place. Both signals agreeing is the only thing
    // that gets near certainty; the mention alone outranks topology alone,
    // because §4.3 measured topology going quiet for exactly the fleet this
    // tool audits.
    let (agent, confidence) = match (&mentioned, &from_cwd) {
        // Ambiguity first: when the transcript named several identities the
        // worktree only broke a tie, which is not the same as two independent
        // signals agreeing, and must not be priced as if it were.
        (Some(_), Some(_)) if named.len() > 1 => (mentioned.clone(), 0.80),
        (Some(m), Some(w)) if m == w => (mentioned.clone(), 0.90),
        (Some(_), Some(_)) => (mentioned.clone(), 0.40),
        (Some(_), None) => (mentioned.clone(), 0.70),
        (None, Some(_)) => (from_cwd.clone(), 0.50),
        (None, None) => (None, 0.0),
    };
    FleetIdentity {
        agent,
        tier: Tier::Topological(Confidence::new(confidence)),
        evidence,
    }
}

/// Every distinct `<name>` in a `PACT_AGENT=<name>` assignment anywhere in the
/// file's bytes.
///
/// A substring scan, not a parse: see the module doc. An unreadable file names
/// no one, which is the same answer as a file that mentions no one — neither
/// is an error.
///
/// ponytail: this re-reads each surviving transcript, which `discover` has
/// just read and the page cache still holds. If a real corpus makes that
/// measurable, the fix is for connectors to surface the identity fields
/// format-notes §3.4 already lists (`attributionAgent`, the `.meta.json`
/// `description`) and for this scan to become the fallback — not a cache.
fn pact_agent_names(path: &Path) -> BTreeSet<String> {
    let Ok(bytes) = fs::read(path) else {
        return BTreeSet::new();
    };
    String::from_utf8_lossy(&bytes)
        .split(PACT_AGENT)
        .skip(1)
        .map(|tail| {
            tail.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// JSON to stdout, digest to stderr; or, without `--json`, digest to stdout.
/// See the module doc.
///
/// The sinks are arguments so the split itself can be asserted — it is the
/// house rule the other four commands inherit, and a rule nothing tests is a
/// suggestion.
fn emit(report: &Report, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> Result<()> {
    if json {
        let rendered = serde_json::to_string_pretty(report)?;
        // Write errors are ignored on purpose: `recount sessions --json | head`
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

    writeln!(w, "recount sessions — {}", r.repo.display())?;
    writeln!(
        w,
        "  window   since {} · until {}",
        stamp(r.window.since),
        stamp(r.window.until)
    )?;
    write!(
        w,
        "  scanned  {} session dir(s) · {} discovered",
        r.session_dirs.len(),
        r.discovered
    )?;
    if r.excluded_by_cwd > 0 {
        write!(
            w,
            " · {} excluded (cwd outside the repo)",
            r.excluded_by_cwd
        )?;
    }
    writeln!(w)?;

    if r.sessions.is_empty() {
        writeln!(
            w,
            "  no sessions — a normal answer, not a failure: recount often runs where \
             transcripts do not exist"
        )?;
    }

    for s in &r.sessions {
        writeln!(w, "\n  {}", s.path.display())?;
        let mut facts = vec![
            s.connector.clone(),
            format!(
                "{} turn{}",
                s.turn_count,
                if s.turn_count == 1 { "" } else { "s" }
            ),
        ];
        if s.declined > 0 {
            facts.push(format!("{} lines declined", s.declined));
        }
        if s.parse_failures > 0 {
            facts.push(format!("{} lines damaged", s.parse_failures));
        }
        facts.push(match (s.first_ts, s.last_ts) {
            (Some(a), Some(b)) => format!("{} → {}", stamp(Some(a)), stamp(Some(b))),
            _ => "no parsed timestamps".to_string(),
        });
        facts.push(format!("matched by {}", s.repo_match.label()));
        if let Some(name) = &s.worktree {
            facts.push(format!("worktree `{name}`"));
        }
        writeln!(w, "    {}", facts.join(" · "))?;

        // Three readings, and the first one is not a stronger version of the
        // second: "KNOWN" carries no number because there is none, and the
        // line of the ledger that says so is the evidence printed under it.
        let g = &s.fleet_identity;
        match (&g.agent, g.tier) {
            (Some(a), Tier::Keyed) => writeln!(
                w,
                "    fleet identity: `{a}` — KNOWN, the ledger names this transcript"
            )?,
            (Some(a), Tier::Topological(c)) => writeln!(
                w,
                "    fleet identity: probably `{a}` — a GUESS (confidence {c})"
            )?,
            // A keyed answer always has an agent; `Keyed` with none would be a
            // ledger row that named a transcript and no one, which pact cannot
            // write. Printed as the guess it is not, rather than unwrapped.
            (None, _) => writeln!(w, "    fleet identity: unknown — no guess")?,
        }
        for e in &g.evidence {
            writeln!(w, "      · {}: {}", signal_words(e.signal), e.detail)?;
        }
    }

    for warning in &r.warnings {
        writeln!(w, "\n! {warning}")?;
    }
    Ok(())
}

/// A [`Signal`] in words, for the digest. The JSON spells the same four as
/// `keyed` / `topology` / `time_overlap` / `transcript_mention`, via serde.
fn signal_words(signal: Signal) -> &'static str {
    match signal {
        Signal::Keyed => "keyed",
        Signal::Topology => "topology",
        Signal::TimeOverlap => "time overlap",
        Signal::TranscriptMention => "transcript mention",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process::Command;

    use serde_json::Value;
    use tempfile::TempDir;

    use crate::connector::generic_jsonl::GenericJsonl;
    use crate::model::{parse_ts, Turn};

    fn wt(path: &str, name: &str) -> Worktree {
        Worktree {
            path: PathBuf::from(path),
            name: name.to_string(),
            alive: true,
            session_dir: PathBuf::from("/nowhere"),
        }
    }

    fn only_generic() -> Vec<Box<dyn Connector>> {
        vec![Box::new(GenericJsonl)]
    }

    /// `{ts, role, content}` lines, one per argument.
    fn write_session(dir: &Path, name: &str, lines: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: String = lines
            .iter()
            .map(|(ts, text)| {
                format!("{{\"ts\":\"{ts}\",\"role\":\"user\",\"content\":{text:?}}}\n")
            })
            .collect();
        fs::write(&path, body).unwrap();
        path
    }

    fn json_of(r: &Report) -> Value {
        serde_json::from_str(&serde_json::to_string(r).unwrap()).unwrap()
    }

    /// The spec's calm-degradation clause, through the real `run`: a repo
    /// nothing has ever run in prints an empty list and exits 0.
    ///
    /// This drives the real connector registry and the real `$HOME`. It is
    /// hermetic because the repo lives in a fresh `TempDir`, so its encoded
    /// session directories (`-tmp-…`) cannot exist on any machine — which is
    /// exactly the fresh-machine shape being tested.
    #[test]
    fn a_repo_with_no_sessions_is_an_empty_list_and_exit_0() {
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

        let args = SessionsArgs {
            repo: repo.clone(),
            window: crate::cli::WindowArgs {
                since: None,
                until: None,
            },
        };
        assert_eq!(
            run(&args, true).expect("no sessions is not an error"),
            EXIT_OK
        );

        // And the same answer, inspected: an empty list, no warnings, no
        // language that reads as a failure.
        let worktrees = worktree::worktrees(&repo).unwrap();
        let report = collect(
            &connector::registry(),
            &repo,
            &worktrees,
            &[],
            &TimeWindow::default(),
        );
        assert!(report.sessions.is_empty());
        assert_eq!(report.discovered, 0);
        assert!(report.warnings.is_empty());
        let mut text = Vec::new();
        digest(&report, &mut text).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert!(text.contains("no sessions"), "{text}");
        assert!(!text.to_lowercase().contains("error"), "{text}");
    }

    /// The house rule the other four commands inherit: `--json` puts the
    /// document on stdout alone and the digest on stderr, second.
    #[test]
    fn json_goes_to_stdout_and_the_digest_to_stderr() {
        let report = collect(
            &only_generic(),
            Path::new("/r"),
            &[wt("/r", "main")],
            &[],
            &TimeWindow::default(),
        );

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&report, true, &mut out, &mut err).unwrap();
        serde_json::from_slice::<Value>(&out).expect("stdout is JSON and nothing else");
        assert!(String::from_utf8_lossy(&err).contains("recount sessions"));

        let (mut out, mut err) = (Vec::new(), Vec::new());
        emit(&report, false, &mut out, &mut err).unwrap();
        assert!(err.is_empty(), "no --json means nothing on stderr");
        assert!(String::from_utf8_lossy(&out).contains("recount sessions"));
        assert!(
            serde_json::from_slice::<Value>(&out).is_err(),
            "the digest is not JSON"
        );
    }

    /// The documented JSON shape, key by key. Audit scripts read this; a change
    /// here is a `SCHEMA` bump.
    #[test]
    fn json_shape_is_the_documented_contract() {
        let td = TempDir::new().unwrap();
        write_session(
            td.path(),
            "s.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions here")],
        );
        let worktrees = [wt("/r", "main")];
        let report = collect(
            &only_generic(),
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );

        let doc = json_of(&report);
        assert_eq!(doc["command"], "sessions");
        assert_eq!(doc["schema"], 2);
        assert_eq!(doc["repo"], "/r");
        assert_eq!(
            doc["window"],
            serde_json::json!({"since": null, "until": null})
        );
        assert_eq!(doc["discovered"], 1);
        assert_eq!(doc["excluded_by_cwd"], 0);
        assert_eq!(doc["warnings"], serde_json::json!([]));

        let s = &doc["sessions"][0];
        for key in [
            "path",
            "connector",
            "cwd",
            "first_ts",
            "last_ts",
            "turn_count",
            "parse_failures",
            "declined",
            "worktree",
            "repo_match",
            "fleet_identity",
        ] {
            assert!(
                !s[key].is_null() || matches!(key, "cwd" | "worktree"),
                "{key}"
            );
        }
        assert_eq!(s["connector"], "generic_jsonl");
        assert_eq!(s["turn_count"], 1);
        assert_eq!(s["first_ts"], "2026-08-19T07:00:00Z");
        // generic_jsonl states no cwd: kept, and labelled with the weaker basis.
        assert_eq!(s["cwd"], Value::Null);
        assert_eq!(s["worktree"], Value::Null);
        assert_eq!(s["repo_match"], "session_dir");

        let g = &s["fleet_identity"];
        assert_eq!(g["agent"], "cmdsessions");
        assert_eq!(g["tier"], "topological");
        assert_eq!(g["confidence"], 0.7);
        assert!(g["evidence"].as_array().is_some_and(|e| !e.is_empty()));
        assert_eq!(g["evidence"][1]["signal"], "transcript_mention");

        // And the digest never states the guess as a fact.
        let mut out = Vec::new();
        digest(&report, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("probably `cmdsessions` — a GUESS (confidence 0.70)"),
            "{text}"
        );
    }

    /// `--since`/`--until` reach `prune::partition` through `discover`, and
    /// what mtime cannot rule out the in-file span can.
    #[test]
    fn since_and_until_filter_the_list() {
        let td = TempDir::new().unwrap();
        let old = write_session(td.path(), "old.jsonl", &[("2026-08-01T00:00:00Z", "old")]);
        let new = write_session(td.path(), "new.jsonl", &[("2026-08-19T00:00:00Z", "new")]);
        // mtimes match the content, which is what an append-only file gives.
        for (p, ts) in [
            (&old, "2026-08-01T00:00:00Z"),
            (&new, "2026-08-19T00:00:00Z"),
        ] {
            let f = fs::File::options().write(true).open(p).unwrap();
            f.set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::SystemTime::from(parse_ts(ts).unwrap())),
            )
            .unwrap();
        }
        let worktrees = [wt("/r", "main")];
        let dirs = [td.path().to_path_buf()];
        let names = |w: TimeWindow| -> Vec<PathBuf> {
            collect(&only_generic(), Path::new("/r"), &worktrees, &dirs, &w)
                .sessions
                .into_iter()
                .map(|s| s.path)
                .collect()
        };

        assert_eq!(
            names(TimeWindow::default()).len(),
            2,
            "unbounded takes both"
        );
        assert_eq!(
            names(TimeWindow {
                since: parse_ts("2026-08-10T00:00:00Z").ok(),
                until: None
            }),
            vec![new.clone()],
            "--since prunes the old file on mtime"
        );
        assert_eq!(
            names(TimeWindow {
                since: None,
                until: parse_ts("2026-08-10T00:00:00Z").ok()
            }),
            vec![old],
            "--until cannot prune on mtime, so the span does it"
        );
        assert!(names(TimeWindow {
            since: parse_ts("2027-01-01T00:00:00Z").ok(),
            until: None
        })
        .is_empty());
    }

    /// A connector that reports a `cwd`, which `generic_jsonl` never does.
    /// Everything downstream of the cwd — the filter, the worktree name, the
    /// topology signal — needs one.
    struct Cwds(Vec<(PathBuf, Option<PathBuf>)>);

    impl Connector for Cwds {
        fn name(&self) -> &'static str {
            "test"
        }
        fn roots(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn discover(&self, _dirs: &[PathBuf], _window: &TimeWindow) -> Vec<SessionMeta> {
            self.0
                .iter()
                .map(|(path, cwd)| SessionMeta {
                    path: path.clone(),
                    cwd: cwd.clone(),
                    first_ts: parse_ts("2026-08-19T07:00:00Z").ok(),
                    last_ts: parse_ts("2026-08-19T08:00:00Z").ok(),
                    turn_count: 1,
                    parse_failures: 0,
                    declined: 0,
                    connector: "test".to_string(),
                })
                .collect()
        }
        fn read(&self, _path: &Path) -> Vec<Turn> {
            Vec::new()
        }
    }

    /// The spec's filter — "sessions whose cwd resolves into the repo or its
    /// worktrees" — and the fleet guess that rides on it.
    #[test]
    fn cwd_decides_membership_and_drives_the_guess() {
        let td = TempDir::new().unwrap();
        let inside = write_session(
            td.path(),
            "in.jsonl",
            &[("2026-08-19T07:00:00Z", "no names")],
        );
        let named = write_session(
            td.path(),
            "named.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions")],
        );
        let elsewhere = write_session(td.path(), "out.jsonl", &[("2026-08-19T07:00:00Z", "x")]);
        let quoting = write_session(
            td.path(),
            "quoting.jsonl",
            &[(
                "2026-08-19T07:00:00Z",
                "PACT_AGENT=cmdsessions and PACT_AGENT=join",
            )],
        );

        let worktrees = [
            wt("/r", "main"),
            wt("/r/wt/cmdsessions", "cmdsessions"),
            wt("/r/wt/gone", "gone"),
        ];
        let connectors: Vec<Box<dyn Connector>> = vec![Box::new(Cwds(vec![
            (inside.clone(), Some(PathBuf::from("/r/src"))),
            (named.clone(), Some(PathBuf::from("/r/wt/cmdsessions"))),
            (elsewhere, Some(PathBuf::from("/some/other/repo"))),
            (quoting.clone(), Some(PathBuf::from("/r/wt/cmdsessions"))),
        ]))];

        let r = collect(
            &connectors,
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );

        assert_eq!(r.discovered, 4);
        assert_eq!(r.excluded_by_cwd, 1, "the cwd in another repo is dropped");
        let by_path = |p: &Path| r.sessions.iter().find(|s| s.path == p).expect("kept");

        // Repo-root cwd: a member, but §4.3 says it names no one.
        let plain = by_path(&inside);
        assert_eq!(plain.repo_match, RepoMatch::Cwd);
        assert_eq!(plain.worktree.as_deref(), Some("main"));
        assert_eq!(plain.fleet_identity.agent, None);
        assert_eq!(
            plain.fleet_identity.tier,
            Tier::Topological(Confidence::new(0.0))
        );
        assert_eq!(
            plain.fleet_identity.evidence.len(),
            2,
            "unknown is an answer with reasons, not a shrug"
        );

        // Both signals agree — the strongest this ever gets, and still a guess.
        let strong = by_path(&named);
        assert_eq!(strong.worktree.as_deref(), Some("cmdsessions"));
        assert_eq!(strong.fleet_identity.agent.as_deref(), Some("cmdsessions"));
        assert_eq!(
            strong.fleet_identity.tier,
            Tier::Topological(Confidence::new(0.90))
        );

        // Two names in one transcript: the worktree breaks the tie, at a
        // visibly lower confidence.
        let ambiguous = by_path(&quoting);
        assert_eq!(
            ambiguous.fleet_identity.agent.as_deref(),
            Some("cmdsessions")
        );
        assert_eq!(
            ambiguous.fleet_identity.tier,
            Tier::Topological(Confidence::new(0.80))
        );
        assert!(ambiguous
            .fleet_identity
            .evidence
            .iter()
            .any(|e| e.detail.contains("names 2 identities")));

        // A worktree git lists but the filesystem does not is a warning, never
        // a silent omission.
        let mut worktrees = worktrees;
        worktrees[2].alive = false;
        let r = collect(
            &connectors,
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(r.warnings[0].contains("`gone`"), "{:?}", r.warnings);
    }

    /// `docs/format-notes.md` §4.2: 46 of 871 real files state more than one
    /// cwd, and `SessionMeta.cwd` is only the most frequent of them. A session
    /// that did most of its work in another repo and some of it here is listed
    /// with the worktree it touched, not counted in `excluded_by_cwd`.
    #[test]
    fn any_cwd_resolving_into_the_repo_is_enough() {
        let td = TempDir::new().unwrap();
        let line = |hm: &str, cwd: &str| {
            format!(
                "{{\"type\":\"user\",\"timestamp\":\"2026-08-19T{hm}:00.000Z\",\
                 \"cwd\":\"{cwd}\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}}}}\n"
            )
        };
        fs::write(
            td.path().join("multi.jsonl"),
            format!(
                "{}{}{}",
                line("07:00", "/other/repo"),
                line("07:01", "/other/repo"),
                line("07:02", "/r/wt/a")
            ),
        )
        .unwrap();

        let worktrees = [wt("/r", "main"), wt("/r/wt/a", "a")];
        // The real registry: only `claude_code` states a cwd at all, and only
        // it can be asked for the whole set.
        let r = collect(
            &connector::registry(),
            Path::new("/r"),
            &worktrees,
            &[td.path().to_path_buf()],
            &TimeWindow::default(),
        );

        assert_eq!(r.discovered, 1, "one file, one connector");
        assert_eq!(r.excluded_by_cwd, 0, "the summary lost, the set won");
        let s = &r.sessions[0];
        assert_eq!(s.connector, "claude_code");
        assert_eq!(s.repo_match, RepoMatch::Cwd);
        assert_eq!(s.worktree.as_deref(), Some("a"));
        assert_eq!(
            s.cwd.as_deref(),
            Some(Path::new("/other/repo")),
            "the summary is still reported as what it is"
        );
        // …and the topology evidence quotes the cwd that actually resolved, so
        // the entry cannot be read as a contradiction.
        let topology = &s.fleet_identity.evidence[0].detail;
        assert!(topology.contains("/r/wt/a is worktree `a`"), "{topology}");
        assert!(
            topology.contains("most frequent cwd is /other/repo"),
            "{topology}"
        );
    }

    #[test]
    fn pact_agent_names_reads_the_assignment_and_nothing_else() {
        let td = TempDir::new().unwrap();
        let path = td.path().join("t.jsonl");
        fs::write(
            &path,
            "export PACT_AGENT=cmdsessions BEADS_ACTOR=cmdsessions\n\
             PACT_AGENT=wave-2.a, then PACT_AGENT= nothing\n",
        )
        .unwrap();
        assert_eq!(
            pact_agent_names(&path).into_iter().collect::<Vec<_>>(),
            ["cmdsessions", "wave-2.a"]
        );
        // A file that is not there names no one, and does not fail.
        assert!(pact_agent_names(&td.path().join("nope")).is_empty());
    }

    /// `<repo>/.pact/events.jsonl`, written by hand: `event(line, agent,
    /// subagent)` produces the fields `pact_events` reads and nothing else,
    /// which is the tolerance contract from the other side.
    fn write_ledger(repo: &Path, rows: &[(&str, Option<&str>)]) {
        fs::create_dir_all(repo.join(".pact")).unwrap();
        let body: String = rows
            .iter()
            .map(|(agent, subagent)| {
                let sub = match subagent {
                    Some(s) => format!(r#","harness_subagent":"{s}""#),
                    None => String::new(),
                };
                format!(
                    r#"{{"at":"2026-08-19T07:00:00Z","agent":"{agent}","invoked_from":"main",
                       "harness":"claude-code","harness_session":"cd7c75eb"{sub}}}"#
                )
                .replace('\n', "")
                    + "\n"
            })
            .collect();
        fs::write(repo.join(".pact").join("events.jsonl"), body).unwrap();
    }

    /// The bead: where the ledger names the transcript, the identity is KNOWN
    /// and the event that proves it is named. Where it does not — which is
    /// most of any real corpus — nothing about the guess changes.
    #[test]
    fn a_transcript_the_ledger_names_is_known_not_guessed() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        let dir = td.path().join("sessions");
        // Line 2 names this transcript; line 1 is a top-level row with no
        // subagent, which must not key anything.
        write_ledger(
            &repo,
            &[("orchestrator", None), ("sessionschain", Some("aac2cd5f"))],
        );
        // The keyed file even *mentions* another identity: the ledger's
        // statement must win outright, not be weighed against a byte scan.
        let keyed = write_session(
            &dir,
            "agent-aac2cd5f.jsonl",
            &[("2026-08-19T07:00:00Z", "quoting PACT_AGENT=someone-else")],
        );
        let guessed = write_session(
            &dir,
            "agent-deadbeef.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions")],
        );

        let worktrees = [wt(repo.to_str().unwrap(), "main")];
        let r = collect(
            &only_generic(),
            &repo,
            &worktrees,
            std::slice::from_ref(&dir),
            &TimeWindow::default(),
        );
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let by_path = |p: &Path| r.sessions.iter().find(|s| s.path == p).expect("kept");

        let known = &by_path(&keyed).fleet_identity;
        assert_eq!(known.agent.as_deref(), Some("sessionschain"));
        assert_eq!(known.tier, Tier::Keyed);
        assert_eq!(
            known.evidence.len(),
            1,
            "a statement is not weighed against the signals it replaces: {:?}",
            known.evidence
        );
        assert_eq!(known.evidence[0].signal, Signal::Keyed);
        assert!(
            known.evidence[0].detail.contains("line 2"),
            "the event that proves it is named: {}",
            known.evidence[0].detail
        );

        // The un-keyed neighbour is untouched: same agent, same score, same
        // two pieces of evidence it had before the ledger was ever read.
        let guess = &by_path(&guessed).fleet_identity;
        assert_eq!(guess.agent.as_deref(), Some("cmdsessions"));
        assert_eq!(guess.tier, Tier::Topological(Confidence::new(0.70)));
        assert_eq!(guess.evidence.len(), 2);

        // The wire shape, both tiers, including the explicit null.
        let doc = json_of(&r);
        let of = |p: &Path| {
            doc["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["path"] == p.to_str().unwrap())
                .unwrap()["fleet_identity"]
                .clone()
        };
        assert_eq!(of(&keyed)["tier"], "keyed");
        assert_eq!(of(&keyed)["confidence"], Value::Null);
        assert_eq!(of(&guessed)["tier"], "topological");
        assert_eq!(of(&guessed)["confidence"], 0.7);

        // And the digest does not read like a stronger guess.
        let mut out = Vec::new();
        digest(&r, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains(
                "fleet identity: `sessionschain` — KNOWN, the ledger names this transcript"
            ),
            "{text}"
        );
        assert!(
            text.contains("probably `cmdsessions` — a GUESS (confidence 0.70)"),
            "{text}"
        );
        assert!(!text.contains("KNOWN (confidence"), "{text}");
    }

    /// Two agents against one transcript id is the ledger contradicting
    /// itself. That is not a statement of identity, so the guess stands — and
    /// the contradiction is reported rather than swallowed.
    #[test]
    fn a_ledger_that_names_two_agents_for_one_transcript_states_nothing() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        let dir = td.path().join("sessions");
        write_ledger(
            &repo,
            &[
                ("sessionschain", Some("aac2cd5f")),
                ("tier", Some("aac2cd5f")),
            ],
        );
        let path = write_session(
            &dir,
            "agent-aac2cd5f.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions")],
        );

        let worktrees = [wt(repo.to_str().unwrap(), "main")];
        let r = collect(
            &only_generic(),
            &repo,
            &worktrees,
            &[dir],
            &TimeWindow::default(),
        );
        let id = &r
            .sessions
            .iter()
            .find(|s| s.path == path)
            .unwrap()
            .fleet_identity;
        assert_eq!(id.tier, Tier::Topological(Confidence::new(0.70)));
        assert_eq!(id.agent.as_deref(), Some("cmdsessions"), "the guess stands");
        assert_eq!(id.evidence.len(), 3, "the contradiction is said out loud");
        assert_eq!(id.evidence[0].signal, Signal::Keyed);
        assert!(
            id.evidence[0]
                .detail
                .contains("2 different agents (sessionschain, tier)"),
            "{}",
            id.evidence[0].detail
        );
    }

    /// A `.pact/events.jsonl` that exists and cannot be read is a warning and
    /// a corpus of guesses, never a failed command: the sessions are still the
    /// answer, they are just all inferred.
    #[test]
    fn an_unreadable_ledger_is_a_warning_not_a_failure() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        // A directory where the file should be: readable path, unreadable file.
        fs::create_dir_all(repo.join(".pact").join("events.jsonl")).unwrap();
        let dir = td.path().join("sessions");
        write_session(
            &dir,
            "agent-aac2cd5f.jsonl",
            &[("2026-08-19T07:00:00Z", "export PACT_AGENT=cmdsessions")],
        );

        let r = collect(
            &only_generic(),
            &repo,
            &[wt(repo.to_str().unwrap(), "main")],
            &[dir],
            &TimeWindow::default(),
        );
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(
            r.sessions[0].fleet_identity.tier,
            Tier::Topological(Confidence::new(0.70))
        );
        assert_eq!(r.warnings.len(), 1);
        assert!(
            r.warnings[0].contains("Every identity below is a guess"),
            "{:?}",
            r.warnings
        );
    }

    #[test]
    fn a_transcript_id_is_the_filename_and_nothing_is_read() {
        assert_eq!(
            transcript_id(Path::new("/p/<s>/subagents/agent-a1ded78e.jsonl")),
            Some("a1ded78e")
        );
        // A top-level session is not named that way, and neither is anything
        // else: the same answer as a ledger that does not name it.
        assert_eq!(transcript_id(Path::new("/p/cd7c75eb-c39f.jsonl")), None);
        assert_eq!(transcript_id(Path::new("/p")), None);
    }
}
