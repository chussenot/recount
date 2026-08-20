# Changelog
All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## 0.2.0 - 2026-08-20

recount now consumes pact 0.11.0's attribution chain, and — having built the
tooling to interrogate coordination ledgers — was pointed at four of them,
including its own. It found more wrong with itself than with anything else it
was auditing. Both halves are below, because the second half is the one that
changed the software.

### An event can now name its transcript

pact 0.11.0 stamps `harness`, `model`, `harness_session`, `harness_subagent`
and `branch` on every event. Where a `harness_subagent` is present, recount
resolves the transcript by **filename** — one path join and one `stat` — and
reports it as an **identity rather than a score**:

    tier keyed · no score, and none is missing: the event named this transcript

`Tier` is an enum whose keyed arm holds no `f64`, so a caller cannot print a
confidence for an identity even by accident. In JSON a keyed answer writes
`{"tier":"keyed","confidence":null}` — an explicit null, so a script
thresholding `.confidence` fails loudly instead of quietly ranking an identity
against a guess. A top-level agent, which has a session and is nobody's
subagent, keys to its own session transcript by the same mechanism.

`recount testify` closes the loop: `pact audit --json | recount testify` takes
findings in and emits them annotated with testimony and its tier.

`at` and `explain` print the whole chain, with `model` marked `(declared)`
because `PACT_MODEL` is what the spawner *asked for*, not what answered.
Against what the session actually recorded there are three verdicts, not two:
`agrees`, `differs`, and **`switched`** — folding a mid-run model change into
`agrees` would hide it, folding it into `differs` would accuse an agent that
obeyed its declaration.

### What it found when pointed at itself

Across pact, quern, millrace and arkanoid, the **topological tier scored one
correct match out of thirteen verifiable ones — and printed `CONFIDENT`.**
Eighteen of pact's forty-five stale-hold findings collapsed onto a single
thirteen-day, 14,129-turn session that overlaps every window and mentions every
agent.

Two independent readers then misdiagnosed the cause, both from a *true*
evidence sentence that reported the session's span — because the span was what
the scorer looked at. Three things were wrong and all three are fixed:

- **Overlap now means a turn inside the window**, not a span that straddles it.
  `Match` carries the turns it found, so a scored answer with an empty window is
  now unrepresentable rather than merely unlikely.
- **The repo root names no one.** Resolving `invoked_from: main` to the main
  checkout no longer counts as the chain discriminating, however few candidates
  happen to sit there. `sessions` already told users this; the join now agrees.
- **A mention-decided answer is a LEAD, not a confident answer**, and says so in
  words rather than leaving it to be inferred from a number.

The cost, stated plainly: **48 previously-confident answers across pact and
quern are now leads.** Every candidate is still found, scored and printed with
its evidence — nothing is hidden, and no refusal changed. The release answers
fewer questions and lies about none of them.

### Also

Agents can discover their own transcript id (`docs/format-notes.md` §10), by a
marker grep with a one-match guard. The obvious alternative — newest file by
mtime — was tested with two agents spawned simultaneously and **each resolved to
the other's transcript**; it must never ship, because an absent id is honest and
a wrong one is a corrupted attribution chain that looks correct.

`docs/session-learnings.md` records what the older ledgers actually said, and
seven findings are filed in the pact repo. All three behavioural questions this
run set out to answer were answered by killing their premise — most notably,
the red-master rule agents were thought to have defied was committed
thirty-eight minutes after the agent it concerns had already parked.

