# recount — session testimony for pact audits

One job: join pact's coordination ledger (WHAT agents did) to coding-
agent session transcripts (WHY), so an audit finding can be answered
with the agent's own reasoning. Motivating incident: the arkanoid-v1
misdiagnosis (arkanoid docs/pact-audit.md), where a transcript would
have answered in one line what artifact-inference got wrong.

## What recount refuses to be (README section, verbatim intent)
cass (Dicklesworthstone/coding_agent_session_search, ~685k lines)
proves the idea at product scale: 25 connectors, SQLite+Tantivy,
embeddings, TUI, sync, crypto. recount implements the audit-testimony
3% and refuses the rest BY DESIGN: no TUI, no daemon, no network, no
embeddings, no fuzzy search (case-insensitive substring only), one
connector family at launch — and NO PERSISTENCE: session files are
the only truth; recount owns no store that can drift from sources it
does not own. Scans are cheap structurally instead: (1) --repo
resolves to the project's encoded session directories (repo +
worktrees) before any file opens; (2) sessions are append-only, so
mtime prunes files outside the query window unopened. Deferral,
recorded in README: a metadata-only sidecar manifest (rebuilt
wholesale on any mtime mismatch, rm-safe) may be added iff real
windowed queries exceed ~2s on a real corpus — never a content index,
never a database. Dependency budget: serde/serde_json, clap, chrono,
anyhow; anything else needs a justifying comment. House rules: clippy
-D warnings, fmt, module docs that carry decisions, files sized
~200-800 lines (the lease/context/diff unit).

## Connectors (exactly two)
1. claude_code: sessions under ~/.claude/projects/<encoded-path>/
   *.jsonl. Discover the real layout EMPIRICALLY on this machine
   before coding; document the observed schema in docs/format-notes.md
   with the date and a "reverse-engineered, expect drift" warning.
   Tolerant parsing: unknown fields ignored, truncated tails counted
   not fatal, one bad line never fails a file.
2. generic_jsonl: any dir of *.jsonl with {ts|timestamp, role,
   content|text}; documented best-effort.
Connector trait (discover() -> Vec<SessionMeta>, read(path) ->
Vec<Turn>) so a third is one file — but do not add one.

## The join (the whole point)
pact events carry agent, at, path, invoked_from (worktree name),
detail. Sessions carry cwd and timestamps. Join key is TOPOLOGICAL:
invoked_from -> worktree path -> session cwd, plus required time-
window overlap, plus a transcript-mention tiebreaker (PACT_AGENT name
or bead id). Scored match that always reports confidence and
evidence, never a bare answer. recount reads .pact/events.jsonl
directly (tolerant, chain_hash ignored); the dependency is one-way —
pact never learns recount exists.

## Commands (all --json; human digest second)
- recount sessions --repo <p> [--since --until] : sessions whose cwd
  resolves into the repo or its worktrees, fleet-identity guesses
  attached.
- recount at --repo <p> --agent <name> --ts <rfc3339> [--window 5m] :
  resolve the session driving that identity at that moment; print the
  transcript window. Exit 0 found / 1 no-confident-match with the
  reason (no cwd match vs no overlap).
- recount grep <needle> --repo <p> [--agent --since --until] :
  (session, line, ts, role, snippet) hits.
- recount explain --repo <p> --event-line <N> : read event N from
  .pact/events.jsonl, run `at` on it — one command from finding to
  testimony.
- recount doctor : connector roots, session counts, parse-failure
  tallies, join-readiness (events present? worktree paths alive?).

## Proof obligations
Fixture ~/.claude tree + fixture events.jsonl with a KNOWN join: `at`
finds it, and correctly refuses (exit 1 + reason) when the worktree
is pruned. Truncated/garbage fixtures. Integration: run every command
against a repo with events but NO sessions — calm degradation, never
a panic (recount will often run where transcripts don't exist).
Bench: pruned `at` <100ms; unwindowed grep over 500 synthetic
sessions (~200MB) <2s. Docs: README (incl. the refusals section and
the cass credit), format-notes.md, and one worked example replaying a
real past finding from this machine's history if present, else the
fixture.
