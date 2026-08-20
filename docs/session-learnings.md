# Session learnings: what the transcripts said, when the ledgers were asked

recount exists to answer *why did this agent do that?* This document is the
first time the question has been asked in anger, of somebody other than
recount's own fleet. Four repositories on this machine carry a committed
`.pact/events.jsonl` and had surviving Claude Code sessions on 2026-08-20:
`pact`, `millrace`, `quern`, `arkanoid`. Everything below was produced by
`recount doctor`, `pact audit --check <c> --json | recount testify`, and
`recount at`/`grep`, plus direct reads of the transcript files recount named.

**How to read a finding.** Each one is: the question, the testimony trimmed
and quoted, the tier the testimony arrived at, and — the part that matters —
what it supports or **kills**. Several of the questions this document was
commissioned to answer had a plausible, tidy answer already written down.
Three of them are wrong, and the transcripts say so.

**Two words used precisely throughout.** *SHOWS* is what is literally in the
transcript. *INFER* is my reading of it. They are never mixed inside a
sentence.

---

## 0. What could be joined at all

`recount doctor --repo <p>`, 2026-08-20:

| repo | ledger rows | ledger days | session dirs found | sessions | verdict |
|---|---|---|---|---|---|
| pact | 729 | 2026-07-31 … 2026-08-18 | 1 | 254 | READY |
| millrace | 890 | 2026-08-15 … 2026-08-16 | 1 | 96 | READY |
| quern | 654 | 2026-08-13 … 2026-08-14 | 1 | 62 | READY |
| arkanoid | 183 | 2026-08-08 … 2026-08-17 | 1 | 42 | READY |

**Not one of those 2,456 rows carries an attribution chain.**

```
$ jq -r 'select(.harness_session!=null)|1' <repo>/.pact/events.jsonl | wc -l
pact 0 · millrace 0 · quern 0 · arkanoid 0        (recount's own ledger: 101 of 278)
```

pact stamps `harness_session`/`harness_subagent` from 0.11.0. These ledgers
were written by 0.7.4 through 0.10.7. Every answer below is therefore
**INFERRED by the topological ladder** — there is no keyed tier available in
any of them, there never will be, and no id is missing from those rows: they
are old, not defective. Section 6 measures how good the inference actually
is, and the number is bad.

---

## 1. Why did agents die refusing red masters?

**The premise is wrong, and this is the most useful thing in the document.**

The brief says: *the standing annex says a red master is never a reason to
hold a proven merge; did they read it, or reason about it and decide
otherwise?* Neither. At the moment they decided, the annex said the opposite.

### What the transcripts SHOW

`millrace`, run 2, 2026-08-15 evening. Four agents ended their session parked
rather than merging. `gudgeon`, last words, 20:35:02Z:

> Waiting on hopper to release `src/front/parser.rs` (their fix for
> millrace-e23 is in progress; master is red for reasons unrelated to my
> changes). […]
>
> Holding off on `pact merge` for both until master goes green, since
> merging onto a master that's already red for a different reason would get
> reverted and muddy the audit trail. Will resume as soon as the parser.rs
> lease frees up.

`damsel`, 20:34:46Z and 20:34:58Z (two separate turns, both verbatim):

> Merge mutex watch and a background monitor are running while master is red
> due to two in-flight sabotage fixes (hopper on `src/front/parser.rs`,
> gudgeon on `src/vm/machine.rs`). I'll wait for that to clear before merging
> the AGENTS.md doc work (millrace-9eg, millrace-cdz, millrace-765).

> Monitor and merge-mutex watch are active. I'll pause here — the
> notification will arrive automatically when master clears […]

`bedstone`, 20:43:42Z and its final turn 20:43:45Z:

> Waiting for both `src/front/parser.rs` (hopper, plus the newly-discovered
> `level+2` bug I flagged) and `src/vm/machine.rs` (gudgeon) to clear so
> master goes green — then I can merge `wt/bedstone-millrace-rra` and finish
> verifying the `t7k`/`2hs` corpus cases.

