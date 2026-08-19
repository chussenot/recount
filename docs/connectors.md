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
