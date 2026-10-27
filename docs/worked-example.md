# Worked example: recount's first testimony is about its own birth

> **This is a dated replay, not a live report.** Every command below was run
> against a 160-event ledger on 2026-08-19. The ledger has since grown to 177
> events and the last agent worktrees have been deleted, so the sweep now reads
> 118 refused / 56 at 0.65 / 3 at 0.95, and the `known:` worktree lists no
> longer reproduce. The *shape* of every case here is unchanged, and
> [join.md](join.md#recount-is-wrong-on-this-repositorys-own-ledger) carries the
> current numbers. Kept as-is deliberately: a replay that is quietly re-run
> every time the corpus moves is not a replay.
>
> **The verdicts have moved too, and not because the corpus did.** Two changes
> to the join on 2026-08-20 — the repo root names no one, and an answer the
> chain did not decide is a *lead* ([join.md](join.md#a-mention-decided-answer-is-a-lead-and-the-tool-says-so))
> — mean the current binary prints §1 and §2 at 0.65 as `LEAD — NOT
> CONFIDENT`, exit 1, with the transcript window withheld; it also words the
> topology and overlap evidence lines differently and adds a `keyed` one. The
> output in §1–§3 is what the binary printed on 2026-08-19; the prose notes
> where the code now disagrees.

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
      · time overlap: 66 turn(s) of this session fall inside the queried 2026-08-19T06:58:12Z..2026-08-19T07:08:12Z (the session covers 2026-08-19T07:00:48Z..2026-08-19T09:16:20Z)
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

**And today it is not 0.95 at all.** The reasoning above — one session at
that cwd, so the chain discriminated — is the exact mistake the join was
later corrected for. `main` is the repo root, every agent not in a worktree
shares it, and "exactly one session sits there" measures the corpus, not the
chain. The current code tests for the root before counting, scores it as a
filter (0.20), and calls the 0.65 that results a lead: exit 1, transcript
withheld. Here the lead happens to point at the right session. The rule
cannot know that, and §2 is why it does not try.

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
      · time overlap: 19 turn(s) of this session fall inside the queried 2026-08-19T08:34:18Z..2026-08-19T08:44:18Z (the session covers 2026-08-19T08:24:08Z..2026-08-19T08:36:37Z)
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

Exit 0. Confidence 0.65, above the 0.50 bar, printed as CONFIDENT. (The
current binary prints the same 0.65 as `LEAD — NOT CONFIDENT` and exits 1;
see the note at the top.)

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
not answers — and since 2026-08-20 the tool reads them that way itself.

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
- accept that once they are gone the join refuses those rows with `no cwd
  match`, and only the transcript mention survives — as a `recount grep` for
  the agent's name, which the join will not score on its own, because a
  mention never creates a match.

## The two topologies, and what each can reach

The join's confidence ladder, from `src/join.rs`:

| situation | confidence | verdict |
|---|---|---|
| chain singled the session out, + mention (topology A) | 0.95 | answer |
| chain singled it out, no mention | 0.70 | answer |
| chain only filtered, + mention (topology B) | 0.65 | lead |
| chain only filtered, no mention | 0.40 | lead |

Topology A is process-per-agent: each agent runs in its own worktree, its
session cwd names that worktree, and the spec's chain resolves. It can reach
0.70 on topology alone — an answer that stands without any transcript
wordplay.

Topology B is subagent-per-agent, which is how this fleet actually ran: one
Claude Code process, every subagent inheriting the parent's cwd. The chain
filters to the right *repo* and then stops. 0.65 is the ceiling, and reaching
it requires the mention tiebreaker — which is precisely the signal section 2
shows firing on a peer's boilerplate. Without a mention, topology B lands at
0.40, below the 0.50 threshold. Neither is a refusal: `at` still prints the
candidate and its evidence, withholds the transcript, and exits 1, because the
evidence is what tells the reader where to look next.

Section 2 is the honest reading of the gap between 0.65 and 0.70. When this
page was written, a confident match on topology B was one weak signal away
from a wrong one, and the evidence block was the only thing standing between
a reader and the arkanoid mistake. The verdict column is the fix that came
after: no score the chain did not decide is confident, so on topology B
nothing is — the 0.65 is still printed, as a lead.

## 4. The keyed tier, on this repository, on 2026-08-20

Everything above is v0.1.0's topological ladder, and every number in it is a
score. v0.2.0 adds a tier above the ladder: when pact stamped the attribution
chain on the event, the event *names* its own transcript and there is nothing
to score. Run against the main checkout — the copy inside a worktree is a
stale git checkout, so `--repo` must point at
`/home/chussenot/Documents/recount`:

```
$ recount explain --repo /home/chussenot/Documents/recount --event-line 207
recount explain — /home/chussenot/Documents/recount/.pact/events.jsonl line 207
  event    2026-08-19T17:20:53.482236042+00:00 · agent `joinkeyed`
  from     worktree `joinkeyed` — the join's first link
  about    src/join.rs
  detail   recount-1cw.5: the keyed tier above the topological ladder
  bead     recount-1cw.5 — also looked for in the transcript

recount at — /home/chussenot/Documents/recount
  agent    `joinkeyed`
  moment   2026-08-19T17:20:53Z · window ±5m each side → [2026-08-19T17:15:53Z … 2026-08-19T17:25:53Z]
  scanned  1 session dir(s) · 2 session(s) the window could not rule out

  chain    what pact stamped on the event itself
    harness           claude-code
    model             claude-opus-5 (declared — the PACT_MODEL the spawner asked for, which is
                      an intent, not an observation: it is not evidence that this
                      model answered)
    harness_session   cd7c75eb-c39f-4be4-92ae-29efda5d7d69
    harness_subagent  aaadf026f08b25971
    branch            wt/joinkeyed

  match           /home/chussenot/.claude/projects/-home-chussenot-Documents-recount/cd7c75eb-c39f-4be4-92ae-29efda5d7d69/subagents/agent-aaadf026f08b25971.jsonl
    tier keyed · no score, and none is missing: the event named this transcript · CONFIDENT
      · keyed: the event named its own transcript: harness_subagent `aaadf026f08b25971` under harness_session `cd7c75eb-c39f-4be4-92ae-29efda5d7d69` is agent-aaadf026f08b25971.jsonl, found at /home/chussenot/.claude/projects/-home-chussenot-Documents-recount/cd7c75eb-c39f-4be4-92ae-29efda5d7d69/subagents/agent-aaadf026f08b25971.jsonl by one path join and one stat. No candidate set, no score, no tie to break — this is an identity, and the topological ladder was not consulted
      · model: declared claude-opus-5, session records claude-opus-5 — they agree

  transcript window — 111 turn(s) in [2026-08-19T17:15:53Z … 2026-08-19T17:25:53Z]

    [2026-08-19T17:16:26Z] user (line 1)
      You are agent `joinkeyed` in a pact-coordinated fleet building recount v0.2.0. This is the bead the whole release turns on.

      WORKTREE: /home/chussenot/Documents/recount/wt/joinkeyed (branch wt/joinkeyed). `cd` there first; never touch the main checkout or another wt/*.
      [+5688 more characters — the full text is in --json]

    [2026-08-19T17:16:32Z] assistant (line 7)
      [tool_use Bash]
      {"command":"SD=$(ls -d \"${CLAUDE_CONFIG_DIR:-$HOME/.claude}\"/projects/*/\"$CLAUDE_CODE_SESSION_ID\"/subagents 2>/dev/null | head -1)\nM='PACT-SELFID-joinkeyed-2a71fd'\n…"}

    [2026-08-19T17:16:35Z] user (line 8)
      [tool_result]
      PACT_HARNESS_SUBAGENT=aaadf026f08b25971

[… 107 of the 111 turns elided here; the pasted turns are verbatim except
   the one marked `…` inside the tool_use JSON, and the elision is mine …]
```

Exit 0. Compare this with §1 and §2. There is no confidence number, because
there is nothing to be uncertain about: the event carries
`harness_subagent: aaadf026f08b25971`, that id is a filename under the
session directory the event also names, and one `stat` either finds it or
does not. The candidate set was never built. The transcript mention — the
signal that cast the deciding, wrong vote in §2 — was never consulted.

Note what the chain block does *not* claim. `model claude-opus-5 (declared …
it is not evidence that this model answered)` is the `PACT_MODEL` the
spawner asked for. The `model:` evidence line then reports that the session
file records the same value, which is a corroboration and is printed as one.

**The self-id handshake is the price.** The keyed tier only works because the
agent's first tool call wrote a literal marker into its own transcript and
grepped for it — that is line 7 above, and it is why line 8 exists. Nothing
in Claude Code hands a subagent its own transcript id. The chain is a
convention the fleet's prompts enforce, not a property of the harness. This
page used to say that a prompt which dropped the handshake sent its events
back to the ladder. It does worse: pact still stamps the *parent's* session id
on such an event, and the join keys a session-only row — as an identity, with
no score — to that session's top-level transcript, which is the parent's
([join.md](join.md#what-the-keyed-tier-does-not-fix)).

## 5. A clean audit, and why that is the interesting case

```
$ pact audit --json | recount testify --repo .
recount testify — /home/chussenot/Documents/recount
  source   stdin
  input    pact audit summary (no --check)
  scanned  1 session dir(s) · 46 session(s) · window ±300s each side
  findings 0 · testified 0 · untestified 0 · passed through 0

  nothing to testify: the audit found nothing. A clean audit is not a failure.
```

Exit 0. This repository's own coordination history has no double-win, no
stale hold, no chain break, no merge divergence, no silent contention and no
retry storm, so there is nothing for `testify` to annotate.

That is the empty-set contract working, not a demonstration that failed to
happen. The tempting implementations both exit 1 here — one because "no
findings were testified" reads as failure, one because an empty `findings`
array trips a "malformed document" guard — and both would put a red line in
a CI log for a repository whose coordination was clean. `testify` exits 1 for
exactly one reason: a finding that carried an agent and a moment and still
got no confident match. Zero findings cannot produce one.

For what `testify` looks like when the audit is *not* clean, see
[session-learnings.md](session-learnings.md), which runs it against four
older repositories on this machine — 2,456 ledger rows between them, and not
one of them keyed.
