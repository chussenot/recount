# Connectors

A connector turns files on disk into sessions and turns. There are exactly two,
they are compiled in, and there is no plugin mechanism. `src/connector/mod.rs`
holds the seam and the rules below; the two implementations are
`src/connector/claude_code.rs` and `src/connector/generic_jsonl.rs`.

## The trait

`docs/spec.md` writes it as two methods — `discover()` and `read(path)` — "so a
third is one file — but do not add one". The code has those two, plus two that
exist only so `doctor` can report honestly:

| method | what it is for |
|---|---|
| `discover(dirs, window)` | the sessions this connector recognises under `dirs` |
| `read(path)` | every turn in one session file, in file order, 1-based line numbers |
| `roots()` | where this connector *would* look, whether or not those places exist |
| `name()` | the stable id stored on every `SessionMeta` and printed by `doctor` |

`discover` takes the directories rather than rediscovering them, and that is a
deliberate departure from the spec's `discover()`, recorded in the README's
deviations section. The spec requires both that `--repo` resolve to the
project's encoded session directories "before any file opens" and that mtime
"prune files outside the query window unopened". Those are the same requirement
seen from the caller's side: the scope has to reach the connector before the
connector opens anything.

`roots()` reports absent directories too, because "the root you expected is not
there" is the answer a person needs. `generic_jsonl` declares none, and
`doctor` prints "roots: none declared" rather than inventing a plausible path —
a made-up root reads as a place recount looked and found empty.

**No third method was added for claiming.** A connector claims a path by
returning a `SessionMeta` for it; `discover_all` records the claim. A
`claims(path) -> bool` method would be a second place for the same decision to
live, and the two would drift.

**No third connector.** Adding one is one file and one line in `registry()`,
which is cheaper than any mechanism that would make it cheaper. There is no
registration macro, no inventory crate, no factory keyed by string.

## The two

**`claude_code`** — sessions under `~/.claude/projects/<encoded-path>/*.jsonl`,
in both observed layouts: the top-level `<session-uuid>.jsonl` and the nested
`<session-uuid>/subagents/agent-<hex>.jsonl`. Three quarters of a real corpus is
in the nested shape, and all of the fleet testimony recount exists to retrieve.
The format is reverse-engineered, not documented by anyone: every field this
connector reaches for cites a section of [format-notes.md](format-notes.md),
which carries its own "expect drift" warning.

**`generic_jsonl`** — any directory of `*.jsonl` with `{ts|timestamp, role,
content|text}`, best-effort, and deliberately no larger than that one line of
spec. No cwd (so the topological join correctly refuses these sessions), no
block-type parser, no sniffing, no repair.

## Resolving a transcript by id

pact 0.11.0 events can name the transcript they were written from, and
`claude_code::keyed` turns that name into a path. It is deliberately the
smallest thing that can: one path join and one `is_file()`.

```text
<project_dir>/<session>.jsonl                              subagent: None
<project_dir>/<session>/subagents/agent-<subagent>.jsonl   subagent: Some
```

It is **not a fifth trait method**, for the same reason claiming is not a third
one: it is specific to a format that happens to name its files after its ids,
and putting it on the trait would ask `generic_jsonl` to have an opinion about
a key it has never heard of. The join calls the free function directly and
`Keyed` is its own type, not an `Option<PathBuf>`:

| outcome | means | what to do |
|---|---|---|
| `Found(path)` | the file is on disk | read it |
| `Absent(path)` | the ids are fine, the path is right, nothing is at it | look elsewhere — pruned, or the ledger came from another machine |
| `Malformed(why)` | the id is not an id; nothing was joined and nothing was stat'd | look at the ledger, not the disk |

Collapsing the last two into a `None` would send a human hunting on disk for a
transcript that was never named. [join.md](join.md) has what the join does with
each.

**Both ids are validated as opaque tokens**: non-empty, `[A-Za-z0-9-]` only.
That is not a uuid check and not a hex check, on purpose — this format is
reverse-engineered and expected to drift, and a stricter grammar would answer
`Malformed` to an id a later Claude Code mints perfectly legally. It is exactly
strict enough to make a join a join: no `/`, no `..`, no NUL, no `~`, so a
corrupt or hostile ledger cannot walk out of the directory it is joined into.
That is the trust boundary, and it is the reason the check is a shape check
rather than a taste check.

