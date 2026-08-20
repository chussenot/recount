# The join

The join answers one question: *which session was driving this identity at
this moment, and how sure are we?* It is implemented in `src/join.rs`, whose
module doc carries the same reasoning next to the arithmetic.

Everything below was checked against the code, or measured on this repository
on 2026-08-19 at `457a254`, and on 2026-08-20 at `6c412a7` and again after the
lead rule landed. Where a number is a
measurement, its conditions are stated with it; re-measure before quoting one,
because the ledger and the corpus both grow and the worktrees a fleet leaves
behind get deleted.

## Two tiers, and only the lower one is scored

pact 0.11.0 stamps an attribution chain on the events it writes, and one field
of it changes the question being asked. An event carrying `harness_subagent`
**names** the transcript it was written from. Resolving that is a path join and
one `is_file()` — no candidate set, no score, no tie to break — so it is not a
fifth rung on the ladder below, it is a tier **above the whole ladder**:

| tier | how it answers | what it returns |
|---|---|---|
| **keyed** | the event names its transcript; `claude_code::keyed` joins the path and stats it ([connectors.md](connectors.md)) | an identity — no confidence |
| **topological** | the four-rung ladder, unchanged | a score, with its evidence |

The two are not tuned in terms of each other, and the ladder was not retuned
when the tier arrived above it: same three signals, same four rungs, same
threshold. One is an identity, the other is a guess, and there is no exchange
rate between them.

**The gate is `harness_subagent`, and nothing else** — not `harness.is_some()`,
not `harness_session.is_some()`. A top-level agent has a harness and a session
and is not a subagent of anything. That is not a degraded chain, it is what a
complete one looks like for a top-level agent: 13 rows of this repo's ledger are
exactly that shape — 12 orchestrator rows and one precheck, measured 2026-08-20
on a live ledger where, unlike the subagent-bearing count, this one does not
grow with each wave. Proxying the gate onto those fields would send a normal
state into a keyed lookup that cannot succeed, turning it into a refusal.

Several encoded session directories are stat'd, not one, because a repo and its
worktrees encode to different ones and the event does not say which. That is a
handful of stats and first hit wins — still not a walk.

### A keyed answer has no confidence, and says so with an explicit null

`Tier` is `enum { Topological(Confidence), Keyed }` (`src/model.rs`): the score
lives *inside* the inferring variant, so there is no `f64` on the keyed arm for
a caller to print, unwrap or compare. It writes two flat fields:

```json
{ "tier": "keyed",       "confidence": null }
{ "tier": "topological", "confidence": 0.65 }
```

`confidence` is always written, so one field answers "was this inferred?". A
keyed answer puts `null` there rather than 0.99 or 1.0, and the type refuses to
read one back — `{"tier":"keyed","confidence":0.99}` is a deserialisation
error. A high number would be a number nobody computed, sitting in the one
field whose entire meaning is that somebody computed it, and the first thing
anyone does with two confidences is compare them.

The threshold is applied through `Tier::clears`, never by reaching for the
number: a keyed answer clears every bar, 1.0 included. A caller that re-derived
the comparison itself would drop an identity for failing a test it was never
sitting.

`Match` still cannot be built without a non-empty `Vec<Evidence>`, keyed answers
included. The single `Signal::Keyed` line names both ids, the path they joined
to, and the fact that the ladder was never consulted.

### Every topological answer says why it was not keyed

Answers from the lower tier carry a `Signal::Keyed` evidence line too, and it
has exactly one thing to say, because there is exactly one way to reach the
ladder from here: the field was absent. An event that *has* a `harness_subagent`
either resolves keyed or refuses. It never quietly demotes to guessing, which
is the outcome the tier exists to prevent — an identity silently rounded down to
a score is indistinguishable, in the output, from a score that was earned.

So the line says "no `harness_subagent` on this event … a normal state, not a
degraded one". It is a statement about the event, not a complaint about it.

### The parent case: a session with no subagent

