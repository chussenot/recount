# recount documentation

The [project README](../README.md) explains *why* recount exists and why it is
shaped the way it is. Everything here is *how*. Pages are grouped by what you
are trying to do; within a group, read top to bottom.

## Start here

| Page | Read it when |
|---|---|
| [quickstart.md](quickstart.md) | You want to go from an audit finding to a transcript, now. Five commands and how to read the answer. |
| [install.md](install.md) | You want a prebuilt binary, the mise tasks, shell completions, or to know what recount depends on. |

## Using recount

| Page | Read it when |
|---|---|
| [cli.md](cli.md) | You need the exact contract: every command, flag, exit code and `--json` shape. Checked against the binary in CI. |
| [limitations.md](limitations.md) | You are about to trust a score. Where the join is weak, where it is wrong, and how its output tells you. |
| [worked-example.md](worked-example.md) | You want to see one real finding replayed end to end, including the tool giving a wrong answer. |
| [troubleshooting.md](troubleshooting.md) | A command printed an error, a `doctor` finding or a refusal, and you want to know what it means and what to do. |

## How it works

| Page | Read it when |
|---|---|
| [join.md](join.md) | An answer surprised you. The keyed and topological tiers, the score ladder, the lead rule, the refusals, the two fleet topologies. |
| [connectors.md](connectors.md) | You want to know how files become sessions: the two connectors, the claim rule, declined vs damaged lines. |
| [performance.md](performance.md) | A query is slow, or you want to know why a scan is cheap without a cache: the mtime prune and the measured ceilings. |

## Contributing

| Page | Read it when |
|---|---|
| [architecture.md](architecture.md) | You are about to change `src/`. The module map, how one query flows through it, where the code departs from the spec, and how the design was arrived at. |
| [development.md](development.md) | You are building, testing or releasing. Every gate, and the incident behind it. |
| [spec.md](spec.md) | You want the original contract. Frozen: departures are recorded in [architecture.md](architecture.md#departures-from-the-spec), never patched into it. |

## Research and evidence

These are dated field notes rather than living reference. They carry their
observation dates; the numbers in them are true of the day they were taken.

| Page | Read it when |
|---|---|
| [format-notes.md](format-notes.md) | You are touching the Claude Code connector. The reverse-engineered `~/.claude` layout, with its drift warning. |
| [session-learnings.md](session-learnings.md) | You want to know what recount found, and got wrong, when it was first used on other repositories' ledgers. |

## Conventions

- **The binary is the source of truth.** `scripts/check-docs.sh` (run by CI and
  `mise run check`) fails if [cli.md](cli.md)'s `Commands` block and the built
  binary disagree in either direction, or if any relative link or `#anchor`
  here is dead.
- **Sample output is pasted, never written.** Where a page shows terminal
  output, it says when and against what it was run.
- **Historical statements are kept and dated; stale claims are not.** "This used
  to be X; since 2026-08-20 it is Y" is how a behaviour change is recorded.
