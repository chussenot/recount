# The join

The join answers one question: *which session was driving this identity at
this moment, and how sure are we?* It is implemented in `src/join.rs`, whose
module doc carries the same reasoning next to the arithmetic.

Everything below was checked against the code, or measured on this repository
on 2026-08-19 at `457a254`. Where a number is a measurement, its conditions are
stated with it; re-measure before quoting one, because the corpus grows and the
worktrees a fleet leaves behind get deleted.

## The key is topological

`invoked_from` → worktree path → session cwd.

A pact event records the worktree an agent was invoked from. `src/worktree.rs`
turns that name into a path (forward-encoding only — it never decodes a
directory name). A session states the cwd it ran in. Matching those is
structural: it does not depend on the agent having said its own name, on log
formatting, or on any guess about content. That is what makes the answer
defensible in an audit.

Three signals, in precedence order:

1. **Topology** — the chain above. Without it there is no match to score.
2. **Time overlap — required, not weighted.** A session whose turns do not
   overlap the event window is not a weak match; it is not a match at all,
   because it cannot have produced the event. No overlap short-circuits to a
   refusal before any score is computed.
3. **Transcript mention — the tiebreaker.** The `PACT_AGENT` name or a bead id
   appearing in the transcript separates sessions that already passed 1 and 2.
   It never creates a match on its own: an agent discussing `skeleton` is not
   `skeleton`, and a session the chain excluded is never scored however often
   it says the name.

## The two fleet topologies

**This is the most important thing on this page.** The same chain behaves
completely differently depending on how the fleet was run, and the difference
is not a detail of the implementation — it decides whether the key can answer
the question at all.

**Topology A, process-per-agent.** Each agent is its own `claude`, started in
its own worktree. The session's cwd *is* the worktree. The chain resolves
exactly as `docs/spec.md` describes, and it discriminates: it can pick one
session out of the field.

**Topology B, subagent-per-agent.** Every agent is a subagent of one Claude
Code session. `cwd` is the cwd of the session *process*, and a subagent is not
a process (`docs/format-notes.md` §4.3), so every subagent transcript records
the *parent's* cwd — the repo root. The chain then returns *every* session in
the repo for *every* worktree. It still filters correctly, but it cannot tell
the candidates apart.

Measured on this repository, over every `*.jsonl` under
`~/.claude/projects/-home-chussenot-Documents-recount` (29 files: 1 top-level
session, 28 subagent transcripts):

| `cwd` value | lines |
|---|---:|
| `/home/chussenot/Documents/recount` (the parent) | 3,592 |
| `/home/chussenot/Documents/recount/wt/diag` | 8 |
| `/home/chussenot/Documents/recount/wt/probe` | 4 |
| `/home/chussenot/Documents/recount/wt/w4tail` | 3 |

3,607 cwd-bearing lines; 3,592 name the parent. All 15 that name a worktree are
in the orchestrator's own top-level transcript, which went into those
directories itself. **Not one of the 28 subagent transcripts names the worktree
its agent was assigned**, against the 26 `wt/*` worktree names the ledger
records in `invoked_from` (27 distinct values, counting `main`).

That is the whole problem, in one table: the fleet this tool was built to audit
ran in the topology its primary key cannot resolve.

## The score ladder

Four fixed numbers, summed. They are reproduced here in full, because the
*relationships* between the rungs are the point — a single rung quoted alone
means nothing. The constants live in `src/join.rs`
(`TOPOLOGY_DISCRIMINATING`, `TOPOLOGY_FILTER`, `OVERLAP`, `MENTION`), and the
threshold is the single `CONFIDENT` constant in the same file.

| outcome | confidence |
|---|---:|
| chain singled the session out, + mention (topology A) | 0.95 |
| chain singled it out, no mention (topology A) | 0.70 |
| chain only filtered, + mention (topology B ceiling) | 0.65 |
| chain only filtered, no mention (topology B alone) | 0.40 |

Read it as three statements:

- Topology A clears the bar **on topology alone** (0.70). No transcript
  wordplay is needed for an answer that stands.
- 0.65 is the **ceiling** under topology B, and reaching it requires the
  mention. It clears the bar but is visibly not 0.70: a mention is testimony,
  not topology.
- 0.40 is **deliberately below** `CONFIDENT`. Under topology B a repo-level cwd
  plus a clock overlap is true of every agent in the fleet, and answering from
  it would be a confident conclusion drawn from a feature that cannot
  discriminate — the arkanoid-v1 misdiagnosis this tool exists to prevent. So
  "filtered but did not discriminate, and nothing else spoke" is a refusal, not
  a guess.

`at` applies the threshold; `join::resolve` does not. A below-bar candidate is
still *printed*, with its evidence and without its transcript, because the
evidence is what tells the auditor whether to look at the clock or at the
topology. See [cli.md](cli.md) for what each outcome prints and exits.

## How the degeneracy is detected

Not configured, and not inferred from the fleet's shape: counted, from the
data, after the overlap gate.

Among the surviving candidates, count those whose cwd resolves **exactly** to
the worktree `invoked_from` named (as opposed to an ancestor of it). That count
is the whole predicate:

- `exact == 1` — the chain singled a session out. Topology A, and the only case
  where topology discriminates.
- `exact == 0` — no candidate names the worktree; every one matched at the repo
  level. Topology B.
- `exact > 1` — several name it and the chain cannot separate them. An
  ambiguous topology-A tie.