A row with a `harness_session` and no `harness_subagent` names a file just as
exactly — the top-level `<session>.jsonl`, whose stem equals its `sessionId` in
238 of 238 observed files ([format-notes.md](format-notes.md) §3.1). The
connector half already does it: `claude_code::keyed(dir, session, None)` returns
that path. The join does not yet call it that way; `recount-1cw.13` is that
change, and it must reuse `Tier::Keyed` rather than invent a third tier.

It is worth what it is worth, which is this run's own self-audit: the 13 rows
above, all of them the orchestrator's, which today fall to the ladder and land
on some subagent's transcript instead of the orchestrator's own. It does **not**
moot the tie-break defect described below — the rows that defect was measured on
predate the contract and carry no session id either, so there is nothing to key
them by.

### Not yet reachable from the command line (2026-08-20)

The tier is in `join::resolve`. The two commands that call it are not wired to
it yet: `at` and `explain` both build their `Query` with `harness_session: None`
and `harness_subagent: None`, so every CLI invocation today takes the
topological branch — including on the rows of this repo's own ledger that do
carry a subagent id (35 of 225 on 2026-08-20). That is `recount-1cw.6` and
`recount-1cw.7`. Until they
land, the `Signal::Keyed` line printed on those rows says the event had no
subagent id when the event has one: the join is describing the query it was
handed, accurately, and the query dropped the field on the way in.

## What the keyed tier does not fix

It is a real improvement and a narrow one. Four things it leaves exactly where
they were, and the last three are the ones a reader is most likely to assume it
handled.

**Every pre-0.11.0 ledger is un-keyed, forever.** Only the agent can write
`harness_subagent`, only at write time, and only if it knew its own id
([format-notes.md](format-notes.md) §10 — the recipe, and the recipes that look
right and are not). Nothing can add the field afterwards: the id was never
recorded, and inferring it is precisely the guess this tier replaces. This
repository's own ledger is the example. The first 177 rows of
`.pact/events.jsonl` — the whole 26-agent run that built recount through 0.0.1
and 0.1.0, and the rows every measurement further down this page was taken
on — carry no chain field on any row; the chain begins at row 178. Those 177
answer topologically or not at all, with every weakness documented below, and
always will.

**The tie-break defect (`recount-gwx`) is sidestepped, not fixed, and not
moot.** Among equally-scored topological candidates the winner is still the
first one offered, candidates arrive in filesystem path order, and path order
puts the `subagents/` directory ahead of its sibling top-level file — so an
orchestrator's event still systematically returns a subagent's transcript.
Keyed events cannot tie, so they never reach that code. The rows the defect was
measured on carry no chain at all and still do reach it: re-run on master at
`6c412a7` on 2026-08-20, *after* the keyed tier landed,
`explain --event-line 60` still returns the wrong transcript at 0.65 and exit 0.
Nothing on this page should be read as saying the defect is gone.

**The tier covers the ids the self-id recipe can produce, not every transcript
that exists.** 371 of 634 subagent transcripts on this machine live one level
deeper, under `subagents/workflows/wf_<id>/`, and no pact event carries a `wf_`
id ([format-notes.md](format-notes.md) §11.0). Those resolve absent and belong
to the ladder. This is harmless today purely by construction — the only recipe
by which an agent can learn its own id globs the flat directory, so an id that
reached an event came from a flat file — and "harmless by construction" is a
condition, not a property. **Do not fix this by scanning `workflows/`**: a scan
is the thing this tier exists not to do, and it would hand the tie-break defect
a set of candidates it currently cannot touch. If a future harness hands an
agent its id directly, the fix is one bounded `read_dir`, still not a walk.