#### Bug Fixes
- **(explain)** print the attribution chain once, in at's header only - (384bf08) - Clement HUSSENOT-DESENONGES
- **(explain)** fill the new Event fields in a test literal - (1079303) - Clement HUSSENOT-DESENONGES
- **(join)** the repo root names no one, and a mention-decided answer is a lead - (719bede) - Clement HUSSENOT-DESENONGES
- **(join)** time overlap means a turn in the window, not a span that straddles it - (6072606) - Clement HUSSENOT-DESENONGES
#### Documentation
- **(cli)** the chain header, the tier field and the model comparison - (4c8ca93) - Clement HUSSENOT-DESENONGES
- **(format-notes)** record whether a subagent can learn its own transcript id - (ff4bf1f) - Clement HUSSENOT-DESENONGES
- **(join,connectors)** the keyed tier, and what it does not fix - (9249959) - Clement HUSSENOT-DESENONGES
- **(learnings)** turn recount on four older ledgers, and on itself - (ac1e19c) - Clement HUSSENOT-DESENONGES
- **(model)** a top-level row keys on harness_session alone - (a8c2c98) - Clement HUSSENOT-DESENONGES
- remove the third-party credit from the documentation - (b3f9d9e) - Clement HUSSENOT-DESENONGES
#### Features
- **(at)** the testimony header names the whole chain, and checks the model - (d889b50) - Clement HUSSENOT-DESENONGES
- **(claude_code)** key a transcript by subagent id, and report the model it recorded - (49080eb) - Clement HUSSENOT-DESENONGES
- **(explain)** pass the event's attribution chain into the join - (b67bc92) - Clement HUSSENOT-DESENONGES
- **(explain)** pass the event's attribution chain into the join - (0651f39) - Clement HUSSENOT-DESENONGES
- **(join)** key a top-level agent to its own session transcript - (d94acd2) - Clement HUSSENOT-DESENONGES
- **(join)** a keyed tier above the topological ladder - (4a2e5e2) - Clement HUSSENOT-DESENONGES
- **(model)** a confidence tier, so an answer says how it was reached - (395603e) - Clement HUSSENOT-DESENONGES
- **(pact_events)** carry pact 0.11.0's attribution chain on Event - (60fb8fd) - Clement HUSSENOT-DESENONGES
- **(sessions)** a known identity is not a guess - (6854cb5) - Clement HUSSENOT-DESENONGES
- **(testify)** an audit finding in, the reason why out - (963bb60) - Clement HUSSENOT-DESENONGES
#### Miscellaneous Chores
- **(model,at,claude_code)** the minimum to keep the tree green on Tier - (e05bf96) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the honesty2 barrier - (0f23381) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the overlapfix barrier - (daa1d8b) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the phase-3 barrier - (6237d2b) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the wave-5 barrier - (c7aea9f) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint ledger after wave 4 - (7929c23) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the wave-4 barrier - (09607d5) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint ledger after wave 3 - (6a78562) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the wave-3 barrier - (cadea2f) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint ledger after wave 2 - (6c412a7) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the wave-2 barrier - (b6dd794) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint ledger after wave 1 - (e88db8b) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint before the wave-1 barrier - (8a727e8) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint ledger before wave 1 - (3296219) - Clement HUSSENOT-DESENONGES
#### Tests
- **(fixtures)** keyed tree, model transcripts, canned audits, keyed bench - (aedb57d) - Clement HUSSENOT-DESENONGES

- - -
## 0.1.0 - 2026-08-19
#### Continuous Integration
- add the release pipeline and dependabot, adapted from pact - (9df6849) - Clement HUSSENOT-DESENONGES
#### Documentation
- restructure around why, and add the gate that keeps it honest - (0481175) - Clement HUSSENOT-DESENONGES
#### Features
- **(cli)** add `completion`, deepen --help, and reconcile the docs with both - (4a43241) - Clement HUSSENOT-DESENONGES
#### Miscellaneous Chores
- **(agents)** add the cli-surface-auditor, ported from pact - (457a254) - Clement HUSSENOT-DESENONGES

