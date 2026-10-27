# Limitations

Where recount is weak, why, and what you can do about it. Read this before you
trust a score — the README's argument for scoring with evidence only holds if
you know which evidence line means "do not trust this number".

None of this is a disclaimer. Each limit below follows from a design choice the
[README](../README.md) defends, and recount is built to *show* you when you have
hit one rather than to hide it.

## The join has a ceiling, and which ceiling depends on how your fleet runs

The topological key assumes an agent's session records the directory that agent
worked in. That holds when each agent is its own process started in its own
worktree. It does **not** hold when your agents are subagents of one session:
they all record the parent's working directory, so the chain filters the
candidates correctly and then cannot tell them apart.

recount detects this rather than papering over it. An answer the chain could
only filter is a **lead**: it is still scored, still printed with its evidence,
and never confident, whatever the number says. `at` and `explain` print
`LEAD — NOT CONFIDENT` beside the score and exit `1`; `testify` lists the
finding as untestified, "a LEAD, not an answer", and exits `1` too; every
`--json` answer carries `"lead": true`. With nothing
else speaking for it, such a match also scores below the confidence bar, so
there is no path from a degenerate key to a confident answer.
[join.md](join.md#the-two-fleet-topologies) describes the two topologies, and
[join.md](join.md#the-score-ladder) the rungs and the lead rule.

**What lifts the ceiling:** a ledger that names the transcript. pact stamps
`harness_session` (and `harness_subagent`, when a subagent wrote the event) on
the events it writes, and recount resolves those rows by identity — no score,
no tie to break. Rows written before pact did this, or by an agent that could
not resolve its own transcript id, still fall through to the topological tier
and its ceiling. See [join.md](join.md#two-tiers-and-only-the-lower-one-is-scored).

## A keyed answer is only as honest as the row that named it

A keyed answer has no score because it is not inferred: the ledger row named the
transcript. But a row that carries `harness_session` and no `harness_subagent`
cannot prove a top-level agent wrote it. A subagent inherits its parent's session
id, so one that skipped resolving its own transcript id writes a row shaped
exactly like the orchestrator's, naming the **parent's** transcript.

recount catches the visible half of this. When the ledger records more than one
agent writing session-only rows under the same session, a keyed answer for any
of them is a **lead**, not an identity: same file, `confident: false`, exit `1`,
and the evidence line names the agents. This repository's own ledger trips it —
35 `orchestrator` rows and one `precheck` row share a session — so all 36 now
answer as leads, even though the one extra name was most likely the orchestrator
acting under a second name. The row cannot say which, and an identity the ledger
contradicts is not one. [join.md](join.md#what-the-keyed-tier-does-not-fix) has
the detail.

It cannot catch the other half: a subagent that skipped self-id in a session
whose parent never wrote a session-only row of its own. The ledger then names one
agent for the session — the subagent — and recount answers with the parent's
transcript as an identity, exit `0`, no warning.

**What to do about it:** make every subagent in your fleet resolve and export
its own transcript id before its first pact command, so its rows carry
`harness_subagent`. And give the orchestrator one name: a second `PACT_AGENT` for
the same session turns every one of its rows into a lead.

## recount is wrong on its own ledger, and its own output shows it

Run recount against the coordination log of its own construction. Measured on
2026-08-19, when that ledger held 177 events: 118 refused, 3 resolved correctly,
and **56 resolved to a match at 0.65 that named the wrong session**, all of them
above the confidence bar at the time — every one a subagent's transcript
standing in for the orchestrator event that was actually asked about.

Two separate things produce that. The topology filtered without discriminating,
which is the ceiling above. But among the candidates it could not tell apart,
the winner is decided by filesystem path order, and that ordering is
systematically biased against the session that was right — a defect (bead
`recount-gwx`, still open) rather than a limitation. The case is walked in
[join.md](join.md#recount-is-wrong-on-this-repositorys-own-ledger).

This is the arkanoid failure mode, reproduced by the tool built to prevent it.
It was caught because the machinery worked as designed: every one of those
matches carried an evidence line stating that the chain "filtered without
discriminating", precisely the sentence that tells a reader to treat it as a
lead. Since 2026-08-20 the tool draws that conclusion itself — the same 59
answers now print as leads and exit `1`. The wrong transcript is still the one
returned; the tool just no longer claims it is right.
[worked-example.md](worked-example.md) replays the case command by command, as
it ran before the lead rule.

**What to do about it:** read the verdict and the evidence line, not just the
score. A lead is somewhere to look in the transcript, never a verdict.

## Keep your agent worktrees until the audit is done

If your fleet deletes each agent's worktree when its work lands, the topological
chain has nothing left to resolve: the ledger names a path that no longer
exists. recount refuses correctly in that situation — the worktree is genuinely
gone — but the refusals are unrecoverable, and they are most of this
repository's own ledger for exactly that reason.

Audit first, tidy second. Or accept that only the mention signal survives, with
the ceiling that implies. `recount doctor` tells you up front how many of the
ledger's worktrees are still alive.

## Known and deliberate

- **The `~/.claude` format is reverse-engineered** from one machine and will
  drift. [format-notes.md](format-notes.md) says so in those words and carries
  its observation date.
- **`grep` parses each file once more than it needs to**, because removing that
  pass honestly requires a change to the connector interface that has not been
  justified yet. See
  [performance.md](performance.md#the-redundant-parse-in-grep-known-and-left-alone).
- **The fallback connector rarely sees a file.** `generic_jsonl` is reachable,
  but in practice it only receives what `claude_code` declines, because no
  command accepts a bare directory. See
  [connectors.md](connectors.md#is-generic_jsonl-reachable-through---repo).

## See also

- [join.md](join.md) — the tiers, the ladder, the refusals, and the measurements
  behind every number on this page.
- [session-learnings.md](session-learnings.md) — what went wrong when recount
  was used on four other repositories' ledgers.
