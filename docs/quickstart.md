# Quickstart

From a pact audit finding to the agent's own words, in five commands. This page
is the shortest path; [cli.md](cli.md) is the full contract, and the
[README](../README.md) explains why the tool is shaped this way.

You need a repository that pact has been coordinating (it has a
`.pact/events.jsonl`), on the machine where the agents ran — recount reads their
session transcripts from disk, and they exist nowhere else.

## 1. Install

```bash
git clone https://github.com/chussenot/recount && cd recount
cargo install --path .
recount --version
```

That is the whole installation: one binary, no config file, no runtime state, no
network. Prebuilt binaries, the mise tasks and shell completions are in
[install.md](install.md).

## 2. Ask whether a join can run here

```bash
cd /path/to/your/fleet-repo
recount doctor
```

`doctor` always exits `0`, and answers the questions that decide everything
after it: does the session root exist, is there a ledger, are the worktrees it
names still alive, and does any transcript sit under them. Its last line is a
verdict, followed by findings that say what is missing and where it looked. If
it says `NOT READY`, fix what the findings name before going further —
[limitations.md](limitations.md#keep-your-agent-worktrees-until-the-audit-is-done)
explains the most common cause.

## 3. See which transcripts belong to the repo

```bash
recount sessions --repo . --since 2h
```

Every session whose working directory resolves into the repo or one of its
worktrees, each with the fleet identity attached — stated where the ledger names
the transcript, marked as a guess everywhere else.

## 4. Explain one event

```bash
pact log                                   # find the event you care about
recount explain --repo . --event-line 42   # 1-based, as pact log numbers them
```

`explain` reads that line of `.pact/events.jsonl` and resolves the session that
was driving the event's agent at that moment, then prints the transcript window
around it.

## 5. Testify a whole audit

```bash
pact audit --check stale-holds --json | recount testify --repo .
```

Every finding comes back verbatim, with the transcript behind it attached. Exit
`1` means at least one finding had an agent and a moment and still got no
confident answer; the output lists exactly which, and why.

## Reading an answer

Every answer has a verdict, and the verdict matters more than the number next to
it. There are five outcomes:

| You see | It means | Exit |
|---|---|---:|
| `tier keyed` · no score | The ledger named the transcript (pact stamped `harness_session` / `harness_subagent` on the event). This is an identity, not a guess — provided the agent that wrote the row is the one it names ([limitations.md](limitations.md#a-keyed-answer-is-only-as-honest-as-the-row-that-named-it)). | `0` |
| `confidence 0.70` or `0.95` · `CONFIDENT` | The worktree singled the session out. An answer that stands on its own. | `0` |
| `LEAD — NOT CONFIDENT` | Nothing structural picked this session out. Usually the worktree could only *filter* — several sessions share it, typically subagents of one parent — and a transcript mention picked this one, or nothing did. With `tier keyed`, the ledger names more than one agent for the session the event named. Somewhere to look, not an answer. | `1` |
| `NOT CONFIDENT` | A candidate scored below the bar. | `1` |
| a refusal: `no cwd match`, `no overlap` or `no keyed transcript` | No candidate at all. The reason tells you where to look next: the topology, the clock, or the transcript store. | `1` |

Below-bar candidates and leads are printed *with their evidence* and without
their transcript. Read the evidence line: it is the part of the output that
tells you when the score should not be trusted. [join.md](join.md) explains
each signal and rung; [limitations.md](limitations.md) explains when the tool is
wrong and how its output shows it.

## Where next

- **Looking up a flag, an exit code or a JSON field** → [cli.md](cli.md)
- **An answer surprised you** → [join.md](join.md), then
  [limitations.md](limitations.md)
- **A real case, end to end** → [worked-example.md](worked-example.md)
- **Everything else** → the [documentation index](README.md)
