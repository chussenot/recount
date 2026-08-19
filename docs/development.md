# Development

Building and testing recount, and why each gate is there. The second half is
the interesting one: nearly every check in this repository exists because
something got past the previous set.

## Build and test

```bash
cargo build
cargo test
mise run check   # the full CI gate, locally
```

`mise tasks` lists everything. The gates:

| task | what it runs |
|---|---|
| `mise run fmt-check` | `cargo fmt -- --check` |
| `mise run lint` | `cargo clippy --all-targets -- -D warnings` |
| `mise run test` | `cargo test` |
| `mise run lint-lines` | `scripts/check-lines.sh` (warning, never a gate) |
| `mise run check` | the four above, in CI's order, serially |
| `mise run bench` | the two spec ceilings, release only, minutes |
| `mise run dogfood` | recount against this repo's own log |

`--all-targets` on clippy is not the default and is not decoration: the default
misses `tests/`, and every proof obligation the spec sets lives there.

## Why `check` is serial

`mise run check` lists its four legs in `run` rather than in `depends`, and
that is not a style preference. `depends` runs tasks in **parallel**, and all
four legs reach the binary through the single path `target/debug/recount`. The
integration tests spawn it via `env!("CARGO_BIN_EXE_recount")`, so a parallel
leg rebuilding that file while another leg's tests are mid-spawn is a race with
a confusing failure. pact's own `mise.toml` records losing three runs out of
three to exactly this.

The order and the commands match `.github/workflows/ci.yml` exactly. A local
gate that does not match CI is not a gate.

## Why `bench` is not in `check`

`mise run bench` writes a ~193 MB synthetic corpus into a tempdir and takes
minutes. A required gate with those properties is one people switch off.

The task pins `--release` rather than trusting a human to remember, because
`tests/bench.rs` refuses to assert under `debug_assertions`: measured on the
same corpus and the same machine, the grep ceiling read 35.9 s in debug against
~1.06 s in release. A debug run still prints every measurement, labelled with
the profile it was taken in, and asserts nothing. Do not quote a debug number
as if it settled anything — a benchmark figure without its build profile,
corpus size and warm/cold state is a rumour, not a measurement.

Run it when you have touched the connectors, the prune, `grep` or the join.
The two ceilings and what they measure are in [performance.md](performance.md).

## Why `dogfood` exists

```bash
mise run dogfood
```

It runs `doctor`, `sessions` and `explain --event-line 1` against this
repository's own pact log. It is not in `check` — it reads `~/.claude`, its
output is for a human to look at, and it prints and exits 0 even when it finds
nothing.

It is here because it has already paid for itself twice. During the 26-agent
build of recount, with 111 green tests:

- `sessions --repo .` reported every transcript **twice** — once real, once as
  an empty `generic_jsonl` phantom. That is why `discover_all` exists rather
  than a loop over the registry.
- `--until` presented real 68-to-167-turn transcripts as **empty**, because a
  connector that recognised a file but windowed it out released its claim to
  the fallback. That is why claiming is by recognition, before the window
  applies.

Neither was visible to a single test, and both were obvious in one command's
output. A fixture asserts what its author already believed; the real corpus
does not.

`explain --event-line 1` exits 1 in the task and the task tolerates it. That
exit is a reasoned refusal, not a failure — see the README's weakness section.

## The scripts

**`scripts/check-lines.sh`** — the spec's house rule, files sized ~200-800
lines, the unit a person leases, reads and diffs. It counts code before the
first top-level `#[cfg(test)]`, because a co-located test module is where
Rust's tests idiomatically belong and nobody edits one to change behaviour.
Three files in this repo are over 800 *total* lines and all three are well
under 800 lines of code; a check that warned on those would be switched off
within a week, which is worse than no check. It is a **warning and always exits
0** — an 850-line file is a conversation, not a build break. One script, two
callers (`mise run lint-lines` and the CI job), so the local and CI checks
cannot drift apart.

**`scripts/check-docs.sh`** — the documentation gate. Build first, then:

```bash
cargo build && ./scripts/check-docs.sh
```

Two checks, both in both directions:

1. Every subcommand and every long flag the binary exposes appears in
   [cli.md](cli.md)'s `Commands` block, **and** every subcommand and long flag
   that block names still exists in the binary. The second direction matters
   most: a doc offering a flag the parser rejects sends a user to a usage error
   while believing the tool is broken. The command tree is read out of `--help`
   at runtime, never hardcoded in the script — a hardcoded list is the same
   drift problem one level down.
2. Every relative link and every `#anchor` in `README.md` and `docs/*.md`
   resolves. Anchors rot exactly like filenames, and silently.

It is not currently wired into `mise run check` or CI; run it by hand after a
change to the CLI surface.

## Test layout

