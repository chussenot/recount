# Performance

recount owns no index — "session files are the only truth", and a store recount
does not own is a store that can drift. Scans are made cheap structurally
instead, by two moves that both happen before any file is opened:

1. `--repo` resolves to the project's encoded session directories (repo plus
   worktrees) — a `git worktree list` and a path encoding, no scanning
   (`src/worktree.rs`).
2. Sessions are append-only, so mtime rules files outside the query window out
   *unopened* (`src/prune.rs`).

## The mtime prune is one-sided, and that matters

An append-only file's mtime is the time of its **last** write. It bounds the
last turn and says nothing about the first. Therefore:

- A file whose mtime is **before** `--since` cannot hold a turn inside the
  window: every turn it holds is older than its mtime. Skipped, unopened. **The
  lower bound prunes.**
- A file whose mtime is **after** `--until` may still hold turns from before
  `--until` — a session that started this morning and was appended to a minute
  ago. It must be opened. **The upper bound does not prune.**

A naive symmetric prune looks tidier and silently loses hits. That is why the
asymmetry is written down here and in the module doc rather than left to the
reader of an `if`. An unbounded window prunes nothing and does not make a single
`stat` call. A file whose mtime cannot be read is opened, not skipped:
uncertainty resolves toward looking.

### The invariant was measured, not assumed

[format-notes.md](format-notes.md) §6.2 measured `mtime − last_timestamp` over
842 real transcripts on this machine: **min 0.0 s, median 0.1 s, p99 ~106,800 s
(~30 h), max 1,963,022 s (22.72 days)** — and **not one file** with an mtime
earlier than its last timestamp.

Both halves license one decision each:

- No inversions in 842 files is what licenses the lower-bound prune. It is an
  arithmetic near-certainty — the file is written when the line is written — but
  a clock step or an NTP correction could invert it, so the prune is kept and
  never asserted in a way that panics.
- Overshoot of up to 22.72 days is why the upper bound is refused. The
  overshoot comes from timestamp-less metadata lines (`last-prompt`,
  `ai-title`, `agent-name`) rewritten when a session is resumed or renamed in
  the UI. Overshoot is **safe downward** — it only makes recount open a file it
  could have skipped, so the prune is conservative, never precise — and **fatal
  upward**: a file touched today can hold nothing but last month's turns.

For the same reason, never report mtime to a user as "last activity".

## The two spec ceilings

`docs/spec.md` sets two: pruned `at` under 100 ms, and unwindowed `grep` over
500 synthetic sessions (~200 MB) under 2 s. A number without its conditions is a
rumour, so here are the conditions, and they are printed by the bench itself:

- **build profile:** release. `mise run bench` encodes it rather than trusting a
  human to remember, and `tests/bench.rs` refuses to assert under
  `debug_assertions` at all: the same grep is recorded there at 35.9 s in debug
  against ~1.06 s in release, which is a number about the profile, not the code.
- **hardware:** AMD Ryzen 7 PRO 8840HS w/ Radeon 780M, 16 cores visible, Linux.
- **corpus:** 500 synthetic sessions, 40 turns each, 193.0 MB, written into a
  tempdir and deleted on exit.
- **page cache:** warm — the corpus was written moments before. Two runs are
  reported so a cold-ish first run is visible if there is one.

Run on 2026-08-19 at `457a254` with `mise run bench`:

```
SPEC CEILING 1 — pruned `at`, answering path            < 100 ms
  run 1     12.8 ms   run 2     12.4 ms   exit 0   [499 of 500 sessions pruned on mtime, unopened]
  MET — 12% of the ceiling

(not a ceiling) `at`, explaining path — the unwindowed re-scan
  run 1    313.9 ms   run 2    311.9 ms   exit 1   [all 500 sessions re-opened to name the true refusal]
  This is the price of `no overlap` being a distinct reason from `no cwd match`.

SPEC CEILING 2 — unwindowed `grep` over ~200 MB          < 2 s
  run 1    942.4 ms   run 2    937.8 ms   exit 0   [500 sessions, 20000 turns searched]
  MET — 47% of the ceiling
```

The 499-of-500 line is the prune doing the work the spec's cheapness argument
assumes: `at` answers in 12 ms because it opens one file.

The explaining path is not a ceiling and is not free. When the cheap windowed
pass refuses on topology, `at` re-scans unwindowed to find the true refusal
reason, because the sessions a `no overlap` refusal must cite are exactly the
ones the prune removed. 312 ms is what that costs, and it is paid only by a run
that is about to explain itself. [join.md](join.md) has the reasoning.

## The same commands on a real corpus

Synthetic corpora are uniform in a way real ones are not, so the bench also runs
read-only against a real `~/.claude/projects` tree — here the one for
`/home/chussenot/Documents/pact`: 1 session directory, 254 sessions, 173.1 MB,
page cache warm-ish (whatever the OS held), same build and machine as above.

```
real unwindowed `grep`  run 1   1903.5 ms   run 2   1824.8 ms   [50692 turns searched]
real   windowed `grep --since 24h`  run 1    129.3 ms   run 2    130.9 ms   [6 of 254 sessions survived the mtime prune]
  for scale: `recount sessions` over the same corpus — one discovery parse, no content
  scan — is   1241.8 ms. `grep` parses twice and then scans every turn.
real `at --agent bench --ts now`  run 1    130.3 ms   run 2    133.3 ms   exit 0 (answering path, 1 session(s) survived the prune)
```

A real corpus is roughly twice as slow per megabyte as the synthetic one, and
the windowed query is fourteen times faster than the unwindowed one on the same
files. That ratio is the prune, and it is the whole argument for not having an
index.

## The sidecar manifest, and why it is not being built

The spec records one deferral: *a metadata-only sidecar manifest (rebuilt
wholesale on any mtime mismatch, rm-safe) may be added **iff real windowed
queries exceed ~2 s on a real corpus** — never a content index, never a
database.*

The clause names windowed queries on a real corpus, and the bench measures
exactly that:

```
  real WINDOWED grep:      130.9 ms  -> NOT triggered. The mtime prune is doing its job; do not build the manifest.
  real UNWINDOWED grep:   1824.8 ms  (not what the clause names, but the worst case)
```

130.9 ms against a 2 s trigger. **Not triggered — do not build it.** Even the
unwindowed worst case is inside the number, and it is not the number the clause
names.

There is a second reason beyond the threshold. A manifest holds metadata, so the
most it could ever remove is the *discovery* parse; the `read` parse and the
character scan stay. Compare the 1241.8 ms `sessions` figure against the
1824.8 ms unwindowed `grep` above before concluding that a manifest is what the
cost is made of.

## The redundant parse in `grep`, known and left alone

`grep` still parses the corpus **twice**: once in `discover_all`, and once in
the `connector.read` loop that produces the turns it searches. On the synthetic
corpus that second parse is worth roughly 540 ms of the 940 ms total — the
largest single item left, now that the scan itself is cheap (`eq_ci` answers an
ASCII pair without building a `char::to_lowercase()` iterator).

It stays, deliberately. Removing it means handing `discover` its parsed turns
back, which is a change to the `Connector` trait — a seam `docs/spec.md`
describes and nobody has authorised widening. The cost is measured, recorded
here and printed by the bench every time it runs; that is preferable to a trait
change made quietly for 540 ms.

## Running it yourself

`mise run bench` (release, minutes, ~193 MB into a tempdir, cleaned up on exit).
It is deliberately **not** part of `mise run check`: a required gate that writes
193 MB and takes minutes is a gate people disable. Run it when you have touched
the connectors, the prune, `grep`, or the join. See
[development.md](development.md) for the gates that *are* required.