**`harness_subagent` is not implied by the rest of the chain.** A top-level
agent has a harness, a model, a session and a branch, and no subagent id. That
is a complete record of a top-level agent, not a truncated record of a subagent.
Anything that reads the absence as damage — a warning, a lower score, a refusal
— manufactures a finding, which is the failure mode this whole tool was written
against.

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
2. **Time overlap — required, not weighted, and measured on turns.** A session
   with no turn inside the event window is not a weak match; it is not a match
   at all, because it cannot have produced the event. No overlap
   short-circuits to a refusal before any score is computed.

   **The primitive is the turn, not the span.** A session whose
   `first_ts..last_ts` straddles the window has *not* overlapped it: a
   thirteen-day session straddles every window anyone will ever query, and a
   session parked overnight straddles the whole night it slept through. The
   gate is `Match::in_window`, the turns actually inside, and it is non-empty
   on every topological answer by construction. The span is still consulted,
   but only as a cheap necessary condition — every turn lies inside the span,
   so a span that misses the window means the file need not be opened at all.

   That field is also the *only* place "inside the window" is computed. `at`
   and `testify` print `Match::in_window`; neither re-derives it. This is
   structural rather than conventional, and deliberately so: the two used to
   answer the question separately, which is how recount came to award a time
   overlap signal and then print "no single turn falls inside it" two lines
   below it, on a session that had been parked through the whole window
   (`session-learnings.md` §6).
3. **Transcript mention — the tiebreaker, and only a tiebreaker.** The
   `PACT_AGENT` name or a bead id appearing in the transcript separates
   sessions that already passed 1 and 2. It never creates a match on its own:
   an agent discussing `skeleton` is not `skeleton`, and a session the chain
   excluded is never scored however often it says the name. Nor does it ever
   *decide* one: an answer the mention picked out, with topology unable to
   separate the candidates, is a **lead** and is never confident whatever it
   scores.

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

This is the lower tier — everything in it is inference, and the keyed tier
above it does not touch a single number here.

Four fixed numbers, summed. They are reproduced here in full, because the
*relationships* between the rungs are the point — a single rung quoted alone
means nothing. The constants live in `src/join.rs`
(`TOPOLOGY_DISCRIMINATING`, `TOPOLOGY_FILTER`, `OVERLAP`, `MENTION`), and the
threshold is the single `CONFIDENT` constant in the same file.

| outcome | confidence | verdict |
|---|---:|---|
| chain singled the session out, + mention (topology A) | 0.95 | answer |
| chain singled it out, no mention (topology A) | 0.70 | answer |
| chain only filtered, + mention | 0.65 | **lead** |
| chain only filtered, no mention | 0.40 | **lead** |

Read it as three statements:

- Topology A clears the bar **on topology alone** (0.70). No transcript
  wordplay is needed for an answer that stands.
- 0.65 is the **ceiling** when the chain only filtered, and reaching it requires
  the mention. It is a lead, not an answer — see below.
- 0.40 is the same regime with nothing speaking for it at all, and it was
  always below `CONFIDENT`.

### A mention-decided answer is a lead, and the tool says so

**Since 2026-08-20, the bottom two rows are never confident, whatever they
score.** This is a second, structural bar beside `CONFIDENT`, and it is
deliberately not a number: `Match::lead` is set whenever topology filtered
without discriminating, and `Match::confident()` — the one verdict `at` and
`testify` are allowed to ask for — is `!lead && tier.clears(CONFIDENT)`.

The reason is measured rather than argued. The 0.65 rung was checked against
the identity each matched transcript declares in its own first turn, over four
repositories, and got 4 right out of 24
([session-learnings.md](session-learnings.md) §6). Re-checked on 2026-08-20 over
the `stale-holds` findings of the same four ledgers, on the 13 distinct
(agent → matched session) pairs recount then called CONFIDENT: **1 correct, 9
wrong, 3 unverifiable** (the unverifiable three matched human-driven sessions
whose opening turns declare no identity). The tiebreaker cannot be repaired
either, and that was measured too: restricting the mention to turns inside the
window leaves the distribution bit-identical, because an orchestrator session
says "orchestrator" in every ten-minute stretch of itself.

Raising `CONFIDENT` to 0.7 would have reached the same findings, but it would
also have moved every other rung and changed what the word "confident" means
for answers nobody had complained about. The lead rule says the one thing that
is actually meant: *a signal that is true of every candidate cannot cast a
deciding vote, and no score reachable without a discriminating chain is an
answer.*

