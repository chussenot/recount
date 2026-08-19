# CLI reference

Every command, every flag, every exit code, every `--json` shape. This is the
page to look things up in; [../README.md](../README.md) is the page that says
why any of it is shaped this way.

Verified against `recount 0.1.0`. `scripts/check-docs.sh` compares the
`Commands` block below against the built binary in both directions, so a flag
that is missing here, or offered here and rejected by the parser, fails the
build.

## Exit codes

| code | meaning |
|---:|---|
| 0 | answered |
| 1 | no confident match (`at`, `explain`), or a hard error |
| 2 | usage error from the parser (clap) |

**A refusal is an answer, not a failure.** `at` and `explain` exit 1 when they
decline to name a session, and they print the reason and the evidence while
doing it. A caller that treats exit 1 as an error has it backwards: the tool
exists because a confident wrong answer is worse than a scored one, and exit 1
is where the scoring shows. Distinguish the two cases by the payload — under
`--json`, `outcome` is `"low_confidence"` or `"refused"` for a reasoned
non-answer, and a hard error writes a single `recount: …` line to stderr with
no document at all.

`doctor` is the exception with the opposite bias: it **always exits 0**, on
every input, including a `--repo` that is not a git repository or does not
exist. Everything it finds is reported as a finding rather than raised. A
doctor that exits non-zero gets wrapped in `|| true` by every script that calls
it, and then its exit code means nothing. The machine-readable verdict is
`.join_readiness.ready`.

## The `--json` contract

`--json` is a global flag; it is accepted before or after the subcommand.

| mode | stdout | stderr |
|---|---|---|
| with `--json` | exactly one JSON document | the human digest, second |
| without | the human digest, alone | nothing |

Both streams always carry the same answer, and the digest is byte-identical
between the two modes: `tests/degradation.rs` asserts `json.stderr ==
plain.stdout` for every command against every hostile tree it builds. That is a
stronger statement than "both modes work" — it means adding a fact to one
stream cannot silently skip the other.

Every document is the same envelope: `{"command": …, "schema": 1, …payload}`.
`schema` bumps only when a field changes meaning or disappears; added fields do
not bump it, so a reader must ignore what it does not recognise.

Never half a document: on a hard error the JSON document is not written at all,
rather than truncated. A partial document that parses is worse than none.

**`completion` is the one deviation.** The spec's rule is "all `--json`; human
digest second", and `completion` does not follow it: its output is a shell
program, and a digest printed beside it on the same run would be sourced as
code by the very redirect the command is designed for. `--json` is therefore
accepted and ignored there — the script goes to stdout alone, byte-identical
with and without the flag, and stderr stays empty. This deviation is recorded
in the README rather than patched into the frozen `spec.md`.

## Commands

Six subcommands. `--json` is global and works on all of them.

### `recount sessions`

```
recount sessions --repo <PATH> [--since <WHEN>] [--until <WHEN>] [--json]
```

Sessions whose cwd resolves into the repo or its worktrees, each with a
fleet-identity *guess* attached. `--repo` is required and is resolved to the
encoded session directories of the repo and its worktrees before any file is
opened.

`--since` and `--until` take RFC 3339 (`2026-08-19T07:13:57Z`) or a duration
ago (`2h`, `1d`). Only `--since` prunes files unopened: transcripts are
append-only, so mtime bounds the *last* turn and never the first — see
[performance.md](performance.md).

Nothing here states an identity as a fact. The key is `fleet_identity_guess`,
its `agent` is nullable, and its `evidence` is never empty: when the agent is
null the evidence says why it could not be told, which is itself the answer.
See [join.md](join.md).

### `recount at`

```
recount at --repo <PATH> --agent <NAME> --ts <RFC3339> [--window <DURATION>] [--json]
```

Resolve the session driving a fleet identity at a moment, and print the
transcript window around it. `--agent` is the identity as it appears in the
pact ledger; `--ts` is RFC 3339. `--window` is the **half-width** either side
of `--ts`, default `5m`, so the default window is ten minutes wide.

Three outcomes behind two exit codes:

| `outcome` | exit | printed |
|---|---:|---|
| `match` | 0 | session, confidence, evidence, transcript window |
| `low_confidence` | 1 | session, confidence, evidence — no transcript |
| `refused` | 1 | the reason (`no cwd match` / `no overlap`) and its detail |

`low_confidence` prints the candidate on purpose: both structural gates passed
and what fell short was the arithmetic, and that evidence is what tells an
auditor where to look next. It withholds the *transcript* on purpose too —
under a topology where every session in the repo overlaps every moment, a
sub-threshold candidate's transcript is *a* transcript, not *the* one, and
printing it under a heading that reads like testimony is exactly the
misdiagnosis recount was built to prevent. The path is printed, so
`recount grep` is one command away for a reader who decides the evidence
justifies it.

`refused` prints no candidate, also on purpose: the chain produced nobody to
score. The two reasons are not interchangeable — `no cwd match` sends you to
the topology, `no overlap` sends you to the clock.

### `recount grep`

```
recount grep <NEEDLE> --repo <PATH> [--agent <NAME>] [--since <WHEN>] [--until <WHEN>] [--json]
```

Case-insensitive substring hits with their coordinates: session, line, ts,
role, snippet. Not a regex, by design — the README's refusals list "no fuzzy
search" as a feature, and there is no flag to turn it into one.

`--agent` restricts to sessions attributed to that fleet identity; sessions
whose identity could not be guessed are reported separately rather than
silently dropped. An empty needle is the one input `grep` refuses (exit 1):
every line contains the empty string, so that is not a search. No hits is exit
0, not an error.

### `recount explain`

```
recount explain --repo <PATH> --event-line <N> [--window <DURATION>] [--json]
```

Read one event out of `.pact/events.jsonl` and run `at` on it: one command from
an audit finding to the agent's own words. `--event-line` is 1-based over
*physical* lines, so `--event-line 5` is the line `sed -n 5p` shows, whether or
not line 5 parsed. A line past the end is a hard error naming the ledger's
length, exit 1.

`explain` resolves more than `at` can from the command line, because the event
also carries the worktree it was invoked from and any bead id in its `detail`
field — signals `at` has no flags for. Exit codes are inherited from `at`
unchanged, and so is the JSON envelope: the document says `"command": "at"`,
with the explain header appearing only in the digest.

### `recount doctor`

```
recount doctor [--repo <PATH>] [--json]
```

Connector roots, session counts, declined-versus-damaged tallies, and both
join-readiness questions: is there a ledger, are the worktrees still alive.
`--repo` defaults to `.` rather than being required, because the spec writes
`recount doctor` bare.

`declined` and `parse_failures` are separate counters and the distinction is
load-bearing. `doctor` once reported 12 declined lines as *"those transcripts
are damaged"* on a corpus with zero corruption — a line over the size cap, or
of a shape a connector does not take, is a line recount **chose** not to read,
not a broken one. See [connectors.md](connectors.md).