> Standing by for the monitor notification.

`pitwheel`, 20:55:01Z:

> I've completed four independent fixes while master was red from stacked
> saboteur regressions, and I'm now waiting on a background watch for
> `src/vm/opcode.rs` to free up (held by sluicegate fixing a separate P1)
> before attempting any merges — merging now would falsely go red due to
> their unrelated unfixed bug. […] I'll continue once the background watch
> notifies me that `src/vm/opcode.rs` is released, then merge each of the
> four branches and close the corresponding beads.

Now the annex they were told to "follow as written". `git show 5f2c728:AGENTS.md`
is the version live at 20:35Z (committed 2026-08-15 22:34:16 +0200 = 20:34:16Z;
the next AGENTS.md commit is 21:13:53Z):

> - **Red** → it reverts the merge and **deliberately keeps the mutex**, and
>   says so. A peer merging onto a branch that has just failed its own oracle
>   would bury the cause. Fix master or file a bead describing what your
>   change broke, then release by hand with the command it prints. A red
>   master is the one thing here that is everyone's emergency.

And their spawn prompt, verbatim, for all of `gudgeon`, `damsel`, `hopper`,
`bedstone` (identical text in all four):

> MERGING is one command now: `pact merge <branch> --verify 'cargo test
> --release'`. […] If verification fails it reverts and DELIBERATELY keeps
> the mutex, and tells you what to do; follow that.
>
> A saboteur is planting real regressions on master while you work and peers
> are merging concurrently. Master may be red for reasons that are not you.
> Check before concluding your change broke it.

The paragraph that would have corrected them exists — but only in the *next*
cohort's prompt. `spillway`, spawned 20:48:43Z:

> There was a bug in `pact merge` itself, now fixed (pact 0.10.3, installed):
> its verification used to ask "is master green after my merge?", which meant
> nobody could merge anything while master was red for someone else's reason
> — including the very fixes that would make it green. It now asks "did MY
> merge add a failure?" and will LAND a merge that adds no new failure onto
> an already-red master […]

The rule the brief quotes — "A RED MASTER IS NEVER A REASON TO HOLD YOUR
MERGE" — is `git log` commit `1cd0231`, *2026-08-15 23:13:53 +0200*, i.e.
21:13:53Z. That is **38 minutes after gudgeon parked** and it was written
because they did.

**Tier: INFERRED (topological), but the documentary half is exact.** The
transcripts were located through the `treadle` session directory (see §5);
the annex versions and their timestamps come from git, not from inference.

### What this kills, and what it supports

