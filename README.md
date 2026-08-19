# recount

Join a pact coordination ledger (`.pact/events.jsonl`) — *what* the agents of a
fleet did — to their coding-agent session transcripts — *why* they did it. So
that an audit finding can be answered with the agent's own words instead of an
inference about them.

recount reads pact's ledger directly and pact never learns recount exists: the
dependency is one-way, and nothing here writes.

One command takes a line number in `.pact/events.jsonl` and prints the
transcript behind it:

```
$ recount explain --repo . --event-line 1
recount explain — /home/chussenot/Documents/recount/.pact/events.jsonl line 1
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
      · time overlap: session covers 2026-08-19T07:00:48Z..2026-08-19T09:41:03Z and overlaps the queried 2026-08-19T06:58:12Z..2026-08-19T07:08:12Z
      · transcript mention: `orchestrator` at line 7 (user): …first) Run on an Opus-class orchestrator with the Task tool. Two-part…

  transcript window — 66 turn(s) in [2026-08-19T06:58:12Z … 2026-08-19T07:08:12Z]

    [2026-08-19T07:00:48Z] user (line 7)
      # PROMPT — build `recount` as a pact fleet (beads first)

      Run on an Opus-class orchestrator with the Task tool. Two-part input:
      Part A below is the product spec — commit it VERBATIM as docs/spec.md
      before anything else; it is the contract every bead cites. …
      [+7469 more characters — the full text is in --json]

    [… 65 further turns, elided here, not by recount …]
```

The ledger said the orchestrator set a policy. The transcript says what it was
told to do. Exit 0.