Always exits 0. See [Exit codes](#exit-codes).

### `recount completion`

```
recount completion <SHELL>
```

Print a shell completion script to stdout. `<SHELL>` is one of `bash`,
`elvish`, `fish`, `powershell`, `zsh`. Generated from the same clap command
tree the parser uses, so it cannot offer a subcommand or a flag the binary does
not have. An unknown shell is a usage error, exit 2.

This is the command that does not follow the `--json` rule; see
[The `--json` contract](#the---json-contract) above. Installation and refresh
are in [install.md](install.md).

## JSON shapes

Elisions are marked `…`; everything else is the real key set, taken from
running each command. `null` in a position means the field is present and null,
never absent.

### `sessions`

```jsonc
{ "command": "sessions", "schema": 1,
  "repo": "/abs/path",                      // canonical
  "window": { "since": <rfc3339|null>, "until": <rfc3339|null> },
  "session_dirs": ["/home/u/.claude/projects/-abs-path", …],
  "discovered": 4,                          // before the cwd filter
  "excluded_by_cwd": 0,                     // said a cwd, outside the repo
  "sessions": [ {
      "path": "…/-abs-path/<uuid>.jsonl",
      "connector": "claude_code",
      "cwd": <path|null>,
      "first_ts": <rfc3339|null>, "last_ts": <rfc3339|null>,
      "turn_count": 660, "parse_failures": 0, "declined": 0,
      "worktree": <name|null>,              // null: the cwd did not say
      "repo_match": "cwd" | "session_dir",
      "fleet_identity_guess": {
        "agent": <name|null>,               // null: we cannot tell
        "confidence": 0.0,
        "evidence": [ { "signal": "topology", "detail": "…" }, … ] } } ],
  "warnings": [] }                          // e.g. a worktree git lists but is gone
```

### `at`, and `explain`

```jsonc
{ "command": "at", "schema": 1,             // "at" for `explain` too
  "repo": "/abs/path", "agent": "skeleton",
  "ts": "2026-08-19T07:13:57Z",
  "window": { "since": "…T07:08:57Z", "until": "…T07:18:57Z" },
  "session_dirs": ["/home/u/.claude/projects/-abs-path", …],
  "considered": 29,                         // sessions the window could not rule out
  "threshold": 0.5,
  "outcome": "match" | "low_confidence" | "refused",
  "best": { "session": { "path": …, "cwd": …, "first_ts": …, "last_ts": …,
                         "turn_count": …, "parse_failures": …, "declined": …,
                         "connector": "claude_code" },
            "confidence": 0.65, "confident": true,
            "evidence": [ { "signal": "topology", "detail": "…" },
                          { "signal": "time_overlap", "detail": "…" },
                          { "signal": "transcript_mention", "detail": "…" } ],
            "transcript": [ { "ts": …, "role": …, "text": …, "line": … }, … ] },
  "refusal": { "reason": "no cwd match" | "no overlap", "detail": "…" } }
```

`best` and `refusal` are exactly one non-null each. `transcript` is `[]` unless
`outcome` is `"match"`. `threshold` is emitted rather than assumed, so a reader
never has to know where `CONFIDENT` lives in `src/join.rs` to interpret
`confidence`.

### `grep`

```jsonc
{ "command": "grep", "schema": 1,
  "needle": "arkanoid", "repo": "/abs/path",
  "window": { "since": <rfc3339|null>, "until": <rfc3339|null> },
  "agent": <name|null>,
  "session_dirs": [ … ],
  "sessions_searched": 3,
  "sessions_skipped_by_agent": 0,
  "sessions_skipped_unknown_identity": [],  // paths, not a count: not silently dropped
  "turns_searched": 98,
  "hits": [ { "session": "…/<uuid>.jsonl",
              "line": 7,                    // 1-based, physical: `sed -n 7p`
              "ts": "2026-08-19T07:00:48.575Z",
              "role": "user",
              "snippet": "…",
              "occurrences": 17,            // in that turn; nothing is hidden
              "block_guess": <"tool_result"|…|null> } ] }
```

### `doctor`

```jsonc
{ "command": "doctor", "schema": 1,
  "repo": "/abs/path",
  "connectors": [ { "name": "claude_code",
                    "roots": [ { "path": "/home/u/.claude/projects", "exists": true } ],
                    "sessions": 843, "parse_failures": 0, "declined": 12 },
                  { "name": "generic_jsonl", "roots": [], … } ],
  "join_readiness": {
    "ready": true,                          // the machine-readable verdict
    "repo_is_git": true,
    "events_path": "/abs/path/.pact/events.jsonl",
    "events_present": true, "events": 177, "event_parse_failures": 0,
    "worktrees": [ { "name": "main", "path": "/abs/path", "alive": true,
                     "session_dir": "/home/u/.claude/projects/-abs-path",
                     "session_dir_exists": true, "sessions": 29 } ],
    "repo_sessions": 29 },
  "findings": [ "connector claude_code: 12 lines declined across 6 of 843 sessions — …" ] }
```

### `completion`

No JSON. See [The `--json` contract](#the---json-contract).

## See also

- [install.md](install.md) — building, installing, completions
- [join.md](join.md) — the topological key, the score ladder, the two refusals
- [connectors.md](connectors.md) — what each connector claims and declines
- [performance.md](performance.md) — the mtime prune and the two ceilings
- [worked-example.md](worked-example.md) — one real finding, replayed
- [development.md](development.md) — the gates, and why each exists
