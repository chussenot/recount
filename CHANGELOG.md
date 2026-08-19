# Changelog
All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

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