**The ladder is untouched and the score is still printed.** 0.65 is still
computed, still serialised, still in the digest beside `threshold 0.50`; what
changed is the verdict next to it, which now reads `LEAD — NOT CONFIDENT`, and
the topology evidence line, which says in words why. A reader must never have
to infer "this is a lead" from the size of a number — that is exactly how a
ledger of 0.65s came to be read as a ledger of answers.

`at` applies both bars; `join::resolve` applies neither. A below-bar or lead
candidate is still *printed*, with its evidence and without its transcript,
because the evidence is what tells the auditor whether to look at the clock or
at the topology — and a structural refusal (which prints no candidate at all)
stays a distinct outcome from a candidate that did not clear the bar. See
[cli.md](cli.md) for what each outcome prints and exits.

Both commands also carry the fact into `--json` as a `lead` boolean beside
`confident`, so a script never has to parse a sentence or re-derive a rule.

## How the degeneracy is detected

Not configured, and not inferred from the fleet's shape: counted, from the
data, after the overlap gate.

First, **is the worktree `invoked_from` named the main checkout?** If it is,
the count below is never taken, because at the repo root the count is not
evidence about the chain — it is evidence about the corpus. Every agent that is
not in a worktree runs in the main checkout, and every subagent of a parent
started there records it as its cwd ([format-notes.md](format-notes.md) §4.3),
so "this session's cwd is the root" is a property of the checkout, not of the
session. `sessions` has said exactly this for as long as it has had a topology
signal — *"cwd `<repo>` is the repo root, which every agent of a pact run
shares (§4.3) — it names no one"* — and so has `grep`. The join now agrees with
them instead of contradicting them one command over.