Nothing here opens the file. `agentId` inside it is the authority if the
filename is ever doubted ([format-notes.md](format-notes.md) §10.5), and a
caller who wants that certainty is already reading the file. The filename and
the field agreed in 604 of 604 files, so nothing is bought by checking it here.

Where the ids come from is [format-notes.md](format-notes.md) §10: an agent can
learn its own transcript id, and the only recipe that works is a marker grep
with a one-match-or-nothing guard. Do not reimplement it from memory — the
obvious recipe, newest file by mtime, was measured returning the *wrong* agent,
symmetrically, for two agents spawned in the same wave (§10.2). That measurement
is why the guard exists and why absence beats a plausible wrong id.

**The ceiling, measured 2026-08-19:** 371 of 634 subagent transcripts on this
machine live one level deeper, in `subagents/workflows/wf_<id>/`, and no pact
event carries a `wf_` id ([format-notes.md](format-notes.md) §11.0). Those
resolve `Absent` and belong to the topological join. That costs nothing today
only because the self-id recipe globs the flat directory, so an id that reached
an event came from a flat file by construction — a condition, not a property.
**Do not fix it by scanning `workflows/`.** If a future harness hands an agent
its id directly, the fix is one bounded `read_dir`, still not a walk.

## The model is recorded per turn, not per session

`Identity.models` carries every distinct model a file's assistant lines
recorded, most frequent first, ties by first appearance — the same rule as
`Identity.cwds`, so there is one rule to remember rather than two.

It is a set because the record is a set, not because the field is patchy:
`.message.model` is on **85,242 of 85,242 assistant lines** in this corpus
(2026-08-19, 880 files) and on no other line kind. 10 files switch between two
real models mid-transcript, 3 of them subagent files
([format-notes.md](format-notes.md) §11.1). A resumed session and a mid-run
`/model` both land there and nothing in the record tells them apart, so "the
model of a session" does not exist, exactly as "the cwd of a session" does not.
A connector that returned one value would be picking a winner and calling it a
fact.

The value is a bare slug, verbatim — `claude-opus-5`,
`claude-haiku-4-5-20251001` — the same shape a `PACT_MODEL` value has, which is
what makes the comparison a string comparison. Empty is a real and unalarming
state: a file with no assistant line records no model, and a comparison with
nothing to compare stays silent.

**`<synthetic>` is excluded, and that exclusion is load-bearing.** All 80 lines
carrying it are records Claude Code wrote itself, not model responses: 72 are
API errors it rendered as turns — 429, 401, 400, 529 — and 8 are its
"No response requested." no-op ([format-notes.md](format-notes.md) §11.2). The
*turns* are still emitted, because "the run stalled because the API said 429"
is exactly what an audit wants. Only the model is disowned. An agent that
declared `PACT_MODEL=claude-opus-5` and hit one rate limit would otherwise be
reported as having run a model it did not declare — a discrepancy manufactured
by the tool, in the one output whose entire purpose is to be believed.

The sidecar's own `model` key is a different thing and is still not read
(§11.1): it is on 70 of 634 sidecars, and it holds the spawn-time alias the
caller asked for (`sonnet`, `opus`, `haiku`), not the slug that answered.

`Identity` is what `scan(path)` returns beside the `SessionMeta`, so this is
connector-level today: as of 2026-08-20 no command prints `models` — `sessions`
reaches for `scan` only for `cwds`.

## The claim rule

**A path belongs to the most specific connector that recognises it**, and
`discover_all` enforces it. Both connectors accept `*.jsonl`, so a caller that
ran the whole registry over one set of directories would discover every file
twice — once as the transcript it is, once as the "0 turns, N declined" the
best-effort reader makes of the same bytes. `generic_jsonl` is last in
`registry()`, so it is the fallback by construction rather than by a flag: it
only ever reports what `claude_code` declined.

**Claiming is by recognition, and the window is applied once, centrally,
afterwards.** A connector claims a path because it understands the file, not
because it has something to say about it inside the caller's window. Those two
came apart once:

> `--until` once presented real 68–167-turn transcripts as **empty**. Each
> `discover` applied the window itself, so a transcript whose turns all fell
> after `--until` was dropped by `claude_code` — which silently released the
> claim, handed the path to `generic_jsonl`, and let the fallback report it as
> "0 turns, span null..null". An unbounded span overlaps every window by
> construction, so the phantom survived the very filter that had excluded the
> real reading of the same bytes.