Every answer carries a confidence and the evidence behind it. There is no bare
answer, because the join is inference over two artifacts, neither of which was
written to be joined. Read [the weaknesses](#where-recount-is-weak) before you
trust a number.

## Install

```
cargo build --release      # ./target/release/recount
cargo install --path .     # or onto your PATH
```

Dependencies: `serde`, `serde_json`, `clap`, `clap_complete`, `chrono`,
`anyhow`. `tempfile` for tests. Nothing else, by budget.

## The six commands

The spec's five all take `--json`; the human digest is printed second.
`completion` is the sixth and the exception — see
[the deviations](#deviations-from-the-specs-literal-text).

```
recount sessions --repo <p> [--since --until]
    Sessions whose cwd resolves into the repo or its worktrees, with a
    fleet-identity guess and the reasoning for it attached.

recount at --repo <p> --agent <name> --ts <rfc3339> [--window 5m]
    Which session was driving that identity at that moment, and the
    transcript window around it. Exit 0 found, exit 1 refused — either
    with a reason, `no cwd match` or `no overlap`, or with a candidate
    that scored below the bar and the evidence that fell short.

recount grep <needle> --repo <p> [--agent --since --until]
    Case-insensitive substring, not a regex. Prints (session, line, ts,
    role, snippet) for every hit.

recount explain --repo <p> --event-line <N>
    Read event N of .pact/events.jsonl and run `at` on it. One command
    from an audit finding to the testimony.

recount doctor [--repo <p>]
    Connector roots, session counts, parse-failure tallies, and whether a
    join is possible here at all: are there events, are the worktrees the
    ledger names still on disk.

recount completion <bash|zsh|fish|elvish|powershell>
    The completion script for your shell, on stdout alone, generated from
    the same clap tree the parser uses. Not a `--json` command.
```

`--since` and `--until` take RFC 3339 or a duration ago (`2h`, `1d`). Start with
`doctor`: it answers "why did that refuse" before you ask.

## What recount refuses to be

No TUI. No daemon. No network. No embeddings. No fuzzy search —
case-insensitive substring only. One connector family.

And **no persistence**. Session files are the only truth; recount owns no store
that can drift from sources it does not own. There is no index, no cache, no
sidecar, no database. Every command re-reads what is on disk, and every answer
is therefore about the corpus as it is now rather than as some earlier run
recorded it.

Scans stay cheap structurally instead of by caching:

1. `--repo` resolves to the project's encoded session directories — the repo
   and its worktrees — before any file is opened.
2. Sessions are append-only, so mtime prunes files outside the query window
   without opening them. On a real corpus that is the difference between
   2.0 s and 142 ms.

## Where recount is weak

recount exists because of a misdiagnosis — a confident conclusion drawn from a
feature that could not discriminate. A README that oversold the tool would be
the same mistake in a new place.

### The join has a ceiling, and which ceiling depends on how your fleet runs

The join key is topological: `invoked_from` (a worktree name in the ledger) →
worktree path → session cwd, plus a required time-window overlap, plus a
transcript-mention tiebreaker. How much that chain can tell you depends
entirely on the fleet's process topology.

**Topology A, process-per-agent.** Each agent is its own `claude`, started in
its own worktree. The session's `cwd` *is* the worktree, so the chain
identifies one session out of the field. This is the case the spec was written
for, and the only one where topology discriminates.

**Topology B, subagent-per-agent.** Every agent is a subagent of one session,
and records the **parent's** cwd. Measured on the fleet that built recount: all
25 subagent transcripts record `/home/chussenot/Documents/recount` and nothing
else, while the ledger names 26 distinct worktrees those agents were working
in. Not one transcript names the worktree its own agent was assigned. (The only
lines that do name a worktree belong to the *orchestrator's* session, which
`cd`'d into three of them itself.) Under B the chain filters to the right
*repository* and then stops.

The score ladder, from `src/join.rs`:

| situation | confidence |
|---|---:|
| chain singled the session out, + mention (topology A) | 0.95 |
| chain singled it out, no mention | 0.70 |
| chain only filtered, + mention (**topology B ceiling**) | 0.65 |
| chain only filtered, no mention | 0.40 |

The threshold is 0.50. So topology A can answer on topology alone; topology B
cannot reach 0.50 without the mention tiebreaker, and 0.40 is deliberately
below the bar — "filtered but did not discriminate, and nothing else spoke" has
to be a refusal rather than a guess.

### The tool is wrong on this repository's own ledger, and its own output shows it

Of the 160 events in this repo's ledger when
[`docs/worked-example.md`](docs/worked-example.md) was written, `explain`
refused 108, matched 49 at 0.65, and matched 3 at 0.95.

Every one of those 49 carries the same evidence line: the chain *"filtered
without discriminating and the transcript mention decides"*. They are leads,
not answers.

One of them is demonstrably wrong. Event 122 records the orchestrator holding
the merge lock; `explain` exits 0 at confidence 0.65 and prints 19 turns of a
**different agent's** session — the `honesty` agent running `recount doctor` in
its own worktree. The deciding signal was the word "orchestrator" appearing in
the *prompt of a peer*, in a sentence every agent in the fleet was handed.

That is the arkanoid-v1 failure mode reproduced live, by the tool built to
prevent it. It is not hidden, and it is not the point. The point is that a
reader who looks at the evidence lines instead of the number catches it in
about ten seconds: the topology line says the chain could not discriminate, the
overlap line shows the session ended before the event, and the mention line
quotes boilerplate. **Confidence plus evidence is what lets a reader catch the
tool being wrong**, which is exactly why the spec forbids a bare answer. A tool
that printed only the session path would have made the same mistake with no way
for anyone downstream to see it.

The full case, with both readings side by side, is in
[`docs/worked-example.md`](docs/worked-example.md) — recount run for real
against this repository's own ledger and this machine's transcripts, showing
one join that is right, one that is confidently wrong, and why two thirds of
the ledger refuses.

### Keep your agent worktrees until the audit is done

108 of those 160 events refuse with `no cwd match`, and they all refuse for the
same reason: `invoked_from` names worktrees — `skeleton`, `ci`, `join`,
`cmdexplain`, `bench` — that the orchestrator deleted at each wave barrier.

```
$ recount explain --repo . --event-line 133
  …
  no confident match — reason: no cwd match
    invoked_from `cmdexplain` names no worktree of this repo (known: `main`, `readme`)
  no candidate to show: this is a structural refusal, not a weak match. Look at
  the topology.
```

The refusal is correct — the worktree is genuinely gone, and guessing would be
the arkanoid mistake again. But it means **a fleet that tidies up as it goes
cannot later be audited through the topological chain.** The transcripts still
exist and the ledger still exists; the link between them lived in the
filesystem and was deleted.

If you plan to point recount at a finished fleet, decide this before the run,
not after: keep the agent worktrees until the audit is done, or accept that
only the transcript-mention signal survives — and on its own that scores 0.40,
below the bar, so `at` will refuse.

### Known and deliberate

- **`grep` still parses the corpus twice.** Once in `discover_all`, once in the
  `connector.read` loop, worth ~540 ms of a ~1.0 s unwindowed scan over 200 MB.
  Removing it honestly requires a change to the `Connector` trait, which nobody
  was authorised to make. It is a known cost, not an oversight.
- **`generic_jsonl` cannot be pointed at "any dir of `*.jsonl`".** No command
  takes a directory; every command takes `--repo`, which resolves into
  `claude_code`'s own tree, and `generic_jsonl` declares no roots of its own. In
  practice the fallback only ever gets the paths `claude_code` declines *inside*
  that tree — on this machine, 12 workflow `journal.jsonl` files under the pact
  repo, none of which state a cwd, so the join correctly refuses all of them.
  The spec mandates two connectors; the commands the spec specifies can only
  really reach one.
- **The `~/.claude` format notes are reverse-engineered and will drift.**
  [`docs/format-notes.md`](docs/format-notes.md) was measured on one machine on
  one day, over a corpus already spanning 22 Claude Code point releases whose
  field sets disagree with each other. Two passes 2h45m apart already disagreed
  on the file count, the line count and the field inventory. The connector
  degrades rather than asserts, and the notes are a snapshot to be
  re-measured — never a contract.

### Deviations from the spec's literal text

**`recount completion <shell>` is a sixth command, and the one that does not
take `--json`.** The spec's command list names five and says "all --json; human
digest second". `completion` writes a shell program to stdout, so a digest
printed beside it would be sourced as code: `--json` is accepted and ignored
rather than refused, since the flag is global and erroring on it would turn
something harmless into a usage error. The alternative was five checked-in
completion scripts, which are hand-written copies of the CLI and drift from it
the first time a flag changes.

The spec writes the trait as `discover() -> Vec<SessionMeta>`. The code is
`discover(dirs, window)`. Taken openly, because the spec's own cheapness
clauses — "`--repo` resolves to the project's encoded session directories …
before any file opens", and mtime "prunes files outside the query window
unopened" — are unsatisfiable if the connector does not learn the scope before
it opens anything. Still two methods plus `roots()`, so the spec's "a third is
one file" holds.

## Credit: cass

[cass](https://github.com/Dicklesworthstone/coding_agent_session_search) —
Dicklesworthstone's coding-agent session search — is the product-scale proof
that this idea is worth having: ~685k lines, 25 connectors, SQLite + Tantivy,
embeddings, a TUI, sync, crypto. It is the thing you want if you want to *search
your agent history*, and it does the whole job properly.

recount implements the audit-testimony 3% of it and refuses the rest by design.
The refusals above are not a critique of cass; they are what is left when the
requirement is narrowed from "search everything" to "answer one audit finding
with one transcript, on a machine that owns no index". cass earned the general
case. recount only had to earn one join.

## The deferral, and the measurement that did not trigger it

The spec defers exactly one thing:

> a metadata-only sidecar manifest (rebuilt wholesale on any mtime mismatch,
> rm-safe) may be added **iff real windowed queries exceed ~2s on a real
> corpus — never a content index, never a database**.

**Not triggered.** A real windowed grep is **138.8 ms**.

```
cargo test --release --test bench -- --ignored --nocapture
```

Release, warm page cache, AMD Ryzen 7 PRO 8840HS:

| measurement | result | ceiling |
|---|---:|---:|
| pruned `at`, answering path (500 synthetic sessions) | 15.1 ms | 100 ms |
| unwindowed `grep`, 500 synthetic sessions / 193 MB | 1025.8 ms | 2000 ms |
| real corpus (~173 MB, 254 sessions), unwindowed `grep` | 1944.8 ms | — |
| real corpus, `grep --since 24h` | **138.8 ms** | 2000 ms (the clause) |

Both spec ceilings are met. Re-run twice while writing this README, on a
machine that still had the fleet on it: 18.5 / 64.2 ms, 1216.9 / 1057.8 ms,
2475.2 / 1974.5 ms, 142.5 / 142.0 ms. The margins move with load; no verdict
did.

Two reasons the manifest stays unbuilt:

1. **The clause names *windowed* queries, and the mtime prune already does the
   manifest's job.** Windowed grep opened 6 of 254 sessions and finished in
   138.8 ms. A manifest would be a second, staler way to learn what one `stat`
   per file already says. Note that the unwindowed number crossed 2 s on one of
   the re-runs while the windowed one did not move — which is the clause
   working exactly as written.
2. **A manifest holds metadata, so it could only ever remove the first of
   `grep`'s two parses.** The character scan cannot be removed by anything that
   is not a content index, and the spec forbids a content index. When the
   unwindowed grep was 2915 ms, 2105 ms of it was that scan; it came down by
   fixing the scan, not by adding a store. Read the bench's printed
   decomposition before reaching for a manifest.

## Docs

- [`docs/spec.md`](docs/spec.md) — the contract. Where quote and code disagree,
  the quote wins.
- [`docs/worked-example.md`](docs/worked-example.md) — recount run for real
  against its own birth: one correct join, one confidently wrong one, and 108
  refusals with a fleet-practice finding behind them.
- [`docs/format-notes.md`](docs/format-notes.md) — the observed `~/.claude`
  layout. Reverse-engineered, expect drift.