- **KILLS "they did not read the annex."** All four cite the exact mechanism
  the annex describes (revert, keep mutex, red is everyone's emergency).
  gudgeon names the consequence — "would get reverted" — which is precisely
  what the annex and their prompt told them happens.
- **KILLS "they reasoned about the rule and decided otherwise."** The rule
  did not exist yet. Their behaviour is compliance, not defiance.
- **SUPPORTS: a protocol correction that lives only in the spawn prompt does
  not reach agents already running, and a per-repo annex is not where a
  cross-cutting merge rule belongs.** The 0.10.3 semantics change was
  communicated by editing the prompt of the next cohort. The four agents in
  flight never saw it and could not have.
- **INFER: the failure is not agent judgement, it is that "verification
  failed → revert" and "master was already failing → land anyway" are the
  same command with the same name, and only the release notes distinguish
  them.** An agent reading `pact merge --help` at 20:35Z had no way to tell
  which semantics it was about to get.

---

## 2. Parking is not death. It is indefinite suspension with a human wake-up.

This is the finding I did not expect and it reframes every stale-hold in the
millrace ledger.

### What the transcripts SHOW

Seven of the twelve maintenance agents in millrace run 2 ended a turn with a
sentence of the form "I'll resume when notified". Measured from turn
timestamps:

| agent | last turn before parking | next turn | gap |
|---|---|---|---|
| gudgeon | 2026-08-15T20:35:02Z | — | never resumed |
| damsel | 2026-08-15T20:34:58Z | — | never resumed |
| bedstone | 2026-08-15T20:43:45Z | — | never resumed |
| pitwheel | 2026-08-15T20:55:01Z | — | never resumed |
| hopper | 2026-08-15T20:30:42Z | 2026-08-16T06:01:29Z | **9h30m** |
| penstock | 2026-08-15T20:53:55Z | 2026-08-16T06:01:39Z | **9h07m** |
| headrace | 2026-08-15T21:16:41Z | 2026-08-16T06:01:43Z | **8h45m** |

The three that resumed all resumed within 14 seconds of each other, at
06:01:29 / 06:01:39 / 06:01:43 the next morning. INFER: nothing woke them
individually; a human resumed the parent Claude Code session and every parked
child flushed at once.

What they were waiting on, verbatim from the tool results:

> watching src/vm/machine.rs — you will be sent a diff when a holder releases it

> Monitor started (task b4lvyt1gz, timeout 1200000ms). You will be notified on
> each event. **Keep working — do not poll or sleep.** Events may arrive while
> you are waiting for the user — an event is not their reply.

### What this kills, and what it supports

- **KILLS "the agents crashed" / "the agents ran out of context."** They did
  not fail. They executed a documented wait and the wait had no terminator.
  A subagent's process *is* its turn loop; ending the turn to wait is
  functionally the same as exiting, and neither `pact watch` nor a background
  `Monitor` can re-enter a subagent that has stopped producing turns.
- **SUPPORTS: every stale-hold and merge-divergence in this run has one
  cause.** The default lease TTL is 2700s. gudgeon parked at 20:35 holding
  `src/vm/machine.rs`; the lease expired at ~21:20 with the agent still
  nominally alive and nobody home. `pact audit --check stale-holds` reports 8
  and `--check merge-divergence` reports 9 for millrace; the transcripts say
  those are not eight and nine separate lapses, they are one design gap
  counted twice.
- **SUPPORTS: pitwheel's park cost four finished, tested, committed fixes**,
  including two stacked saboteur regressions it had already repaired and a
  peer-recovery takeover of `parser.rs` from a stalled hopper. All of it sat
  in worktrees.
- **INFER, and this is the uncomfortable one: pact's advice is correct for a
  process and wrong for a subagent.** "Subscribe and pick up other ready
  work" assumes something will still be running to receive the notice. §3 is
  the same coin's other face.

---

## 3. Why did refusal storms spin instead of backing off?

Neither of the two tidy answers. The agent knew the TTL, saw the payload,
subscribed as told — and polled anyway, for a reason the payload made easy.

### What the transcripts SHOW

`millrace` `retry-storm` reports three storms, all on the merge mutex; the
largest is `tailrace`, 4 refusals, min gap 16s, `median_holder_remaining_secs`
2643. Here is the whole of it, from the `treadle` session's
`agent-a1a724549638348da.jsonl`.

First refusal, 21:05:04Z, tool result verbatim:

> note: `pact watch add .pact/internal/merge-to-master` and pact will tell you
> when spillway releases it, instead of you asking again
> error: lease on .pact/internal/merge-to-master is held by spillway on branch
> master in worktree millrace (33s old, 2667s remaining); use --steal to override

tailrace subscribed six seconds later, at 21:05:10Z:

> watching .pact/internal/merge-to-master — you will be sent word that it is
> free when a holder releases it

Then checked its inbox (`inbox empty`) — and at 21:05:24Z said, verbatim:

> Let's retry the merge now — spillway's hold should be nearly done.

Second refusal, 21:05:28Z, 24 seconds after the first:

> note: you already watch .pact/internal/merge-to-master — pact will send you
> the diff when spillway releases it (2643s left on their lease). Pick up
> other ready work; **do NOT poll for this path**
> error: lease on .pact/internal/merge-to-master is held by spillway […]
> (57s old, 2643s remaining); use --steal to override

After that tailrace *did* pick up other work — `bd show millrace-98u`,
`millrace-4t9`, then a full second bead implemented and committed. It
returned at 21:17:08Z, was refused again (`3s old, 1797s remaining`),
subscribed again, and then ran

> ./tools/pw pact lease ls 2>&1 | grep merge-to-master

which SHOWS the mutex had changed hands (`spillway 4s active … merging
wt/penstock-millrace-dh5`). It retried immediately — fourth refusal at
21:17:27Z, `0s old, 1800s remaining`. It repeated the `lease ls` probe once
more, saw `millpond 5s active`, retried at 21:17:56Z and this time the merge
ran.

The protocol block those agents read already contains the measurement:

> Measured on one fleet: an agent retried every 15 seconds, 33 times, against
> a median 355 seconds of remaining hold whose note said it would renew — and
> **24 refusals in that run came from agents that had ALREADY subscribed and
> polled anyway.**

### What this kills, and what it supports

- **KILLS "the agent did not see the refusal payload."** It read the payload,
  acted on its `note:` line within six seconds, and quoted the situation back
  correctly.
- **KILLS "the agent did not know the holder's TTL."** The number was on
  screen. Twice.
- **KILLS "the agent was looping deliberately."** There is no loop. There are
  four refusals across twelve minutes, each preceded by a fresh observation,
  with a complete bead implemented in between.
- **SUPPORTS: the payload's two numbers are read in the wrong order.**
  `(33s old, 2667s remaining)` produced "spillway's hold should be nearly
  done" 20 seconds later. The salient number is the age. The decision number
  is the remainder, and it sits second, in parentheses, in smaller
  grammatical weight.
- **SUPPORTS: `watch` is a subscription with no in-turn delivery, so polling
  is the only observation channel an agent has.** tailrace's inbox was empty
  every time it looked; the mutex was never free at any instant it observed;
  the two retries that mattered were triggered by `pact lease ls`, i.e. by
  polling, which is the thing the payload forbids. The advice and the only
  workable behaviour are in direct conflict — and §2 shows what happens to
  the agents that take the advice.
- **INFER: "do NOT poll" is unenforceable advice with no supported
  alternative, and pact's own protocol block has already measured it failing
  once. It failed again here.**

### The same refusal, working perfectly

Worth recording because it is the same mechanism succeeding. At 21:14:09Z
tailrace was refused `.gitignore`:

> error: lease on .gitignore is held by headrace on branch master in worktree
> millrace (15s old, 2685s remaining); use --steal to override

It ran `pact lease ls | grep gitignore`, read headrace's **note**, and closed
its own freshly-created duplicate bead:

> ✓ Closed millrace-kam — […] already satisfied: headrace is doing this exact
> fix as millrace-pb8 (lease note: 'gitignore wt/ and untrack the 28 stale
> worktree gitlinks'), discovered while trying to lease .gitignore for the
> same purpose. Duplicate, closing in their favor.

**SUPPORTS: the `--note` field is the highest-value part of a lease.** It is
what turned a refusal into avoided duplicate work in under 15 seconds. The
number in the refusal caused a wasted retry; the note in the refusal
prevented a wasted bead.

---

## 4. Why were claim steps skipped before the protocol sync?

**They were not skipped. The check is measuring the wrong thing.**

### What the transcript SHOWS

`pact audit --check claim-lease-divergence` reports 7 findings for millrace.
Four of them are `bedstone`, e.g.:

```json
{"path":"tests/conform/343-assign-global-from-loop-in-fn.tr","agent":"bedstone",
 "bead":"millrace-t7k","assignee":"tailrace",
 "acquired_at":"2026-08-15T20:32:33.554931414+00:00","line":75}
```

Read as written: bedstone leased a file for a bead assigned to tailrace, so
bedstone leased without claiming. The transcript, verbatim tool calls, in
order:

> **20:31:44.457Z** `pw pact lease ls 2>&1; pw bd update millrace-t7k --claim`

> **20:32:31.848Z** `pw pact lease acquire tests/conform/343-assign-global-from-loop-in-fn.tr tests/conform/344-… tests/conform/345-… --note "millrace-t7k: corpus gap, assignment reaching outward from nested scopes (global/local/shadowed)"`

bedstone checked leases, claimed the bead, and leased 47 seconds later — the
protocol's exact order. It then parked (§2) without merging. Later, at
21:16:42Z, tailrace recovered the abandoned work:

> git commit --trailer Pact-Agent=tailrace -m "test(conform): pin assignment
> reaching outward from nested loop scopes […] Recovers millrace-t7k,
> abandoned by bedstone (see millrace-9xr)."

The `assignee` field the audit compares against is the bead's assignee **now**
— after tailrace re-claimed it. The lease event is history; the bead is
present tense. The two are being joined across time and the check reports the
difference as a protocol violation by an agent that did nothing wrong.

The same shape explains the other three: `hopper`/`millrace-e23` shows
assignee `pitwheel`, and `pact lease ls` at 21:04:11Z SHOWS pitwheel's note —
"hopper lease SUSPECT quiet 22m+, taking o…" — i.e. a documented peer
recovery. `pitwheel`/`millrace-af7` shows assignee `sluicegate`, and
pitwheel's own final report SHOWS it "released and reassigned `millrace-af7`
back to sluicegate after a lease-bookkeeping race".

### What this kills

- **KILLS the question as asked.** Ignorance, ambiguity and deliberateness
  are all wrong: 7 of 7 millrace `claim_divergences` are, on the evidence,
  correct behaviour followed by a peer recovery that the check then
  retroactively reinterprets.
- **SUPPORTS: `claim-lease-divergence` needs assignee-at-lease-time.** bd
  writes `.beads/interactions.jsonl` (the run enabled it explicitly:
  `export BD_AUDIT_ENABLED=1`, with the comment "Without it
  `.beads/interactions.jsonl` is never written and `pact audit --check
  claim-lease-divergence` passes in silence"). The as-of assignee is
  recoverable; the check is not using it.
- **Confidence note: this is 4 of 7 proven from transcript and 3 of 7 INFERRED
  from lease notes and a final report.** I did not check every one against a
  primary claim command. The finding graduates because the mechanism is
  proven and the remaining three fit it exactly, not because all seven were
  individually verified.

---

## 5. The transcripts existed. recount could not see them.

### What SHOWS

`recount testify --repo /home/chussenot/Documents/millrace` **refuses 57 of
the 65 millrace findings**, every one with the same structural reason:

```
  no confident match — reason: no overlap
    96 session(s) matched no invoked_from and no worktree named after agent
    `tailrace` (known: `main`); candidates are every session in the repo, but
    none has a turn in 2026-08-15T21:04:25Z..2026-08-15T21:14:25Z
```

535 of millrace's 890 ledger rows are dated 2026-08-15. Every session file
under `-home-chussenot-Documents-millrace` is dated 2026-08-16. The 08-15
transcripts are not deleted. They are here:

```
/home/chussenot/.claude/projects/-home-chussenot-Documents-treadle/
  2eaec220-d67f-4470-918d-316d85e5b9f4/subagents/agent-*.jsonl   (19 files)
```

Nineteen subagent transcripts, each opening `You are an autonomous maintenance
agent in a fleet working on the millrace repository at
/home/chussenot/Documents/millrace` and instructed to `cd` there as their
first act. The parent Claude Code process was launched from
`/home/chussenot/Documents/treadle`, a *different* repository, and Claude Code
files a session by the cwd at launch. Point recount at treadle and the join
works:

```
$ recount at --repo /home/chussenot/Documents/treadle --agent tailrace --ts 2026-08-15T21:09:25Z
  match  …/-home-chussenot-Documents-treadle/2eaec220-…/subagents/agent-a065464fe9fc00c7d.jsonl
    tier topological · confidence 0.65 · threshold 0.50 · CONFIDENT
      · topology: session cwd /home/chussenot/Documents/treadle resolves to worktree
        `main`, an ancestor: repo-level match only — topology unnamed
```

(That particular answer is also *wrong* — `agent-a065464fe9fc00c7d` is
`hopper`, not `tailrace`. See §6.)

- **SUPPORTS: session cwd is a weak join key, and its weakness is not the one
  documented.** `docs/join.md` and the worked example treat the risk as
  *deleted worktrees*. The larger risk on this machine is *displacement*: a
  fleet whose work is entirely in repo A, logged entirely under repo B,
  because of where a terminal happened to be. recount's `--repo` resolution
  cannot discover this, and `doctor` reports `READY` while 60% of the ledger
  is unreachable.
- **INFER: the fix is not a smarter heuristic.** A `cd` inside the transcript
  is discoverable but ambiguous (agents `cd` all over). The keyed tier solves
  this exactly, which is the argument for the chain, and §0 is the argument
  for why it solves nothing retroactively.

---

## 6. What recount got wrong while I was using it in anger

This section is the honest one. I checked recount's answers against the
transcripts it named, by reading each matched transcript's first turn and
comparing the identity it declares against the identity in the ledger row.

### The 0.65 tier is labelled CONFIDENT and is wrong five times out of six

| repo | distinct (ledger agent → matched session) pairs at 0.65 | correct | wrong |
|---|---|---|---|
| pact | 14 (4 unverifiable: they matched human-driven sessions whose first turn declares no identity) | 3 | 7 |
| quern | 7 | 0 | 7 |
| arkanoid | 2 | 0 | 2 |
| millrace (08-16) | 5 | 1 | 4 |
| **total checkable** | **24** | **4** | **20** |

Specimens, ledger identity → what the matched transcript's first turn says it
is:

- quern `parser-stmt` → "Your agent name: **machine-core**"
- quern `compiler-stmt` → "Your agent name: **eval-stmt**"
- quern `error` and `spec-adversary` → both → "Your agent name: **lexer**"
- quern `orchestrator` and `slt-ddl-dml` → both → "Your agent name: **slt-txn**"
- arkanoid `agent-b3-pack` → "PACT_AGENT=**agent-c4-texturing**"
- pact `lease-metrics` and `span-dev` → both → "you are pact agent **\"msg-metrics\"**"
- pact `otel-core` → "you are pact agent **\"closer\"**"
- millrace `hurst`, `wallower`, `thrall`, `brayer` → all four → session
  `02be3da1…`, whose first turn reads "You are **stonenut**"

Note the shape: identities collide *in pairs and fours*, all landing on
whichever transcript happens to overlap the window and contain the agent's
name somewhere. This is §2 of `worked-example.md` — "a signal that is true of
every agent in the fleet cast the deciding vote" — measured at scale.
**16.7% precision.** The document already says a 0.65 match is a lead. The
CLI still prints `CONFIDENT` next to it, and `testify` prints it in a report
a person is meant to act on.

### 0.95 is not a safe floor either

pact's `stale-holds` produced eighteen findings at **0.95** — twelve for
`orchestrator`, six for `crucible-impl-b` — all pointing at the same file,
`416610a6-0d95-4ddf-8c70-7915d9c4ff33.jsonl`. recount's own evidence line
gives the reason away:

> · time overlap: session covers 2026-08-02T05:32:11Z..2026-08-15T07:03:01Z
> and overlaps the queried 2026-08-14T15:00:43Z..2026-08-14T15:10:43Z

That is a **single session spanning 13 days and 14,129 turns**. It overlaps
every window anyone will ever query. Its cwd is the pact main checkout, so
topology "discriminates". It mentions every agent name in the fleet, because
it is the session that spawned them. Topology + overlap + mention = 0.95, on
a session that cannot be wrong and cannot be informative. Six of those eighteen are `crucible-impl-b`, a subagent with its own
transcript under that very session's `subagents/` directory; recount pointed
at the parent instead.

**INFER: session span is the wrong overlap primitive.** A 13-day session and
a 3-minute session score the overlap signal identically. Overlap should be
measured against *turn density inside the window*, not against the outer
bounds of the file.

### A CONFIDENT match with an empty transcript window

Reproduced above in §5:

```
    tier topological · confidence 0.65 · threshold 0.50 · CONFIDENT
      · time overlap: session covers 2026-08-15T20:22:44Z..2026-08-16T06:32:03Z
        and overlaps the queried 2026-08-15T21:04:25Z..2026-08-15T21:14:25Z
  transcript window — empty: the session's span overlaps
  [2026-08-15T21:04:25Z … 2026-08-15T21:14:25Z], but no single turn falls inside it.
```

recount scored a *time overlap* signal at 0.20 for a session that has no turn
in the window, then said so itself two lines later. The two statements are
produced by different code paths and they disagree. The message is good; the
score should not have been awarded. (This session is hopper, parked from
20:30:42Z to 06:01:29Z — §2's gap is exactly what created the empty window.)

### `testify --json` drops the refusal reason

`recount testify --repo … --json` emits `{"outcome":"refused","reason":null,
"detail":null}` for every refusal, while the human digest on the same run
prints the full structural reason and the candidate list. Anyone consuming
the JSON — which is the whole point of `testify` being a pipeline stage —
gets a refusal with no explanation.

### `retry-storm` is the one check `testify` cannot join

```json
{"agent":"tailrace","path":".pact/internal/merge-to-master","refusals":4,
 "min_gap_secs":16,"median_gap_secs":24,"median_holder_remaining_secs":2643,
 "ever_claimed":true}
```

No timestamp field of any shape. `testify` finds the moment inside a finding
by name (`at`, `*_at`), correctly passes this through as untestifiable, and
counts it — exit 0, three findings `passed_through`. That is the right
behaviour on the wrong data: the single audit check that is *about agent
behaviour over time* is the one whose findings carry no time. Every answer in
§3 had to be found by hand.

---

## 7. What cannot be known, and why

Absence of testimony is a result, so here it is quantified.

- **All four historical ledgers are un-keyable, permanently.** 2,456 rows,
  zero chains (§0). Nothing recount ships or will ship changes this. The only
  remedy is prospective.
- **Reasoning is not in the transcripts.** Every extended-thinking block in
  every session read for this document stores `"thinking": ""` with only a
  signature:
  ```json
  [{"type":"thinking","thinking":"","signature":"Eu0FCokBCBAYAipAmcOb…"}]
  ```
  recount prints these as `[thinking]` turns with no body, correctly. So
  "did the agent *reason* about X?" is answerable only when the agent said so
  in visible text. Every quote in §1–§4 is visible assistant text or a tool
  call for exactly this reason. **A question that depends on private
  reasoning is unanswerable on this corpus, and will stay that way.**
- **The pact repository's own 08-11, 08-12, 08-14 and 08-17 ledger activity
  has no session file dated those days** (session file dates present: 07-30,
  07-31, 08-02, 08-03, 08-04, 08-06, 08-07, 08-08, 08-13, 08-15, 08-18,
  08-19, 08-20). 48 rows on 08-11, 12 on 08-12, 72 on 08-14, 27 on 08-17 —
  159 rows, 22% of that ledger — have no same-day transcript. recount answers
  many of them anyway, at 0.95, by matching the 13-day session (§6). Those
  answers should be read as *no answer*.
- **Retention is not the binding constraint here, and I expected it to be.**
  Claude Code keeps roughly 30 days; every ledger examined is inside that
  window. The testimony that is missing is missing because of *where it was
  filed* (§5) and *what a transcript records* (thinking), not because it aged
  out. On a ledger older than a month, retention becomes the whole story —
  and there is no warning anywhere in pact or recount that a coordination log
  committed to git outlives, by design and by a lot, the transcripts that
  explain it.

---

## 8. What was filed, and what was deliberately not

Six findings graduated to beads in the **pact** repository, under epic
`pact-x16` — "session-learnings (recount v0.2.0)". Each carries its repo,
session path, quoted excerpt, tier, and a ready-to-paste fix prompt.

| bead | class | finding | § |
|---|---|---|---|
| `pact-x16.2` | protocol | "subscribe and pick up other work" is unexecutable for a subagent | 2 |
| `pact-x16.1` | protocol | the red-master rule reached agents only via the next cohort's prompt | 1 |
| `pact-x16.3` | tooling | the refusal payload leads with the holder's age | 3 |
| `pact-x16.4` | tooling | `claim-lease-divergence` uses the bead's present-tense assignee | 4 |
| `pact-x16.5` | tooling | `retry-storm` findings carry no timestamp | 6 |
| `pact-x16.6` | tooling | `pact lease acquire` has no `--bead` | 8 |

**The findings in §5 and §6 are about recount, not pact, and were not filed
as pact beads.** They belong in recount's own tracker: the cwd-displacement
blind spot, the 16.7% precision of the 0.65 tier, session-span overlap
scoring, the CONFIDENT-with-empty-window contradiction, and
`testify --json` dropping the refusal reason.

## 9. Findings that did NOT graduate

Kept here deliberately; each is real but under-evidenced, and none became a
bead.

- **`bd ready` is not a reservation.** Two independent final reports say so —
  `fuller`: "two of my first three candidate picks […] were already claimed
  and had live worktrees […] despite showing as `○ open` moments earlier in
  `bd ready`'s own output"; `wheelwright` reports the same class of collision
  discovered only at `lease acquire`. This is a bd finding, not a pact one,
  and millrace already has beads open on it.
- **bd's claim heartbeat is a second staleness concept.** `sluice`: "my own
  `bd show millrace-62d` reported `Lease: expires expired (heartbeat 7 mins
  ago)` while I was still actively working it — this is bd's own
  claim/heartbeat, unrelated to pact leases and not covered by the annex".
  One transcript, one occurrence; millrace-9eg already covers it.
- **`pact lease acquire` has no `--bead`.** Confirmed against 0.11.0
  (`--agent --ttl --json --steal --note`), and every lease note quoted in
  this document carries its bead id by convention: `"millrace-t7k: corpus
  gap, …"`, `"millrace-ymj: new corpus case, …"`. The attribution chain
  landed in 0.11.0; the bead link did not. Filed as a bead (it is
  well-supported), but flagged low-impact: the convention works, it is just
  unparseable without a regex.
- **A `0.65` match on `treadle` scored a "transcript mention" from the string
  `tailrace` appearing in a peer's commit-message quote.** Same failure mode
  as `worked-example.md` §2. Not filed separately — §6 supersedes it.
- **`model:` corroboration is weaker than it reads.** The one keyed example in
  `worked-example.md` §4 says "declared claude-opus-5, session records
  claude-opus-5 — they agree". Every thinking-block signature in the millrace
  transcripts base64-decodes to a string containing `claude-sonnet-5`, i.e.
  the model recorded at session level and the model that produced a given
  turn are not guaranteed to be the same thing. One observation, no follow-up
  — a lead, not a finding.

---

## Appendix: reproducing this

```sh
for r in pact millrace quern arkanoid; do
  recount doctor --repo ~/Documents/$r
  for c in stale-holds merge-divergence silent-contention retry-storm \
           double-win claim-lease-divergence; do
    (cd ~/Documents/$r && pact audit --check $c --json) \
      | recount --json testify --repo ~/Documents/$r
  done
done
```

The millrace 2026-08-15 testimony is **not** reachable that way; use
`--repo ~/Documents/treadle` for anything before 2026-08-16T11:00Z (§5).
Every count in this document was taken on 2026-08-20 against pact 0.11.0 and
recount built from `wt/learnings` at `7929c23`.