So `discover_all` claims by path first, then applies the window once. The rule
generalises past the bug: whether a file is in the window is a property of the
*query*; only who understands the file can be a property of the *connector*.
The `window` a connector receives is a prune hint for `src/prune.rs`, nothing
more — see [performance.md](performance.md).

Two consequences worth knowing. A path the window excludes is still *claimed*,
so it cannot fall through to the fallback. And a connector with no unclaimed
path left to be offered is not asked at all — `discover` is a full parse, and a
fallback whose every result is about to be dropped as already-claimed once
parsed the whole corpus to produce nothing.

## The tolerance contract

"Unknown fields ignored, truncated tails counted not fatal, one bad line never
fails a file." In this codebase that is not a promise in prose, it is the
signature: `discover` and `read` return plain `Vec`s, never `Result`s. A
connector has nowhere to put a fatal error, so it cannot invent one. Damage is
reported as data — counted on `SessionMeta`, shown by `doctor`.

A `.jsonl` written by a process that is still running has a half-line at the
end. That is the normal case, not corruption. A directory that does not exist
yields an empty `Vec`, because a machine with no `~/.claude` is a normal
machine.

## `declined` is not `parse_failures`

Two counters on `SessionMeta`, and the distinction is the whole point.

- **`parse_failures` — damage.** A line the connector *tried* to read and could
  not: not JSON, not a JSON object, or a testimony line whose timestamp will
  not parse.
- **`declined` — policy.** A line the connector *could* have read and chose not
  to, for a reason it can name. In `claude_code` that is exactly one thing
  today: a line over `MAX_LINE_BYTES` (`src/connector/claude_code.rs`). In
  `generic_jsonl` it is a line that is perfectly good JSON but not turn-shaped.
- **Neither** covers a line kind the connector does not read. An `attachment`, a
  `queue-operation`: that is the format's structure, not a loss. Tallying
  10,309 attachments as "declined" would drown the twelve lines that actually
  went unread.

They were one counter once:

> `doctor` reported 12 declined lines as *"those transcripts are damaged"* when
> the corpus had zero corruption. All twelve were one runaway prompt, retried
> six times, in files with nothing wrong with them
> ([format-notes.md](format-notes.md) §5.1).

What `doctor` prints now, on this machine, 2026-08-19, debug build at `457a254`:

```
connectors
  claude_code    843 sessions, 12 lines declined, 0 damaged
                 root: /home/chussenot/.claude/projects (exists)
  generic_jsonl  0 sessions, 0 lines declined, 0 damaged
                 roots: none declared
```

```
findings
  - connector claude_code: 12 lines declined across 6 of 843 sessions — recount chose not to read them (over its line-size cap, or a shape this connector does not take); 0 damaged, so nothing under this root is corrupt
```

The two numbers point at different actions: `declined` is recount's own
configuration and is a decision to revisit, `damaged` is a reason to go and look
at the file.

## Is `generic_jsonl` reachable through `--repo`?

Yes, but only for what `claude_code` declines, and never for a real Claude Code
transcript. `--repo` resolves to this repo's encoded session directories, which
are `claude_code`'s own tree, so every transcript found there is claimed above
it. What is left is the workflow `journal.jsonl` records
([format-notes.md](format-notes.md) §2.4) — `started`/`result` rows with no
`cwd`, no `timestamp` and no `message`, which `claude_code` drops at discovery
rather than carrying forever as sessions that have no cwd.

There are none of those under this repo, so the check has to be run somewhere
that has them:

```
$ recount --json sessions --repo /home/chussenot/Documents/pact \
    | jq -c '[.sessions[].connector] | group_by(.) | map({connector:.[0], sessions:length})'
[{"connector":"claude_code","sessions":242},{"connector":"generic_jsonl","sessions":12}]
```

All twelve are `journal.jsonl`, all report 0 turns and 0 parse failures, and 374
lines declined between them — the fallback correctly saying "these are not
turns" about files that are not transcripts. An earlier version of this
documentation claimed the fallback was *unreachable* through `--repo`. It is
not, and the corrected sentence is the one above: it is an escape hatch for a
tree recount was pointed at, not a second opinion on the tree it already
understands. `doctor`'s whole-machine sweep gives it nothing either — that sweep
runs each connector over its own `roots()`, and `generic_jsonl` declares none.
