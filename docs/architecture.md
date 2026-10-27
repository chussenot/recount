# Architecture

How the code is laid out, how one query flows through it, where the code
deliberately departs from [spec.md](spec.md), and how the design was arrived at.
Read this before you change anything under `src/`; read
[development.md](development.md) for how to build and test it.

Every module opens with a `//!` doc comment that carries the reasoning for its
own rules. This page is the map; those comments are the territory.

## The module map

recount is one binary with no Cargo features and six direct dependencies —
`serde`, `serde_json`, `clap`, `clap_complete`, `chrono`, `anyhow`. `Cargo.toml`
states the budget and requires a justifying comment beside anything outside it,
which `clap_complete` carries.

| Module | Owns |
|---|---|
| `src/main.rs` | The module tree and dispatch only: a flat match from subcommand to `cmd::*`, and the mapping of a hard error to exit `1` |
| `src/cli.rs` | The whole command-line surface, in one file — every command, flag, default, `--help` text and exit-code table |
| `src/cmd/` | One module per command (`sessions`, `at`, `grep`, `explain`, `testify`, `doctor`, `completion`), each with one `run` returning an exit code rather than calling `exit` |
| `src/model.rs` | The shared vocabulary: `Turn`, `SessionMeta`, `TimeWindow`, and the confidence/evidence pair every answer carries |
| `src/pact_events.rs` | A tolerant reader for `.pact/events.jsonl`. One-way: recount reads pact's ledger, pact never learns recount exists, and `chain_hash` is ignored on purpose |
| `src/worktree.rs` | `--repo` → its worktrees → their encoded session directories, and the reverse: a session's `cwd` → which worktree it was |
| `src/connector/` | The connector seam (`mod.rs`) and its two implementations, `claude_code.rs` and `generic_jsonl.rs` — see [connectors.md](connectors.md) |
| `src/prune.rs` | The one-sided mtime prune: which files a query provably need not open — see [performance.md](performance.md) |
| `src/join.rs` | The join itself: the keyed tier, the topological tier and its score ladder, the refusals — see [join.md](join.md) |

The dependency direction is strictly downward: `cmd` calls `join`, `connector`,
`worktree` and `pact_events`; `join` never decides which connector owns a file
(it is handed a callback that reads transcripts); connectors call `prune` before
they open anything.

## How one query flows

`recount explain --repo . --event-line 42` is the longest path through the code,
and every other command is a prefix of it.

1. **Parse.** `cli.rs` turns argv into a typed `Command`. A usage error exits
   `2` from clap before any recount code runs.
2. **Read the event.** `cmd::explain` reads line 42 of `.pact/events.jsonl`
   through `pact_events`, and builds the same query `at` would — the event's
   agent, its timestamp, and the attribution fields pact stamped on it. It then
   calls `at`'s own entry point, so it inherits `at`'s join, refusals and exit
   codes rather than reimplementing them.
3. **Resolve where to look.** `worktree::worktrees` asks git for the repo's
   worktrees, and `worktree::session_dirs` encodes each into the session
   directory a coding agent would have written under. Nothing has been opened
   yet.
4. **Discover.** `connector::discover_all` runs the two connectors in order over
   those directories. Each prunes on mtime first, so a file that ended before
   the window is never opened; a path the first connector claims is dropped from
   the second's results, so no transcript is reported twice.
5. **Join.** `join::resolve` takes the query, the worktrees and the discovered
   sessions. If the event names its transcript (`harness_subagent` or
   `harness_session`), the keyed tier returns that identity with no score.
   Otherwise the topological tier filters by worktree, gates on time overlap,
   breaks ties on transcript mentions, and returns a scored match with its
   evidence — or a refusal naming one of its two reasons. Transcripts are read
   lazily, only for candidates that survive topology.
6. **Explain a topology refusal honestly.** If the cheap, windowed pass refused
   on topology, `at` repeats the scan unwindowed, because the prune may have
   removed exactly the sessions a "no overlap" refusal would have cited. A match
   never needs this second pass; only a refusal pays for it.
7. **Print.** Without `--json`, the human digest goes to stdout. With `--json`,
   the JSON document goes to stdout and the same digest to stderr, so a pipe
   gets clean JSON and a person at the terminal still gets the reading. The exit
   code is `0` for an answer, `1` for a refusal or a below-the-bar candidate.

`testify` runs steps 3–7 once per finding of a `pact audit --json` document read
from stdin or `--file`. `sessions` and `grep` stop after step 4 — `sessions`
also reads the ledger to attach a fleet identity, and `grep` reads every
surviving transcript. `doctor` runs steps 3 and 4 and reports on them instead of
joining.

## Departures from the spec

[spec.md](spec.md) was committed verbatim as the contract this tool was built
against, and is treated as frozen: where the code departs from it, the departure
is recorded here rather than edited into the spec. (It has been edited once
since, to remove references to a third-party project — a change to no
requirement, and the only edit it has had.)

**`recount completion <shell>` is a command the spec does not name, and the one
that does not honour `--json`.** The spec's command list says "all `--json`;
human digest second". `completion` writes a shell program to stdout, so a digest
beside it would be sourced as code; `--json` is accepted and ignored. The
alternative was checked-in completion scripts, which are hand-written copies of
a CLI and drift from it the first time a flag changes.

**`recount testify` is a command the 0.1.0 spec does not name.** It arrived in
0.2.0 as a new obligation of that release — reading `pact audit --json` and
annotating every finding — not as a flag some piece of work wanted mid-build. It
follows the spec's rules for every other command: `--json` first, digest second,
exit `1` naming every finding it could not answer.

**The connector's `discover` takes the scan scope as an argument.** The spec
writes it as `discover()`, taking nothing. But the spec's own cheapness clauses
— resolve the session directories "before any file opens", and prune outside the
window "unopened" — are unsatisfiable if the connector cannot learn the scope
before it opens anything. So it is `discover(dirs, window)`. The trait does the
work in those two methods, `discover` and `read`, plus `name()`, which ties a session back to
the connector that can read it, and `roots()`, which `doctor` reports; the spec's "a third is one file" still holds.

**The join has a tier above the topological key.** The spec says "Join key is
TOPOLOGICAL". When pact began stamping `harness_session` and `harness_subagent`
on its events, an event could *name* its transcript, and recount resolves those
rows by identity before the topological key is consulted. The topological key is
unchanged and still answers every row that names nothing. Why the two tiers are
not scored against each other is in
[join.md](join.md#two-tiers-and-only-the-lower-one-is-scored).

## Where the design came from

recount was built by a fleet of 26 coordinated agents against a specification
frozen before any code was written, and most of what the README defends is a
finding rather than a plan.

The topology ceiling was discovered by an agent surveying 169,775 lines of real
transcripts and noticing that its own fleet's sessions never named the worktrees
they worked in. The separation of *declined* from *damaged* lines exists because
`doctor` shipped a sentence claiming twelve transcripts were corrupt when the
corpus had zero corruption — the twelve were one 9.6-million-character prompt
that had quoted itself into a size cap. The connector ownership rule exists
because a `--until` query was presenting real transcripts as empty. None of
those were caught by the test suite; all three were obvious the first time
someone pointed the tool at its own repository, which is why doing that is a
task (`mise run dogfood`) rather than a habit.

The full account, including what the coordination audit could and could not
verify afterwards, is in the beads and in `.plan/checkpoints/`.

## See also

- [join.md](join.md), [connectors.md](connectors.md),
  [performance.md](performance.md) — each subsystem in depth.
- [development.md](development.md) — building, testing, and why each gate
  exists.
- [spec.md](spec.md) — the frozen contract the departures above are measured
  against.