`cargo test` runs 112 unit tests, 10 degradation tests and 3 fixture tests; the
2 bench tests are `#[ignore]`d.

**Unit tests live in-module**, under `#[cfg(test)]`, next to the code they pin.

**`tests/degradation.rs`** runs every command against empty and hostile trees:
a bare directory, a repo with events and no sessions, an empty projects root, a
session directory with no transcripts, an empty ledger, a garbage ledger, a
truncated ledger, an event line out of range, a `--repo` that is absent or is a
file, and an empty needle. The spec's obligation is "calm degradation, never a
panic", because recount will often run where transcripts do not exist. This
file is also where the output contract is pinned: it asserts `json.stderr ==
plain.stdout` byte for byte, that `--json` puts exactly one document on stdout,
and that a hard error writes one `recount: …` sentence to stderr and never a
partial document.

**`tests/join_fixture.rs`** discharges the spec's proof obligation against the
real binary: a fixture `~/.claude` tree and a fixture ledger with a join that is
**known by construction** — the tests take the query from line 2 of
`tests/fixtures/events.jsonl` rather than restating it, so the ledger cannot
drift from them. Three transcripts are offered and exactly one can win; the
loser says the agent's name on purpose, because the mention signal is a
tiebreaker between sessions the topology already admitted, never a way in.
`at_refuses_with_no_cwd_match_when_the_worktree_is_pruned` pins the refusal
*string*, not just the exit code, after a control run proving the same inputs
answer 0 while the directory is still there. Asserting only on the exit code
would pass for the wrong reason, which is the exact class of confident-wrong
answer recount was written to catch.

**`tests/bench.rs`** — two measurements, `#[ignore]`d, release only. See
[performance.md](performance.md).

## The two agents

`.claude/agents/` holds two, and they do not overlap:

- **`cli-surface-auditor`** — checks that the binary's own `--help` text and its
  generated completions still describe the binary that exists. Reach for it
  after adding or renaming a subcommand, flag, enum value or default, and
  whenever a help string quotes a constant that may have moved. `at --help`
  hand-writes the two refusal reasons, for instance; a third one added in
  `src/join.rs` would not update that prose.
- **`docs-curator`** — keeps `README.md` and `docs/` in step with the binary.
  Reach for it after any change a user could notice, when `check-docs.sh` fails,
  or when a doc claim needs verifying. Its first job is deciding whether
  anything needs documenting at all; "no doc change needed" is a correct answer.

Rough split: the auditor owns what the binary says about itself, the curator
owns what the repository says about the binary.

## Cutting a release

`cog bump` moves `Cargo.toml`, `Cargo.lock` and `CHANGELOG.md` together, then
tags. Push the tag and `.github/workflows/release.yml` does the rest.

It refuses more than it builds, and the refusals are the point. Before anything
is compiled, the `integrity` job checks that the tag's commit is an **ancestor
of `origin/master`** (a tag left behind by a rebase still resolves, still shows
a diff, and can ship something master never had), that `Cargo.toml` and
`Cargo.lock` both agree with the tag (a manifest that disagrees ships a binary
whose `--version` lies), and that `CHANGELOG.md` has a section for it. Each of
those is a minute-long failure instead of a bad release.

Then one job per target builds `--locked`, strips, re-signs on macOS — `strip`
invalidates the ad-hoc signature the linker gives every arm64 macOS binary, and
an invalidated signature does not warn, it dies with `Killed: 9` — and runs the
binary it just produced. Only then does `publish` checksum the tarballs, attach
a provenance attestation and create the release.

**Dry runs.** `workflow_dispatch` with a `tag` input builds and packages every
leg and publishes nothing. It gets that behaviour from the needs-chain rather
than from a condition of its own: the dispatch skips `integrity`, and `publish`
needs `integrity`. The same mechanism that stops a bad tag stops a dry run, so
the two cannot disagree.

Test the pipeline that way. The alternative is cutting a tag, and a tag is not a
thing you get to take back.

## House rules

From the spec: clippy `-D warnings`, `cargo fmt`, module docs that carry
decisions, files ~200-800 lines. The module-doc rule is the one that is easy to
under-read — source comments here carry reasoning and bead ids, and a doc page
that contradicts a module doc means one of the two is wrong. Determine which by
reading the code, not by picking.

`docs/spec.md` is treated as frozen. It was committed verbatim as the contract
the fleet built against, and where the code deliberately departs from it that
belongs in the README's deviations section, never in an edit to the spec. It has been edited once since, on the maintainer's instruction, to remove
references to a third-party project. That edit changed no requirement.

## See also

- [cli.md](cli.md) — the CLI contract the docs gate checks against
- [install.md](install.md) — installing, completions, the dependency budget
- [performance.md](performance.md) — the ceilings `bench` measures
