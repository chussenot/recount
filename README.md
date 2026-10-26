# recount

**Session testimony for pact audits.** A coordination ledger records *what* your
agents did. Their transcripts record *why*. recount joins the two, so a finding
in an audit can be answered with the agent's own reasoning instead of a guess
about it.

It reads [pact](https://github.com/chussenot/pact)'s `.pact/events.jsonl` and
your coding-agent session files. It owns no database, runs no daemon, and
touches no network.

**New here?** [docs/quickstart.md](docs/quickstart.md) gets you from a pact
audit finding to a transcript in five commands. Everything else — the CLI
contract, how the join scores, installing, contributing — is indexed in
[docs/](docs/README.md). This page only explains why the tool exists and why it
is shaped the way it is.

## The problem

An audit of a multi-agent run can see everything except the only thing that
explains it.

The ledger is complete and useless on its own: agent `skeleton` held
`src/model.rs` from 07:13 to 07:24, agent `join` merged at 08:39. Every row is
true. None of them says why anyone did anything. So the reader infers motive
from artifacts — from what the diff touched, from what the timestamps imply —
and inference from artifacts is exactly how you get a confident wrong answer.

The motivating incident is the arkanoid-v1 misdiagnosis (`arkanoid`'s
`docs/pact-audit.md`), where an audit reasoned from the shape of the changes to
a conclusion about intent, and the conclusion was wrong. The agent had written
down what it was doing, in its own words, at the time. Nobody could get at it,
because nothing connected a row in the ledger to a transcript on disk.

That connection is the whole tool. Everything else here is a consequence of
making it cheap and making it honest.

## Why recount is shaped this way

### The join key is topological, because a name is not an identity

Two agents in one run share a git identity, a clock and often a prompt. Their
names are strings that appear in each other's transcripts constantly — an agent
quoting a peer's assignment is not that peer. So the join does not start from
names. It starts from *where the work happened*: the worktree an event was
invoked from, and the session whose working directory resolves into it. A
required time-window overlap narrows the candidates, and only then does a
transcript mention break a remaining tie.

The ordering is the point. Topology is a fact about the filesystem; a mention is
a coincidence of text. Reaching for the coincidence first is how you match the
wrong session confidently. And where the ledger can *name* the transcript
outright — pact stamps the harness session on the events it writes — recount
takes that name and scores nothing at all, because an identity read off the
record is not a guess. [docs/join.md](docs/join.md) has both tiers and the full
ladder.

### Every answer carries a confidence and its evidence, because the failure being prevented is a confident wrong one

recount could return the best-matching session and nothing else. It never does.
Every result reports what it scored and *which signals produced that score* —
and every refusal reports which of its two reasons applies, so "no transcript"
and "no overlap in time" can never be confused for one another.

This is not a UI preference. A tool built because an audit believed something
false must not be able to hand you a false belief with a clean face. The number
tells you how much to trust it; the evidence line is what lets you catch it
being wrong. It is designed to be caught, because on some inputs it *is* wrong —
see [the limits below](#it-is-honest-about-where-it-is-weak), which are a design
consequence rather than a disclaimer.

### There is no database, because a store you own can drift from sources you do not

Session files belong to your coding agent. It appends to them, rotates them, and
deletes them on its own schedule — this machine's corpus lost 14 files and
gained others in under three hours while recount was being built. Any index
recount kept would be a second, staler account of files it does not control,
and the first time the two disagreed the reader would have no way to know which
one lied.

So there is no store. Session files are the only truth, every run reads them
fresh, and a stale answer is structurally impossible. The cost is paid at query
time, which is why the next choice exists.

### Scans are made cheap structurally, not by caching

Two properties do the work a cache would otherwise do, and neither can go stale.
A repository path resolves to the session directories of that repo and its
worktrees *before* a single transcript is opened, so most of the corpus is
eliminated by knowing where to look. And because sessions only ever grow, a
file's modification time bounds its *last* turn: a file whose mtime precedes
the query window is skipped without being opened.

That second argument is one-directional, and the asymmetry matters more than the
optimisation: mtime says nothing about a file's *first* turn, so the upper bound
of a window cannot prune. A symmetric implementation would be faster and would
silently lose hits. [docs/performance.md](docs/performance.md) has the
measurements.

### Two connectors, and the seam exists so a third stays cheap — not so you add one

recount reads Claude Code sessions, plus any directory of plain JSONL on a
best-effort basis. The connector interface is small enough that a third would be
one file, and there is deliberately no plugin registry, no discovery mechanism
and no configuration naming connectors. Making an extension point cheap is not
the same as building for extension: a mechanism that makes a third connector
*easy* is a mechanism to maintain forever, in exchange for a file somebody could
have written anyway. [docs/connectors.md](docs/connectors.md) has the interface
and the ownership rule it enforces.

## What recount deliberately is not

No TUI. No daemon. No network access of any kind. No embeddings, no semantic
search, no ranking model. **No fuzzy search** — matching is case-insensitive
substring and nothing else, so there is no regex flag to add and no
almost-matched result to explain. One connector family at launch. And **no
persistence**: recount owns no store that can drift from sources it does not
own.

These are refusals, not gaps. Each one is a thing that would have to be kept
correct, kept in step with the coding agents' own formats, and kept honest about
its own staleness. The tool's job is to answer one audit question well on a
machine that owns no index; every capability above is load-bearing for a
different job.

## It is honest about where it is weak

The topological key assumes each agent's session records the directory that
agent worked in. When your agents are *subagents of one session*, they all
record the parent's directory, and the key filters the candidates correctly and
then cannot tell them apart. recount detects that, and an answer the key could
not single out is never reported as confident, whatever it scores: it is printed
as a **lead**, with its evidence, and exits `1`.

That rule exists because the tool failed its own audit first. Run against the
ledger of its own construction, it named the wrong session at a confident score
for 56 events. Every one of those answers already carried an evidence line
saying the chain "filtered without discriminating" — the sentence that tells a
reader to treat it as somewhere to look rather than an answer. A reader who took
the number at face value was misled; a reader who read one more line was not.
The tool now says it for them. It still returns the same wrong transcript, but
it no longer stands behind it, and that is the whole argument for scoring with
evidence: [docs/limitations.md](docs/limitations.md) has the numbers, the open
defect behind them, and what your fleet can do about it.

## Where the design came from

recount was built by a fleet of 26 coordinated agents against a specification
frozen before any code was written ([docs/spec.md](docs/spec.md)), and most of
what is above is a finding rather than a plan — usually one made by pointing the
tool at its own repository. [docs/architecture.md](docs/architecture.md) records
the findings, and the two places the code deliberately departs from the spec.