- - -
## 0.0.1 - 2026-08-19
#### Bug Fixes
- **(at)** set declined on the test fixture so recount-hwk.45 can land - (2bad33b) - Clement HUSSENOT-DESENONGES
- **(connector)** claim by recognition, apply the window once in discover_all - (751fe10) - Clement HUSSENOT-DESENONGES
- **(integration)** one connector per path, one --json contract, cwd as a set - (7b589b1) - Clement HUSSENOT-DESENONGES
- **(model)** a line we declined is not a line that was damaged - (1b64216) - Clement HUSSENOT-DESENONGES
- **(orchestration)** refuse to close a worktree whose branch is unmerged - (0d69a96) - Clement HUSSENOT-DESENONGES
#### Build system
- add mise tasks for build, test, lint, bench, install and check - (e65a4c3) - Clement HUSSENOT-DESENONGES
#### Continuous Integration
- enforce fmt, clippy -D warnings and test on every push - (90540df) - Clement HUSSENOT-DESENONGES
#### Documentation
- **(format-notes)** reconcile the observations with the shipped connector - (ddbb742) - Clement HUSSENOT-DESENONGES
- **(format-notes)** empirical ~/.claude/projects layout observed 2026-08-19 - (e13629b) - Clement HUSSENOT-DESENONGES
- **(worked-example)** replay this repo's own ledger, right answer and wrong one - (f9ec523) - Clement HUSSENOT-DESENONGES
- README — the refusals are the feature - (5196e98) - Clement HUSSENOT-DESENONGES
- commit the recount spec verbatim as the fleet contract - (26121d6) - Clement HUSSENOT-DESENONGES
#### Features
- **(at)** resolve the session driving an identity, or refuse with a reason - (b1181ee) - Clement HUSSENOT-DESENONGES
- **(connector)** claude_code, implementing the observed session format - (7a0750e) - Clement HUSSENOT-DESENONGES
- **(connector)** best-effort generic_jsonl reader - (1c19e18) - Clement HUSSENOT-DESENONGES
- **(connector)** fill in the registry and the candidate walk - (4924322) - Clement HUSSENOT-DESENONGES
- **(doctor)** report connector roots, session counts, tallies and join-readiness - (de58cd1) - Clement HUSSENOT-DESENONGES
- **(explain)** one command from a ledger finding to the agent's own words - (d30e185) - Clement HUSSENOT-DESENONGES
- **(grep)** case-insensitive substring hits with their coordinates - (6f87962) - Clement HUSSENOT-DESENONGES
- **(join)** topological scoring with degeneracy detection - (7016abb) - Clement HUSSENOT-DESENONGES
- **(pact_events)** tolerant reader for .pact/events.jsonl - (0969080) - Clement HUSSENOT-DESENONGES
- **(prune)** mtime lower-bound windowing that skips files unopened - (4c103d7) - Clement HUSSENOT-DESENONGES
- **(sessions)** list a repo's transcripts with fleet-identity guesses - (907e83d) - Clement HUSSENOT-DESENONGES
- **(skeleton)** declare the module tree and the whole CLI surface - (42e293a) - Clement HUSSENOT-DESENONGES
- **(worktree)** resolve --repo to its encoded session directories - (1c0db50) - Clement HUSSENOT-DESENONGES
#### Miscellaneous Chores
- **(orchestration)** final checkpoint and audit export - (c8aea86) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 7 - (afb82b9) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 6 - (28243c9) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint plan before the wave-6 barrier - (d72e7ff) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 4 - (4685e58) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 3 - (3e6faa9) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 2 - (ef6146f) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 1 - (a89928c) - Clement HUSSENOT-DESENONGES
- **(orchestration)** checkpoint coordination logs after wave 0 - (0a45d62) - Clement HUSSENOT-DESENONGES
- **(orchestration)** add the wave helper the orchestrator lands branches with - (36b976c) - Clement HUSSENOT-DESENONGES
- **(pact)** sync the coordination protocol block - (dc5922e) - Clement HUSSENOT-DESENONGES
- track Cargo.lock - (095df2f) - Clement HUSSENOT-DESENONGES
- ignore agent worktrees under wt/ - (b366a16) - Clement HUSSENOT-DESENONGES
- scaffold cargo crate, beads store, and pact protocol - (499a019) - Clement HUSSENOT-DESENONGES
- Setup - (9c22ac7) - Clement HUSSENOT-DESENONGES
- Root commit - (f86b159) - Clement HUSSENOT-DESENONGES
#### Performance Improvements
- **(grep)** meet the spec's 2s ceiling by removing two redundant passes - (6db3268) - Clement HUSSENOT-DESENONGES
#### Tests
- **(bench)** the two ceilings the spec puts on at and grep - (ebaef30) - Clement HUSSENOT-DESENONGES
- **(degradation)** every command against events with no sessions - (a52d2e0) - Clement HUSSENOT-DESENONGES
- **(fixtures)** a known join, and the pruned worktree at must refuse - (57b53af) - Clement HUSSENOT-DESENONGES

- - -

Changelog generated by [cocogitto](https://github.com/cocogitto/cocogitto).