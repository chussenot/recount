# Troubleshooting

Find the message you are looking at, read what it means, and apply the fix.
Every message quoted here is one recount actually prints. When an answer only
surprises you rather than failing, [join.md](join.md) explains how it was
reached, and [limitations.md](limitations.md) explains where the join is known
to be wrong.

## Before you start

You need three things for a join to be possible at all:

- a repository that pact coordinated, with a `.pact/events.jsonl`
- the machine where the agents ran, because their transcripts exist nowhere else
- the agents' worktrees, still on disk

Run `recount doctor` first. It always exits `0`, and it checks all three:

```bash
recount doctor --repo .
```

The `verdict` line reads `NOT READY — see findings` when something is missing,
and each line under `findings` names one problem. The sections below follow the
order you are likely to meet them in.

## `recount doctor` reports a finding

| Finding (start of the line) | What it means | What to do |
|---|---|---|
| `connector claude_code: root … does not exist` | No Claude Code sessions on this machine at all. | Run recount on the machine where the agents ran. |
| `no ledger at …/.pact/events.jsonl` | pact never ran in this repository, or `--repo` points somewhere else. | Pass the repository pact coordinated as `--repo`. |
| `… is empty` | pact ran here but recorded nothing. | Nothing to explain yet. |
| `…: N of M lines did not parse as events` | Part of the ledger is unreadable. | The rest still joins. Check the named lines with `sed -n <N>p`. |
| `worktree … is listed by git but its directory is gone` | The worktree an event names was deleted, so the join has no topology for it. | Restore the worktree if you can. Otherwise those events refuse with `no cwd match`. See [limitations.md](limitations.md#keep-your-agent-worktrees-until-the-audit-is-done). |
| `worktree …: no session directory at …` | No transcript was ever recorded for that checkout on this machine. | Check you are on the right machine, and that the agents ran in that worktree. |
| `no sessions under any of this repo's … session directories` | Nothing to join on this machine. | As above. |
| `connector …: N declined across …` | recount chose not to read those lines (a line-size cap or a shape the connector does not take). | Usually nothing. Declined is policy, not damage: see [connectors.md](connectors.md#declined-is-not-parse_failures). |
| `connector …: N damaged across …` | Those lines did not parse. | The one tally that means a file is damaged. The rest of each file still reads. |

## `at`, `explain` or `testify` exits `1`

Exit `1` from these commands is an answer, not a crash. The output says which
of four answers you got.

### `no confident match — reason: no cwd match`

No session's working directory resolved into the worktree the event names, or
that worktree is gone. Look at the topology:

1. Run `recount doctor --repo .` and check the worktree's line.
2. Run `recount sessions --repo .` to see which sessions resolve where.
3. If the event has no `invoked_from`, recount falls back to a worktree named
   after the agent. The refusal's detail line lists the worktrees it knows.

### `no confident match — reason: no overlap`

Sessions matched the worktree, but none has a turn inside the time window. Look
at the clock:

1. Widen the window, for example `--window 15m`. The default is `5m` each side.
2. Compare the event's moment with each session's span: `recount sessions
   --repo .` prints every session's first and last turn.

### `no confident match — reason: no keyed transcript`

The event named its transcript (pact stamped `harness_session`, and perhaps
`harness_subagent`), and that file is not on disk. Either the transcript was
pruned (Claude Code keeps about 30 days, and deleting a worktree takes its
session directory with it), or the ledger came from another machine and the
transcript was never here. The detail line lists every path it checked.

### `LEAD — NOT CONFIDENT`

recount found a session, but nothing structural picked it out. Two causes:

- The worktree could only filter, typically because several subagents of one
  session share it, and a transcript mention picked this one.
- With `tier keyed`, the ledger names more than one agent writing under the
  session the event named.

The path is printed and the transcript is withheld. Open the file and check who
was speaking before you treat it as testimony.
[limitations.md](limitations.md) explains both causes and what a fleet can
change to avoid them.

### `NOT CONFIDENT`

A candidate scored below the bar of `0.50`. The evidence lines say which signal
fell short.

## Input errors

| Message | Cause | Fix |
|---|---|---|
| `error: unrecognized subcommand '…'` (exit `2`) | The `recount` on your `PATH` is older than the command. `testify` arrived in 0.2.0. | Run `recount --version`, then reinstall with `mise run install`. |
| `no pact ledger at … — nothing to explain here` | `explain` was pointed at a repository with no ledger. | Pass the right `--repo`. |
| `no event on line N: lines are 1-based and the ledger has M` | The line number is out of range. | Lines are numbered from 1, as `sed -n <N>p` counts them. |
| ``stdin is not JSON — testify reads a `pact audit --json` document`` | Nothing, or not JSON, reached `testify`. | Check the producer: `pact audit --check <check> --json`. |
| `stdin is JSON but not an audit: it is …` | A number, string, boolean or `null` reached `testify`. | Pipe the audit document, an array of findings, or one finding object. |

## Build and development errors

### `output file … is not writeable`

A Cargo that bypasses the mbx build cache tried to rebuild an artifact mbx
restored as a read-only hard link. Run Cargo through mise, or put mbx's Cargo
shim first on `PATH`:

```bash
mise exec -- cargo build
# or, once:
mbx setup   # then prepend the directory it prints to PATH
```

[development.md](development.md#the-build-cache) explains why `target` is a
link into mbx's store.

### `cargo clean` changes nothing

Under mbx, `cargo clean` removes only the `target` link, and the next build
relinks the same managed target. Use `mise run clean`, which also runs
`mbx clean`.

### `scripts/check-docs.sh` fails

| Message | Fix |
|---|---|
| ``docs/cli.md does not document the `…` subcommand``, or ``… `--flag` on `recount …` `` | Add it to the `Commands` block of [cli.md](cli.md#commands). |
| ``docs/cli.md documents `--flag`, which no command accepts`` | Remove it, or fix its spelling. |
| `link target does not exist` or `no heading matches anchor` | A page or a heading was renamed. Fix the link. |
| `./target/debug/recount not built` | Run `cargo build` first. |

## Still stuck

Run the command again with `--json` and read the `evidence` array, or the
`refusal.detail` field: they carry the full reasoning that the digest
shortens. [cli.md](cli.md#json-shapes) documents every field.