That was not a theoretical inconsistency. On pact's own ledger, `invoked_from:
main` resolved to the root, exactly **one** session in that corpus had its cwd
there, `exact == 1` fired, and eighteen `stale-holds` findings were awarded
0.95 and printed `CONFIDENT` — all eighteen landing on the same session, one
spanning 13 days and 14,129 turns. A predicate whose answer swings on how many
*other* sessions happen to exist is not a discriminator. A corpus with two
sessions at the root would have scored the identical evidence lower.

Otherwise, among the surviving candidates, count those whose cwd resolves
**exactly** to the worktree `invoked_from` named (as opposed to an ancestor of
it). That count is the rest of the predicate:

- `exact == 1` — the chain singled a session out. Topology A, and the only case
  where topology discriminates. **`exact == 1` at the repo root does not count**:
  see above.
- `exact == 0` — no candidate names the worktree; every one matched at the repo
  level. Topology B.
- `exact > 1` — several name it and the chain cannot separate them. An
  ambiguous topology-A tie.

Zero and many are both coin flips, not answers, so both score as a filter
(0.2) rather than as a discriminator (0.5), and the mention becomes decisive.
A fifth case exists for completeness: nothing named a worktree at all — no
`invoked_from`, and no worktree named after the agent — where the chain has
filtered to "sessions in this repo" and says so.

Every answer names the regime it was decided in, in its topology evidence line.
That sentence is not decoration: in every regime but topology A it is followed
by the words `THIS IS A LEAD, NOT A CONFIDENT ANSWER`, and the answer is not
confident whatever it scored.

## The refusals

Three, and no more, because an auditor needs to know which question to ask
next. The spec named the first two; the third arrived with the keyed tier,
which fails in a way neither of the others describes.

- **`no cwd match`** — the topology failed. No session's cwd resolved into the
  repo or the named worktree, or `invoked_from` names a worktree this repo does
  not have, or the worktree it names is gone from disk. *Go and look at the
  topology.*
- **`no overlap`** — the topology succeeded and the clock failed. Sessions
  matched the chain, but none has a turn inside the window. *Go and look at the
  clock.* The detail lists the sessions that matched and the span each covers,
  so the reader can widen `--window` with a number rather than a guess — and
  says of each whether its span missed the window entirely or straddled it and
  was silent through it, because only the first is fixed by widening.
- **`no keyed transcript`** — the event named a transcript and the name did not
  produce one. Keyed events only; nothing on the ladder can raise it. *Go and
  look at the disk, or at the ledger* — which of those depends on the cause,
  and the detail says which.

The third one refuses rather than falling back to the ladder, and that is the
whole design: an event that named its transcript has told us the answer, so a
score computed instead of it would be a guess dressed as the identity it failed
to find.

### The two causes of `no keyed transcript`, which are different news

- **Absent** — the ids are well formed, the path was joined and stat'd, and
  nothing is there. Two things do this and the action differs. Either the
  transcript was **pruned**: Claude Code keeps 30 days
  ([format-notes.md](format-notes.md) §0.0), and a fleet that tidies its
  worktrees as it goes takes their encoded session directories with them — so
  look elsewhere on this machine, and at your backups. Or the **ledger came
  from another machine**, in which case the transcript was never here, no
  amount of looking will find it, and the question cannot be answered on this
  host at all. The refusal lists every path it stat'd, so the reader can tell
  the two apart by looking at them.
- **Malformed** — the id is not an id. Nothing was joined and nothing was
  stat'd, so this is not a transcript that has gone missing, it is a defect in
  the event that names it: look at the ledger, not the disk. The shape check is
  deliberately loose — non-empty, `[A-Za-z0-9-]` — and the reasoning is in
  [connectors.md](connectors.md).

A third case is a refusal too, and is nearly unreachable: the file is on disk
but is not among the sessions the join was offered, so no connector read it.
Look *at* the file rather than *for* it. A transcript being written when the
event fired has an mtime at or after it, so the mtime prune cannot drop it —
but "nearly unreachable" is not "impossible", and an identity silently
downgraded to a guess is the one outcome the keyed tier exists to prevent.

The first two are not interchangeable either, and collapsing them was not a
hypothetical:

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

**Still true after the keyed tier landed, and still the wrong transcript.**
These 177 events predate pact 0.11.0 and carry no attribution chain, so the
tier above cannot reach them, and `recount-gwx` — the tie broken by filesystem
path order — decides them exactly as it did before. Re-run on 2026-08-20 after
the lead rule landed, the example below returns the same wrong transcript at
the same 0.65 — but now as a **lead**, with `"confident": false`, `"lead":
true` and exit 1. That is the change: the answer is as wrong as it was and the
tool no longer stands behind it. The
ledger has 225 rows on 2026-08-20; rows 178 onward are the ones that carry a
chain, and this table is not about them.

Sweeping `explain` over every row of `.pact/events.jsonl`
(`recount explain --repo . --event-line N`, debug build at `457a254`,
2026-08-19, 177 events):

| outcome | events |
|---|---:|
| refused, `no cwd match` (exit 1) | 118 |
| match at 0.65 (exit 0) | 56 |
| match at 0.95 (exit 0) | 3 |

Of the 59 answers, **56 are wrong**, and all 56 were above the confidence bar.
Every one of the 59 is `invoked_from: main`, so under the lead rule all 59 are
now leads at exit 1: the 56 wrong ones no longer claim to be answers, and the
3 that were right no longer claim more than the chain can support. Losing three
correct answers to keep 56 wrong ones from printing CONFIDENT is the trade this
project exists to make.

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

One of them, re-run at `6c412a7` on 2026-08-20. The command is `recount --json
explain --repo . --event-line 60`; the transcript window and the unchanged
event fields are projected away with `jq`, nothing is edited:

```json
{
  "agent": "orchestrator",
  "tier": "topological",
  "confidence": 0.65,
  "sess": "agent-a0b9643f93682445f.jsonl",
  "evidence": [
    {
      "signal": "topology",
      "detail": "session cwd /home/chussenot/Documents/recount is worktree `main` — invoked_from named the MAIN CHECKOUT, which every agent not in a worktree shares … a cwd at the repo root names no one, however many sessions happen to sit there, so the chain filtered without discriminating and the transcript mention decides. THIS IS A LEAD, NOT A CONFIDENT ANSWER: …"
    },
    {
      "signal": "time_overlap",
      "detail": "6 turn(s) of this session fall inside the queried 2026-08-19T07:33:15Z..2026-08-19T07:43:15Z (the session covers 2026-08-19T07:41:47Z..2026-08-19T07:51:56Z)"
    },
    {
      "signal": "transcript_mention",
      "detail": "`orchestrator` at line 1 (user): …lines>\"` Do NOT merge — the orchestrator lands your branch at the wave…"
    },
    {
      "signal": "keyed",
      "detail": "no `harness_subagent` on this event, so nothing named a transcript: it was not written by a subagent. A top-level agent has a harness_session and is not a subagent of anything, and every orchestrator row in this repo's ledger looks exactly like this — a normal state, not a degraded one. The answer below is INFERRED by the topological ladder, not keyed"
    }
  ]
}
```

The event is the orchestrator taking the merge lock to merge `wt/connector`.
The transcript returned is `agent-a0b9643f93682445f.jsonl`, whose sidecar
describes it as *"W2 join engine bead"* — the `join` agent, a different agent
doing different work. Until 2026-08-20 that was exit 0, printed as CONFIDENT;
it is now exit 1, printed as `LEAD — NOT CONFIDENT`, with the same 0.65 and the
same evidence.

**Its own output says so.** The topology line says "filtered without
discriminating … the transcript mention decides". The mention it decided on is
the word `orchestrator` at line 1 of a *peer's prompt*, in the sentence "Do NOT
merge — the orchestrator lands your branch at the wave barrier" — boilerplate
handed to every agent in the fleet. A signal true of every candidate cast the
deciding vote.

That is the point of the evidence block, and the reason `Match` cannot be
constructed without a `Tier` and a non-empty `Vec<Evidence>`. The bare
answer misleads. The answer plus its evidence does not. This page used to end
the paragraph with *"read every 0.65 in this ledger as a lead, not an answer"*,
and leave the reader to do it. The tool says it now.

## What the lead rule cost, measured on four ledgers

`pact audit --check stale-holds --json | recount testify --repo <r>`, on
2026-08-20, immediately before and after the change. Same audit documents, same
corpora, same window.

| repo | findings | testified before | testified after | leads after | refused |
|---|---:|---:|---:|---:|---:|
| pact | 45 | 45 (18 × 0.95, 27 × 0.65) | **0** | 45 × 0.65 | 0 |
| quern | 3 | 3 (3 × 0.65) | **0** | 3 × 0.65 | 0 |
| millrace | 8 | 0 | 0 | 0 | 8 `no overlap` |
| arkanoid | 1 | 0 | 0 | 0 | 1 `no cwd match` |

Forty-eight confident answers became forty-eight leads, and no refusal changed:
every candidate is still found, still scored, still printed with its evidence.
The eighteen at 0.95 became 0.65 — the repo root no longer discriminates — and
then stopped being confident, because a mention decided them.

**That drop is the result, not a regression.** Of the 13 distinct
(agent → matched session) pairs behind those 48, checked against the identity
each matched transcript declares in its own first turn: **1 correct, 9 wrong,
3 unverifiable**. The tool was wrong nine times out of ten where it could be
checked, and said `CONFIDENT` every time.

The keyed tier is untouched by all of this, and was verified so:
`explain --event-line 207` (subagent-keyed) and `--event-line 216`
(parent-keyed) on this repo produce byte-identical output before and after. The
keyed tier never consulted the ladder, has no candidate set and no tie for a
mention to break, so `lead` is `false` on it by construction.

[worked-example.md](worked-example.md) replays one such case end to end, with
the full terminal output and with the refusals analysed. Its distribution
(160 events: 108 / 49 / 3, three worktrees still alive) was measured earlier the
same day; the ledger has grown to 177 rows since and the last two worktrees have
been removed, which is why the refusal count above is higher. Both readings are
correct on the day they were taken — which is itself the finding: **a fleet that
tidies its worktrees as it goes destroys the join key for its own audit.** The
transcripts still exist, the ledger still exists; the link between them was in
the filesystem and was deleted.
