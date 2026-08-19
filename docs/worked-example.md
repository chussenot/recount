# Worked example: recount's first testimony is about its own birth

> **This is a dated replay, not a live report.** Every command below was run
> against a 160-event ledger on 2026-08-19. The ledger has since grown to 177
> events and the last agent worktrees have been deleted, so the sweep now reads
> 118 refused / 56 at 0.65 / 3 at 0.95, and the `known:` worktree lists no
> longer reproduce. The *shape* of every case here is unchanged, and
> [join.md](join.md#recount-is-wrong-on-this-repositorys-own-ledger) carries the
> current numbers. Kept as-is deliberately: a replay that is quietly re-run
> every time the corpus moves is not a replay.

Run for real on 2026-08-19 against this repository's own
`.pact/events.jsonl` (160 events) and this machine's `~/.claude/projects`
(839 sessions). Every command output below was executed and pasted. Where
output is cut, the cut is marked and counted — nothing is silently trimmed.

## Which branch of the spec

> one worked example replaying a real past finding from this machine's
> history if present, else the fixture.

The branch taken is **"if present"**. The presence check:

```
$ recount grep arkanoid --repo .
recount grep "arkanoid" — /home/chussenot/Documents/recount
  window   since unbounded · until unbounded
  scanned  1 session dir(s) · 25 session(s) · 2507 turn(s)
  hits     95
```

The arkanoid-v1 misdiagnosis that `docs/spec.md` names as recount's
motivating incident left traces on this machine, and this repo's own fleet
run left 25 sessions and 2507 turns of them. (The hit count climbs on every
rerun — this document is being written inside one of the sessions being
searched. An earlier run in the same session read 88, then 89. That is not
instability; it is an append-only corpus with no index, working exactly as
the spec asks.)

So the finding replayed here is a real one from this machine: the
coordination ledger of the fleet that built recount. The fixture replay
(`tests/join_fixture.rs`) stands as the else-branch and is not used here.

## 1. From an audit finding to the agent's own words

The question a person actually has, looking at a pact ledger: *the first
row of this ledger says agent `orchestrator` set a policy at 07:03:12 —
why?* `explain` takes the line number and answers it.

```
$ recount explain --repo . --event-line 1
recount explain — ./.pact/events.jsonl line 1
  event    2026-08-19T07:03:12.694912822+00:00 · agent `orchestrator`
  from     worktree `main` — the join's first link
  bead     none in detail — agent name only

recount at — /home/chussenot/Documents/recount
  agent    `orchestrator`
  moment   2026-08-19T07:03:12Z · window ±5m each side → [2026-08-19T06:58:12Z … 2026-08-19T07:08:12Z]
  scanned  1 session dir(s) · 1 session(s) the window could not rule out

  match           /home/chussenot/.claude/projects/-home-chussenot-Documents-recount/cd7c75eb-c39f-4be4-92ae-29efda5d7d69.jsonl
    confidence 0.95 · threshold 0.50 · CONFIDENT
      · topology: session cwd /home/chussenot/Documents/recount is worktree `main` — topology A (process-per-agent): the session cwd names the worktree, so the chain discriminates
      · time overlap: session covers 2026-08-19T07:00:48Z..2026-08-19T09:16:20Z and overlaps the queried 2026-08-19T06:58:12Z..2026-08-19T07:08:12Z
      · transcript mention: `orchestrator` at line 7 (user): …first) Run on an Opus-class orchestrator with the Task tool. Two-part…

  transcript window — 66 turn(s) in [2026-08-19T06:58:12Z … 2026-08-19T07:08:12Z]

    [2026-08-19T07:00:48Z] user (line 7)
      # PROMPT — build `recount` as a pact fleet (beads first)

      Run on an Opus-class orchestrator with the Task tool. Two-part input:
      Part A below is the product spec — commit it VERBATIM as docs/spec.md
      before anything else; it is the contract every bead cites. Part B is
      the run plan. The fleet's deliverables, in order: (1) a working
      recount, (2) the coordination telemetry, (3) a pact-findings note if
      the tooling got in the way.

      ═══════════════════════════════════════════════════════════════════
      PART A — docs/spec.md (commit as-is)
      ══════════════════════════════════════════════════════════════════…
      [+7469 more characters — the full text is in --json]

    [2026-08-19T07:00:56Z] assistant (line 19)
      [thinking]

    [2026-08-19T07:00:58Z] assistant (line 20)
      [tool_use Bash]
      {"command":"pwd && ls -la && echo \"--- git ---\" && git log --oneline -5 && echo \"--- tools ---\" && which pact bd cargo rustc 2>&1","description":"Inspect repo state and tooling"}

    [2026-08-19T07:00:59Z] assistant (line 22)
      [tool_use Bash]

[… 62 of the 66 turns elided here; the pasted window is verbatim, the
   elision is mine …]
```

Exit 0. The ledger says *what*: `orchestrator` set `commit-policy=per-task`
from worktree `main`. The transcript says *why*: turn 7 is the prompt that
created recount, and it demands a policy. The join needed no configuration
and no index — `--repo .` resolved to one encoded session directory, the
±5m window left one session the scan could not rule out, and the answer is
the first thing that agent was ever told.

This is the whole product in one command, and its first testimony happens to
be about its own birth.

**Why 0.95 and not 1.0.** Nothing here is certain. At 07:03 there was
exactly one session with that cwd, so topology discriminated rather than
merely filtered (0.50), the window overlapped (0.20), and the agent's name
appeared in the transcript (0.25). recount never claims certainty, because
it never has it: the join is inference over two artifacts neither of which
was written to be joined.

## 2. The same command, confidently wrong

Now a question from later in the same ledger: *row 122 says `orchestrator`
took the merge lock at 08:39:18 to merge `wt/cmdat` — what was it doing?*

```
$ recount explain --repo . --event-line 122
recount explain — ./.pact/events.jsonl line 122
  event    2026-08-19T08:39:18.048242748+00:00 · agent `orchestrator`
  from     worktree `main` — the join's first link
  about    .pact/internal/merge-to-master
  detail   merging wt/cmdat into master
  bead     none in detail — agent name only

recount at — /home/chussenot/Documents/recount
  agent    `orchestrator`
  moment   2026-08-19T08:39:18Z · window ±5m each side → [2026-08-19T08:34:18Z … 2026-08-19T08:44:18Z]
  scanned  1 session dir(s) · 4 session(s) the window could not rule out

  match           /home/chussenot/.claude/projects/-home-chussenot-Documents-recount/cd7c75eb-c39f-4be4-92ae-29efda5d7d69/subagents/agent-a0762be1f73c29e45.jsonl
    confidence 0.65 · threshold 0.50 · CONFIDENT
      · topology: session cwd /home/chussenot/Documents/recount is worktree `main` — topology A (process-per-agent) but ambiguous: several candidate sessions name the same worktree, so the chain filtered without discriminating and the transcript mention decides
      · time overlap: session covers 2026-08-19T08:24:08Z..2026-08-19T08:36:37Z and overlaps the queried 2026-08-19T08:34:18Z..2026-08-19T08:44:18Z
      · transcript mention: `orchestrator` at line 1 (user): …true>"` Do NOT merge — the orchestrator lands your branch at the wave…

  transcript window — 19 turn(s) in [2026-08-19T08:34:18Z … 2026-08-19T08:44:18Z]

    [2026-08-19T08:34:31Z] user (line 127)
      [tool_result]
      recount doctor — /home/chussenot/Documents/recount/wt/honesty

      connectors
        claude_code    831 sessions, 12 lines declined, 0 damaged
      …
      [+401 more characters — the full text is in --json]

    [2026-08-19T08:34:39Z] assistant (line 129)
      [tool_use Bash]
      {"command":"cd /home/chussenot/Documents/recount/wt/honesty && ./target/debug/recount --json doctor --repo . 2>/dev/null | jq '.connectors' && git status --short","description":"Doctor JSON and git status"}

[… 16 of the 19 turns elided; the doctor tool_result body is elided by
   recount itself, which prints the character count it withheld …]
```

Exit 0. Confidence 0.65, above the 0.50 bar, printed as CONFIDENT.

**It is wrong.** The event records the orchestrator holding the merge lock.
The transcript printed is the `honesty` agent's subagent session, running
`recount doctor` inside `wt/honesty` — a different agent doing different
work. A reader who takes the answer at face value walks away believing the
orchestrator was debugging the doctor command during a merge. It was not.

**Its own output says so.** Read the three evidence lines instead of the
number:

- *topology* — "filtered without discriminating … the transcript mention
  decides". Under this run's topology every agent is a subagent of one
  Claude Code process, so every session's cwd is `/home/chussenot/Documents/recount`.
  The chain's first link identified four candidates and could not choose
  among them.
- *time overlap* — the session ends at 08:36:37, before the 08:39:18 event.
  It overlaps the ±5m window only by its tail.
- *transcript mention* — the word `orchestrator` appears at line 1, in the
  *prompt* of a peer agent, in the sentence "Do NOT merge — the orchestrator
  lands your branch at the wave barrier". Every agent in this fleet was
  handed that sentence. It is not a signal about who this session is; it is
  boilerplate.

So a signal that is true of every agent in the fleet cast the deciding vote.
That is the arkanoid-v1 failure mode exactly — a confident conclusion drawn
from a feature that could not discriminate — reproduced live by the tool
built to prevent it.

The two readings are the point. The bare answer misleads; the answer plus
its evidence does not. This is what `docs/spec.md` means by "always reports
confidence and evidence, never a bare answer": the evidence line is not
decoration on the number, it is the part a reader can check. A tool that had
printed only the session path would have made the arkanoid mistake with no
way for anyone downstream to catch it.

Every one of the 49 matches at 0.65 in this ledger carries the same
"filtered without discriminating" line. They should all be read as leads,
not answers.

## 3. Why 108 of 160 events refuse, and whose fault that is

Sweeping `explain` over the whole ledger:

| outcome | events |
|---|---|
| refused, `no cwd match` (exit 1) | 108 |
| match at 0.65 (exit 0) | 49 |
| match at 0.95 (exit 0) | 3 |

Two thirds of this ledger refuses, and every refusal has the same reason.
A representative one:

```
$ recount explain --repo . --event-line 133
recount explain — ./.pact/events.jsonl line 133
  event    2026-08-19T08:45:48.747459802+00:00 · agent `cmdexplain`
  from     worktree `cmdexplain` — the join's first link
  about    src/cmd/explain.rs
  detail   recount-hwk.36: explain, from finding to testimony
  bead     recount-hwk.36 — also looked for in the transcript

recount at — /home/chussenot/Documents/recount
  agent    `cmdexplain`
  moment   2026-08-19T08:45:48Z · window ±5m each side → [2026-08-19T08:40:48Z … 2026-08-19T08:50:48Z]
  scanned  1 session dir(s) · 25 session(s) the window could not rule out

  no confident match — reason: no cwd match
    invoked_from `cmdexplain` names no worktree of this repo (known: `main`, `grepperf`, `worked`)
  no candidate to show: this is a structural refusal, not a weak match. Look at
  the topology.
```

Exit 1. Note what refuses and what does not: the 52 matches are the
orchestrator's own rows, which carry `invoked_from: main`. 106 of the 108
refusals are rows that carry a bead — the actual work of the fleet; the other
two are a `probe` agent's warm-up. `cmdexplain`, `skeleton`, `ci`, `join`,
`fixtures`, `honesty`, `bench`: the ledger names their worktrees, and those
worktrees no longer exist. The orchestrator removed each one at its wave
barrier. `doctor` agrees — it lists three worktrees alive, `main`,
`grepperf`, `worked`, against a ledger whose `invoked_from` field carries 24
distinct names.

**The refusal is correct.** recount is being asked to resolve
`cmdexplain` → worktree path → session cwd, and the middle term is gone from
disk. Guessing would be the arkanoid mistake again. Refusing with the reason
and the list of worktrees it does know is the honest answer, and it is the
answer that tells you what to fix.

**This is a finding about how the fleet was run, not a recount bug.** A fleet
that tidies its worktrees as it goes destroys the join key for its own audit.
The transcripts still exist; the ledger still exists; the link between them
was in the filesystem and was deleted. Two ways out, and a team has to pick
one before the run, not after:

- keep agent worktrees until the audit is done, and delete them afterwards; or
- accept that once they are gone only the transcript-mention signal survives,
  which on its own scores 0.40 and is deliberately below the bar.

## The two topologies, and what each can reach

The join's confidence ladder, from `src/join.rs`:

| situation | confidence |
|---|---|
| chain singled the session out, + mention (topology A) | 0.95 |
| chain singled it out, no mention | 0.70 |
| chain only filtered, + mention (topology B) | 0.65 |
| chain only filtered, no mention | 0.40 |

Topology A is process-per-agent: each agent runs in its own worktree, its
session cwd names that worktree, and the spec's chain resolves. It can reach
0.70 on topology alone — an answer that stands without any transcript
wordplay.

Topology B is subagent-per-agent, which is how this fleet actually ran: one
Claude Code process, every subagent inheriting the parent's cwd. The chain
filters to the right *repo* and then stops. 0.65 is the ceiling, and reaching
it requires the mention tiebreaker — which is precisely the signal section 2
shows firing on a peer's boilerplate. Without a mention, topology B lands at
0.40, below the 0.50 threshold, and `at` refuses. That threshold is set where
it is so that "filtered but did not discriminate, and nothing else spoke" is
a refusal rather than a guess.

Section 2 is the honest reading of the gap between 0.65 and 0.70: on topology
B, a confident match is one weak signal away from a wrong one, and the
evidence block is the only thing standing between a reader and the arkanoid
mistake.