Zero and many are both coin flips, not answers, so both score as a filter
(0.2) rather than as a discriminator (0.5), and the mention becomes decisive.
A fourth case exists for completeness: nothing named a worktree at all — no
`invoked_from`, and no worktree named after the agent — where the chain has
filtered to "sessions in this repo" and says so.

Every answer names the regime it was decided in, in its topology evidence line.
That sentence is not decoration; on topology B it is the difference between a
lead and an answer.

## The two refusals

Exactly two, because the spec names exactly two, and because an auditor needs
to know which question to ask next.

- **`no cwd match`** — the topology failed. No session's cwd resolved into the
  repo or the named worktree, or `invoked_from` names a worktree this repo does
  not have, or the worktree it names is gone from disk. *Go and look at the
  topology.*
- **`no overlap`** — the topology succeeded and the clock failed. Sessions
  matched the chain, but none has a turn inside the window. *Go and look at the
  clock.* The detail lists the sessions that matched and the span each covers,
  so the reader can widen `--window` with a number rather than a guess.

They are not interchangeable, and collapsing them was not a hypothetical:

> The mtime prune once made `at` name the wrong refusal reason. A session that
> ran yesterday is pruned before the join sees it (see
> [performance.md](performance.md) for why the prune is sound), so the join
> finds no candidate at all and refuses with `no cwd match` — sending the
> auditor to the topology when the answer was the clock.

The sessions a `no overlap` refusal must cite are exactly the ones the prune
removes. That is why `at` re-scans **unwindowed** when, and only when, the cheap
windowed pass refuses on topology. A match never needs the re-scan — overlap is
a hard gate, so a pruned session could not have won — so the cost falls on the
run that is about to explain itself, never on the run that has an answer. It is
about 312 ms on the synthetic bench corpus; [performance.md](performance.md)
has the conditions.

## recount is wrong on this repository's own ledger

Sweeping `explain` over every row of `.pact/events.jsonl`
(`recount explain --repo . --event-line N`, debug build at `457a254`,
2026-08-19, 177 events):

| outcome | events |
|---|---:|
| refused, `no cwd match` (exit 1) | 118 |
| match at 0.65 (exit 0) | 56 |
| match at 0.95 (exit 0) | 3 |

Of the 59 answers, **56 are wrong**, and all 56 are above the confidence bar.

Every match is for an event the orchestrator recorded (`invoked_from: main`),
because the orchestrator is the only identity whose worktree still exists. The
orchestrator's session is the top-level transcript
`cd7c75eb-….jsonl`. The 3 matches at 0.95 return it — those are the first three
events, recorded before any subagent had been spawned, when `exact == 1` held.
The other 56 return a *subagent's* transcript instead: every subagent shares the
orchestrator's cwd, so every candidate that survives the window is an exact cwd
match — ten of them for the event below — `exact > 1`, topology drops to a
filter, and the mention decides. Ties keep the first candidate offered;
candidates arrive in path order, and path ordering compares components, which
puts the `subagents/` directory ahead of its sibling top-level file. So the
answer goes to the alphabetically first subagent transcript that happens to say
the word — never to the orchestrator's own session, which mentions its own name
on line 7 and would have tied.

One of them. The command is `recount --json explain --repo . --event-line 60`;
the transcript window and the unchanged event fields are projected away with
`jq`, nothing is edited:

```json
{
  "line": 60,
  "agent": "orchestrator",
  "conf": 0.65,
  "sess": "agent-a0b9643f93682445f.jsonl",
  "evidence": [
    {
      "signal": "topology",
      "detail": "session cwd /home/chussenot/Documents/recount is worktree `main` — topology A (process-per-agent) but ambiguous: several candidate sessions name the same worktree, so the chain filtered without discriminating and the transcript mention decides"
    },
    {
      "signal": "time_overlap",
      "detail": "session covers 2026-08-19T07:41:47Z..2026-08-19T07:51:56Z and overlaps the queried 2026-08-19T07:33:15Z..2026-08-19T07:43:15Z"
    },
    {
      "signal": "transcript_mention",
      "detail": "`orchestrator` at line 1 (user): …lines>\"` Do NOT merge — the orchestrator lands your branch at the wave…"
    }
  ]
}
```

The event is the orchestrator taking the merge lock to merge `wt/connector`.
The transcript returned is `agent-a0b9643f93682445f.jsonl`, whose sidecar
describes it as *"W2 join engine bead"* — the `join` agent, a different agent
doing different work. Exit 0, printed as CONFIDENT.

**Its own output says so.** The topology line says "filtered without
discriminating … the transcript mention decides". The mention it decided on is
the word `orchestrator` at line 1 of a *peer's prompt*, in the sentence "Do NOT
merge — the orchestrator lands your branch at the wave barrier" — boilerplate
handed to every agent in the fleet. A signal true of every candidate cast the
deciding vote.

That is the point of the evidence block, and the reason `Match` cannot be
constructed without a confidence and a non-empty `Vec<Evidence>`. The bare
answer misleads. The answer plus its evidence does not. Read every 0.65 in this
ledger as a lead, not an answer.

[worked-example.md](worked-example.md) replays one such case end to end, with
the full terminal output and with the refusals analysed. Its distribution
(160 events: 108 / 49 / 3, three worktrees still alive) was measured earlier the
same day; the ledger has grown to 177 rows since and the last two worktrees have
been removed, which is why the refusal count above is higher. Both readings are
correct on the day they were taken — which is itself the finding: **a fleet that
tidies its worktrees as it goes destroys the join key for its own audit.** The
transcripts still exist, the ledger still exists; the link between them was in
the filesystem and was deleted.
