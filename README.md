# recount

**Session testimony for pact audits.** A coordination ledger records *what* your
agents did. Their transcripts record *why*. recount joins the two, so a finding
in an audit can be answered with the agent's own reasoning instead of a guess
about it.

It reads [pact](https://github.com/chussenot/pact)'s `.pact/events.jsonl` and
your coding-agent session files. It owns no database, runs no daemon, and
touches no network.

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
invoked from, the path that worktree occupies, and the session whose working
directory resolves into it. A required time-window overlap narrows it, and only
then does a transcript mention break a remaining tie.

The ordering is the point. Topology is a fact about the filesystem; a mention is
a coincidence of text. Reaching for the coincidence first is how you match the
wrong session confidently. [docs/join.md](docs/join.md) has the full ladder.

### Every answer carries a confidence and its evidence, because the failure being prevented is a confident wrong one

recount could return the best-matching session and nothing else. It never does.
Every result reports what it scored and *which signals produced that score* —
and every refusal reports which of its two reasons applies, so "no transcript"
and "no overlap in time" can never be confused for one another.

This is not a UI preference. A tool built because an audit believed something
false must not be able to hand you a false belief with a clean face. The number
tells you how much to trust it; the evidence line is what lets you catch it
being wrong. It is designed to be caught, because on some inputs it *is* wrong —
see [Where recount is weak](#where-recount-is-weak), which is not a disclaimer
section but a design consequence.

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

Two properties do the work a cache would otherwise do, and neither can go stale:

**Resolution before reading.** A repository path resolves to the encoded session
directories of that repo and its worktrees before a single transcript is opened.
Most of the corpus is eliminated by knowing where to look.

**Append-only files have a usable mtime.** Sessions only ever grow, so a file's
modification time bounds its *last* turn. A file whose mtime precedes the query
window cannot contain anything inside it, and is skipped without being opened.

That argument is one-directional, and the asymmetry matters more than the
optimisation: mtime says nothing about a file's *first* turn, so the upper bound
of a window cannot prune. A symmetric implementation would be faster and would
silently lose hits. [docs/performance.md](docs/performance.md) carries the
measurements and the deferral they did not trigger.

### Two connectors, and the trait exists so a third stays cheap — not so you add one

The connector interface is two methods, deliberately small enough that a third
implementation would be one file. There is no plugin registry, no discovery
mechanism, no configuration naming connectors. Making the extension point cheap
is not the same as building for extension: a mechanism that makes a third
connector *easy* is a mechanism to maintain forever, in exchange for a file
somebody could have written anyway.

What the interface does buy is a rule about ownership. A file belongs to the
most specific connector that recognises it, and it belongs to that connector
*before* any query window is considered. That ordering was learned the hard way
and is explained in [docs/connectors.md](docs/connectors.md).

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

### Credit: cass

[cass](https://github.com/Dicklesworthstone/coding_agent_session_search) —
Dicklesworthstone's coding-agent session search — is the product-scale proof
that the underlying idea is worth having: ~685k lines, 25 connectors, SQLite +
Tantivy, embeddings, a TUI, sync, crypto. It is what you want if you want to
*search your agent history*, and it does that job properly.

recount implements the audit-testimony 3% of it and refuses the rest. The
refusals above are not a critique of cass; they are what remains when the
requirement narrows from "search everything" to "answer one audit finding with
one transcript, on a machine that owns no index". cass earned the general case.
recount only had to earn one join.

## Where recount is weak

### The join has a ceiling, and which ceiling depends on how your fleet runs

The topological key assumes an agent's session records the directory that agent
worked in. That holds when each agent is its own process started in its own
worktree. It does **not** hold when your agents are subagents of one session:
they all record the parent's working directory, so the chain filters the
candidates correctly and then cannot tell them apart.

recount detects this rather than papering over it, and caps what such a match
can score — a topology where the primary key cannot discriminate is not allowed
to produce a top-confidence answer, and a match resting on nothing but that
degenerate key scores *below* the confidence bar on purpose.
[docs/join.md](docs/join.md) has the rungs and the measurements.

### The tool is wrong on this repository's own ledger, and its own output shows it

Run recount against the coordination log of its own construction: of 177 events,
118 refuse, 3 resolve correctly at top confidence, and **56 resolve to a
*confident* match that names the wrong session** — every one of them a subagent's
transcript standing in for the orchestrator event that was actually asked about.

Two separate things produce that. The topology filtered without discriminating,
which is the ceiling working as designed. But among the candidates it could not
tell apart, the winner is decided by filesystem path order, and that ordering is
systematically biased against the session that was right — a defect
(bead `recount-gwx`; the case is walked in
[docs/join.md](docs/join.md#recount-is-wrong-on-this-repositorys-own-ledger)) rather than a limitation.

This is the arkanoid failure mode, reproduced by the tool built to prevent it.
It is documented rather than hidden because the machinery worked exactly as
designed — every one of those matches carries an evidence line stating that the
chain "filtered without discriminating", which is precisely the sentence that
tells a reader to treat it as a lead rather than an answer. A reader who takes
the number at face value is misled. A reader who reads one more line is not.
That is the whole argument for scoring with evidence, and
[docs/worked-example.md](docs/worked-example.md) walks the actual case.

### Keep your agent worktrees until the audit is done

If your fleet deletes each agent's worktree when its work lands, the topological
chain has nothing left to resolve: the ledger names a path that no longer
exists. recount refuses correctly in that situation — the worktree is genuinely
gone — but the refusals are unrecoverable, and they are most of this
repository's own ledger for exactly that reason.

Audit first, tidy second. Or accept that only the mention signal survives, with
the ceiling that implies.

### Known and deliberate

The `~/.claude` format notes are reverse-engineered from one machine and will
drift; [docs/format-notes.md](docs/format-notes.md) says so in those words and
carries its observation date. `grep` still parses each file once more than it
needs to, because removing that pass honestly requires a change to the connector
interface that has not been justified yet. And while the fallback connector is
reachable, in practice it only ever receives what the primary connector
declines, because no command accepts a bare directory.

### Deviations from the spec's literal text

[docs/spec.md](docs/spec.md) is committed verbatim as the contract this tool was
built against. Where the code departs from it, the departure is recorded here
rather than edited into the spec:

**`recount completion <shell>` is a sixth command, and the one that does not
take `--json`.** The spec's command list names five and says "all `--json`;
human digest second". `completion` writes a shell program to stdout, so a digest
beside it would be sourced as code. The alternative was checked-in completion
scripts, which are hand-written copies of a CLI and drift from it the first time
a flag changes.

**The connector's `discover` takes the scan scope as an argument.** The spec
writes it as taking nothing. But the spec's own cheapness clauses — resolve the
session directories "before any file opens", and prune outside the window
"unopened" — are unsatisfiable if the connector cannot learn the scope before it
opens anything. Still two methods plus `roots()`, so the spec's "a third is one
file" holds.

## Where this design came from

recount was built by a fleet of 26 coordinated agents against a specification
frozen before any code was written, and most of what is documented above is a
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
task rather than a habit.

The full account, including what the coordination audit could and could not
verify afterwards, is in the beads and in `.plan/checkpoints/`.

## Documentation

| Page | What it answers |
|---|---|
| [docs/cli.md](docs/cli.md) | Every command, flag, exit code and JSON shape — the contract |
| [docs/install.md](docs/install.md) | Installing, the task runner, shell completions |
| [docs/join.md](docs/join.md) | The topological key, the score ladder, refusals, the two topologies |
| [docs/connectors.md](docs/connectors.md) | The two connectors, the claim rule, declined vs damaged |
| [docs/format-notes.md](docs/format-notes.md) | The reverse-engineered `~/.claude` layout, and its drift warning |
| [docs/performance.md](docs/performance.md) | The mtime prune, the measured ceilings, the deferral |
| [docs/worked-example.md](docs/worked-example.md) | One real finding replayed end to end, including a wrong answer |
| [docs/development.md](docs/development.md) | Building, testing, and why each gate exists |
| [docs/spec.md](docs/spec.md) | The frozen contract. Never edited; deviations are recorded above |
