# format-notes: the real `~/.claude` layout, observed

**Observed: 2026-08-19, in two passes.** Machine: this one — Linux 6.17
(Ubuntu OEM kernel), `/home/chussenot`, running the `recount` repo itself.

* **Pass A — 2026-08-19 ~07:20 local (wave 0).** The survey this document was
  written from: 871 `.jsonl` files, 169,775 lines. Every unqualified number
  below is from pass A.
* **Pass B — 2026-08-19 ~10:05 local (wave 3).** A re-measurement taken while
  reconciling these notes against the shipped `claude_code` connector and
  `recount doctor`. Where B disagrees with A the text says so and dates both.
  §0 carries the full diff; §5 carries the one that mattered.

Claude Code versions seen in pass A: 22 distinct, `2.1.215` … `2.1.234`,
dominated by `2.1.220` (38,797 lines) and `2.1.233` (26,058).

> **REVERSE-ENGINEERED, EXPECT DRIFT.** Nothing here comes from a published
> schema. Every statement below was measured on one machine, on one day, over
> one corpus. Anthropic changes this format between point releases — the corpus
> already spans 22 versions and the field sets differ between them (see
> §2.2). Treat this document as a snapshot to be re-measured, never as a
> contract. The connector must degrade, not assert. Two passes 2h45m apart on
> the same machine on that one day already disagree — on the file count, the
> line count, the field inventory and the earliest timestamp in the corpus
> (§0, §0.0, §2.2). In one phrase: this document is reverse-engineered, expect
> drift.

## 0. The corpus this was measured on

**Two denominators, and they are not the same number.** A *file* is any
`.jsonl` under the root. A *session* is a file the connector will open:
files **minus** the 29 `journal.jsonl`, which are not transcripts (§2.4).
`recount doctor` reports sessions. This document's older tables say "871
files"; `doctor` says "828 sessions"; both were right on their own day and of
their own thing.

| Thing | Pass A (07:20) | Pass B (10:05) |
|---|---:|---:|
| Project directories under `~/.claude/projects/` | 48 | **41** |
| …of which contain at least one `.jsonl` | 43 | **36** |
| `.jsonl` files (all depths) | 871 | **857** |
| …top-level session files (`<project>/<uuid>.jsonl`) | 238 | **216** |
| …subagent files (`<project>/<uuid>/subagents/agent-<hex>.jsonl`) | 604 | **612** |
| …workflow journals (`…/subagents/workflows/wf_<id>/journal.jsonl`) | 29 | 29 |
| **sessions** = files − journals (what `doctor` counts) | 842 | **828** |
| `.meta.json` sidecars (one per subagent file) | 604 | **612** |
| Total JSONL lines | 169,775 | **163,633** |
| Total bytes | 754.7 MB | **776.8 MB** |
| Externalised `tool-results/*.txt` | 87 | **91** |
| Earliest timestamp | `2026-07-02T12:33:20.334Z` | **`2026-07-02T19:39:56.172Z`** |
| Latest timestamp | `2026-08-19T07:20:00.504Z` | `2026-08-19T08:09:49.625Z` |

Reproduce pass A: `scan.py`, `probe.py`, `probe2.py`. Reproduce pass B:
`corpus.py` and `failscan.py` (all single-pass Python walks in the session
scratchpad, ~40 s over the whole corpus). Pass B's session count agrees with
`recount --json doctor` on the same machine to the file: 828.

### 0.0 The corpus is live in **both** directions — it grows *and* it forgets

Pass A counted 149,297 timestamped lines and, minutes later, 149,351. The
54-line delta was not an error: the live session file
`-home-chussenot-Documents-recount/cd7c75eb-….jsonl` grew between the passes.
**A reader that stats a file and then reads it may see more bytes than it
stat'd.**

Pass B, 2h45m later, found the corpus **smaller**: 14 fewer files, 6,142 fewer
lines, 22 fewer top-level sessions, seven fewer project directories — while
subagent files went *up* by 8 (this fleet's own transcripts) and the earliest
timestamp in the whole corpus moved **forward** by seven hours. The corpus is
22 MB *larger* over the same interval, which is not a contradiction: the files
that arrived are this fleet's own transcripts and are far above average size,
while the ones that left were small and old. Old sessions are being deleted
underneath you. Every file this document cites by name survived,
so nothing here went stale, but the counts did.

*Mechanism not proven* (§8): the oldest surviving `.jsonl` mtimes bunch at
2026-07-20, thirty days before the measurement, which is what Claude Code's own
`cleanupPeriodDays` retention (default 30) would produce. `recount` itself
deletes nothing — every `fs::remove_*` and `File::create` in the tree is inside
`#[cfg(test)]`, against a `tempfile::TempDir`. Verified while writing this.

Consequences, and they are the point of measuring twice:

- **Never cache a scan and trust it.** A path that resolved an hour ago can be
  gone; `discover()` re-walking every time is correct, not wasteful.
- **A missing session is normal, not an anomaly.** A pact event that names a
  session recount can no longer find is an expected `doctor` finding, not
  corruption — the transcript aged out.
- **Absence of testimony is not testimony of absence.** `recount` must never
  say "that agent said nothing"; only "no transcript for it exists today".

### 0.1 Directory layout, actual

```
~/.claude/projects/
  -home-chussenot-Documents-pact/                      # encoded project path
    416610a6-0d95-4ddf-8c70-7915d9c4ff33.jsonl         # a session (238 of these)
    416610a6-0d95-4ddf-8c70-7915d9c4ff33/              # sidecars for that session
      subagents/
        agent-a1a724549638348da.jsonl                  # a subagent transcript
        agent-a1a724549638348da.meta.json              # {agentType, description, toolUseId, spawnDepth[, model]}
        workflows/wf_5b13f9d2-abb/
          agent-<hex>.jsonl                            # subagent spawned inside a workflow
          journal.jsonl                                # NOT a transcript — see §2.4
      tool-results/bahd13vhz.txt                       # externalised tool output (87 files at pass A, 91 at pass B; §8.1)
    memory/MEMORY.md                                   # not session data
```

Five project dirs contain **only** a `memory/` dir and no `.jsonl` at all
(`-home-chussenot-Documents-reverse-bins`, `-home-chussenot-Downloads-debug-dd-otel`,
`-tmp-tmp-H6sINtW4dH`, `-tmp-tmp-oNsNEYLxnI`, `-tmp-tmp-VkZluK584B`).
Discovery must tolerate a project directory with zero sessions.

**Shell hazard (not a Rust hazard):** every project directory name begins with
`-`. `head "$f"` where `$f` starts with `-home-…` is parsed as an option and
fails. Any shell tooling in tests or `doctor` must prefix `./`. Rust's
`std::fs` is unaffected.

---

## 1. The directory-name encoding

### 1.1 The rule

> Take the **absolute** path. Replace **every character that is not
> `[A-Za-z0-9]` with a single `-`**, one dash per character, no collapsing, no
> escaping. Case is preserved.

Equivalently: `path.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()`.

### 1.2 Evidence

Derived, not assumed: for each project dir I read the `cwd` field out of its
own session files and re-encoded it. **41 of 48 directory names are reproduced
exactly** by the rule above. The 7 that are not: 5 dirs have no `.jsonl` to
read a `cwd` from, and 2 are worktree dirs whose files only ever record the
*parent repo* as `cwd` (§4.2) — neither is a counter-example to the rule.

| Real path (from a `cwd` field) | Directory name | Character(s) proven |
|---|---|---|
| `/home/chussenot` | `-home-chussenot` | leading `/` → `-`, `/` → `-` |
| `/home/chussenot/.dotfiles` | `-home-chussenot--dotfiles` | `.` → `-` (hence the doubled dash) |
| `/home/chussenot/Documents/blv_claude_marketplace` | `-home-chussenot-Documents-blv-claude-marketplace` | **`_` → `-`** |
| `/home/chussenot/Documents/codefactory.believe.tech/idp/run-plane/observability/terrasync` | `-home-chussenot-Documents-codefactory-believe-tech-idp-run-plane-observability-terrasync` | `.` and `/` and literal `-` all → `-` |
| `/tmp/tmp.5GCCO7j3pI` | `-tmp-tmp-5GCCO7j3pI` | case preserved, digits preserved |
| `/home/chussenot/Documents/cost-reallocator/.claude/worktrees/fix-mise-python-tool` | `-home-chussenot-Documents-cost-reallocator--claude-worktrees-fix-mise-python-tool` | **worktree example** |
| `/home/chussenot/Documents/datadog-templator/.claude/worktrees/chore+pdm-to-uv` | `-home-chussenot-Documents-datadog-templator--claude-worktrees-chore-pdm-to-uv` | **`+` → `-`** |

The `--claude-worktrees-` infix that identifies a Claude-Code worktree
directory is just `/` + `.claude` → `-` + `-claude`. There is no special-casing.

### 1.3 THE ENCODING IS LOSSY. This is the most important fact in this document.

`/`, `.`, `_`, `+`, and a literal `-` **all encode to the same `-`** and are
indistinguishable in the encoded form. Confirmed on this machine for `/`, `.`,
`_`, `+`, and `-` (rows above). It follows that:

- **Decoding is impossible.** Given `-home-chussenot-Documents-blv-claude-marketplace`
  you cannot recover `blv_claude_marketplace`. `-a-b` could be `/a/b`, `/a.b`,
  `/a_b`, `/a-b`, `/a/.b`, …
- **The encoding is not injective.** Two distinct real paths can own the same
  directory name. No actual collision exists on this machine today, but
  `.../blv_claude_marketplace` and `.../blv-claude-marketplace` would collide,
  and the first of those is real.
- **Therefore `--repo <p>` must go forward, never backward.** Encode the
  candidate paths you already know (the repo root, plus every worktree path you
  can enumerate from `git worktree list` and from `<repo>/.claude/worktrees/*`
  and `<repo>/wt/*`) and look those names up. Never parse a directory name back
  into a path. The spec's "`--repo` resolves to the project's encoded session
  directories (repo + worktrees) before any file opens" is exactly right and is
  the only sound direction.
- Because it is not injective, a directory that encodes to your repo's name is
  **evidence, not proof**. If you need certainty, confirm against the `cwd`
  inside the files — but see §4, `cwd` lies in its own way.

---

## 2. Line kinds actually present

`type` is present on **every one of the 169,775 lines** (the per-type counts
below sum to exactly 169,775). A line without `type` was never observed —
which is not a licence to panic on one.

| `type` | Lines | % | `cwd`? | `timestamp`? | `message`? | Carries testimony? |
|---|---:|---:|---|---|---|---|
| `assistant` | 85,031 | 50.1% | always | always | yes | **YES** |
| `user` | 49,039 | 28.9% | always | always | yes | **YES** |
| `attachment` | 10,309 | 6.1% | always | always | no (`attachment` obj) | context only |
| `last-prompt` | 4,737 | 2.8% | never | never | no | no |
| `mode` | 4,119 | 2.4% | never | never | no | no |
| `permission-mode` | 4,065 | 2.4% | never | never | no | no |
| `ai-title` | 3,684 | 2.2% | never | never | no | title only |
| `system` | 2,108 | 1.2% | always | always | no (`content` str **on 17% only** — §2.3) | hooks/compaction |
| `queue-operation` | 2,076 | 1.2% | never | **yes** | no (`content` str) | queued user text |
| `agent-setting` | 891 | 0.5% | never | never | no | no |
| `file-history-snapshot` | 858 | 0.5% | never | never | no | no |
| `file-history-delta` | 729 | 0.4% | never | **yes** | no | file provenance |
| `agent-name` | 617 | 0.4% | never | never | no | no |
| `worktree-state` | 420 | 0.2% | never | never | no | **join gold, §4.3** |
| `relocated` | 387 | 0.2% | never | never | no | **join gold, §4.3** |
| `started` | 371 | 0.2% | never | never | no | journal only |
| `result` | 328 | 0.2% | never | never | no | journal only |
| `frame-link` | 5 | 0.003% | never | **yes** | no | no |
| `atis-latch` | 1 | 0.0006% | never | never | no | no |

**There is no `summary` line kind on this machine.** Older write-ups of this
format describe one. It does not occur in 169,775 lines here; `ai-title` and
`last-prompt` occupy that niche. Do not key anything on `summary`; do not
assume its absence either.

### 2.1 Lines that carry NONE of cwd/timestamp/role/content — skip these

Fifteen of the nineteen types never carry `cwd`: everything except `assistant`,
`user`, `attachment`, `system`. Twelve types never carry `timestamp` either.
The pure-metadata kinds are tiny, single-purpose objects, e.g.

```json
{"type":"mode","mode":"normal","sessionId":"852e9d96-…"}
{"type":"permission-mode","permissionMode":"auto","sessionId":"852e9d96-…"}
{"type":"ai-title","aiTitle":"number sequence generation","sessionId":"852e9d96-…"}
{"type":"agent-name","agentName":"number sequence generation","sessionId":"852e9d96-…"}
{"type":"agent-setting","agentSetting":"claude","sessionId":"852e9d96-…"}
{"type":"atis-latch","atis":"","sessionId":"852e9d96-…"}
```

These have no `uuid` and no `timestamp`, so they are not in the `parentUuid`
chain and cannot be ordered. They are **rewritten in place at the tail of the
file when the session is resumed or renamed** — which is what makes file mtime
outrun the last real turn by up to 22 days (§6.2).

Two non-obvious ones worth reading rather than skipping:

- **`worktree-state`** — the whole Claude-Code worktree record:
  `{"type":"worktree-state","worktreeSession":{"originalCwd":"/home/chussenot/Documents/datadog-templator","preEnterOriginalCwd":"/home/chussenot","worktreePath":"/home/chussenot/Documents/datadog-templator/.claude/worktrees/otel-flag","worktreeName":"otel-flag","worktreeBranch":"feat/otel-migration-flag","sessionId":"165211c6-…","enteredExisting":true},"sessionId":"165211c6-…"}`
- **`relocated`** — `{"type":"relocated","sessionId":"165211c6-…","relocatedCwd":"/home/chussenot"}`

### 2.2 Where cwd / timestamp / role / content live

For the two kinds that matter (`assistant`, `user`), all four are top-level or
one level in:

- **cwd** → top-level `.cwd` (absolute path string).
- **timestamp** → top-level `.timestamp`.
- **role** → `.message.role`. Observed values: exactly `"assistant"` on all
  85,031 `assistant` lines and exactly `"user"` on all 49,039 `user` lines.
  `.type` and `.message.role` never disagreed. **Prefer `.type`** — `message`
  is absent on 20% of lines, `type` is present on 100%.
- **content** → `.message.content`.

Full top-level field inventory seen on `assistant`/`user` lines. Pass A found
24 names; **pass B, on a corpus 4% smaller, found 46** — the extra 23 are not
new to the format, they are what a second look finds, which is the whole
argument of this document. Counts are pass B, out of 130,955 `assistant`/`user`
lines.

*On every line (130,955/130,955):* `parentUuid`, `isSidechain`, `type`,
`message`, `uuid`, `timestamp`, `cwd`, `sessionId`, `userType`, `entrypoint`,
`version`, `gitBranch`.

*On some:* `agentId` (83,651), `requestId` (82,866, assistant), `effort`
(81,434, assistant), `attributionAgent` (52,345), `slug` (49,941), `promptId`
(47,362, user), `sourceToolAssistantUUID` (45,986, user), `session_id` (41,485
— yes, both spellings, same value), `toolUseResult` (29,427, user),
`sessionKind` (12,397), `attributionSkill` (2,684), `promptSource` (1,046),
`permissionMode` (1,044), `attributionMcpServer` / `attributionMcpTool` (1,021
each), `origin` (922), `attributionPlugin` (413), `toolDenialKind` (363),
`toolEndsTurn` (227), `isMeta` (184), `classifierMetaLines` (165),
`isApiErrorMessage` (79), `error` (72), `apiErrorStatus` (68), `mcpMeta` (67),
`interruptedMessageId` (57), `sourceToolUseID` (43), `imagePasteIds` (18),
`errorDetails` (15), `queuePriority` (14), `isVisibleInTranscriptOnly` (12),
`isCompactSummary` (12), `userFeedback` (3), `isAbortedMidStream` (1).
`logicalParentUuid` rides on `system` compaction lines, not these.

Three of those are worth a reader's attention even though nothing reads them
today: `isApiErrorMessage`/`error`/`apiErrorStatus` mark an `assistant` line
that is a **transport error, not the model speaking** (79 lines), and the
connector emits it as an ordinary assistant turn; `isCompactSummary` (12) is
the compaction summary that older write-ups called a `summary` *line kind*
(§2) — it is a flag on a `user` line here, not a kind; `isMeta` (184) marks
injected rather than typed user text.

Field-set drift within the corpus, for the "expect drift" claim: in pass A the
single most common `assistant` key-set had 16 keys and occurred 27,638 times;
the second-most-common differed by one key (`slug`) and occurred 17,835 times;
a third variant swapped `agentId,attributionAgent` for `session_id` (11,894).
All were the same version family. Pass B's 23 extra names, found the same day
on a smaller corpus, make the point harder still. **Unknown/missing fields are
the norm, not an anomaly. A `deny_unknown_fields` here would have failed on
23 names in under three hours.**

### 2.3 Content shape — an array of blocks, almost always

| Kind | Shape | Lines |
|---|---|---|
| `assistant` | **always** a JSON array of blocks | 85,031 / 85,031 |
| `user` | array of blocks | 47,062 |
| `user` | **plain string** | 1,977 |

A reader that assumes `content` is a string loses 98% of the corpus; one that
assumes an array drops the 1,977 lines that are almost entirely *the human's
own typed prompts*, which is precisely the testimony recount exists to find.
Handle both.

Block types observed, by role:

| Role | `block.type` | Count |
|---|---|---:|
| user | `tool_result` | 46,846 |
| assistant | `tool_use` | 46,833 |
| assistant | `thinking` | 24,851 |
| assistant | `text` | 13,347 |
| user | `text` | 219 |
| user | `image` | 25 |
| assistant | `fallback` | 4 |

- `text` block: `{"type":"text","text":"…"}`.
- `thinking` block: `{"type":"thinking","thinking":"…","signature":"…"}` — the
  reasoning. For an audit tool whose whole purpose is "WHY", **`thinking` is
  the highest-value content in the file and is 65% more common than `text`.**
  Extract it. (Note it also means `recount grep` will surface reasoning the
  user may not expect to be searchable; that is a feature here, but say so in
  the README.)
- `tool_use` block: `{"type":"tool_use","id":"toolu_01…","name":"Bash","input":{…}}`.
  `input` is tool-specific and arbitrary.
- `tool_result` block: `{"type":"tool_result","tool_use_id":"toolu_01…","is_error":bool,"content":…}`
  where `content` is a **string** 46,004 times, and an **array** 854 times
  (of `text`, or `tool_reference`, or `image`, or mixed). `fallback` (4
  occurrences) is undocumented and unknown — ignore it, do not crash.
- The `user` line carrying a `tool_result` also carries a top-level
  `toolUseResult` object with the raw shape, e.g.
  `{"stdout":…,"stderr":…,"interrupted":…,"isImage":…,"noOutputExpected":…}`
  for Bash. Redundant with the block; shape varies by tool. Ignore it.

`attachment` lines have no `message`; they carry `.attachment.type`, of which
23 distinct values occur — top: `hook_success` (2,188), `task_reminder`
(1,911), `total_tokens_reminder` (1,493), `deferred_tools_delta` (1,080),
`skill_listing` (846). These are injected context, not human or model speech.
Skip them for `grep`/`at` output; they will otherwise flood every result with
boilerplate.

`system` lines carry `.subtype`: `turn_duration` (903), `stop_hook_summary`
(815), `away_summary` (320), `local_command` (41), `scheduled_task_fire` (14),
`compact_boundary` (12), `model_refusal_fallback` (2), `informational` (1).
`compact_boundary` matters: it has `parentUuid: null` plus
`logicalParentUuid`, and `compactMetadata` with `preTokens`/`postTokens`. It
**breaks the uuid chain mid-file**.

> **CORRECTION (2026-08-19, from the `claude_code` connector bead,
> recount-hwk.29).** The §2 table above said a `system` line carries `content`
> as a string. **Most do not.** Re-counted over the whole corpus that day:
> 1,963 `system` lines, of which **345 carry a string `content` and 1,618 carry
> none at all**. The two commonest subtypes are the ones without it —
> `turn_duration` (828) carries `durationMs`/`messageCount`/
> `pendingBackgroundAgentCount`, and `stop_hook_summary` (790) carries
> `hookCount`/`hookInfos`/`hookErrors`/`stopReason`. Both are telemetry about a
> turn, not anything anyone said. The subtypes that *do* carry prose are
> `away_summary` (276), `local_command` (41), `scheduled_task_fire` (14),
> `compact_boundary` (12), `model_refusal_fallback` (1) and `informational`
> (1). (The per-subtype totals differ slightly from the line above because the
> corpus is live and append-only — §0 — and was re-walked a few hours later.)
>
> Consequence, implemented in `src/connector/claude_code.rs`: a `system` line
> is read for its `cwd` and then becomes a turn **only if it has a `content`
> string**. Emitting the other 82% would put 1,618 empty-bodied turns into
> every `sessions`/`grep` result, which is the same flooding §2.3 rejects
> `attachment` lines for.

### 2.4 `journal.jsonl` is not a transcript

The 29 `…/subagents/workflows/wf_*/journal.jsonl` files hold only `started`
(371) and `result` (328) lines. They have no `cwd`, no `timestamp`, no
`message`, no `uuid`. **They are exactly the 29 files in the corpus with zero
`cwd` lines.** The `result` payload does carry a useful prose `summary` of what
a workflow subagent did, but with no time and no cwd it cannot participate in
the join. Recommendation: the connector's `discover()` should skip any file
named `journal.jsonl` outright; otherwise `doctor` will report 29 "sessions
with no cwd" forever.

---

## 3. Session identity

### 3.1 One file is one session — for top-level files, verified

- Top-level file name is `<sessionId>.jsonl` where `<sessionId>` is a UUIDv4.
  **All 238 top-level files have a single `sessionId` value equal to the
  filename stem.** Zero mismatches. You may trust the filename.
- **Subagent files do not follow this.** All 604 `agent-<hex>.jsonl` files
  carry the **parent session's** `sessionId`, not their own identity. Their own
  identity is `agentId`, and **`agentId` equals the filename hex in 604 of 604
  files** (`agent-a1a724549638348da.jsonl` → `"agentId":"a1a724549638348da"`).
  604 distinct `agentId` values corpus-wide, exactly matching the file count.
- `sessionId` is therefore **not unique across files**. Keying a map on
  `sessionId` silently merges a parent session with all its subagents. Key on
  the file path.

### 3.2 Sidechains

`isSidechain` appears only on `assistant`, `user`, `attachment`. It is `true`
on 85,516 lines — and **zero of those are in a top-level file**. `isSidechain:
true` ⟺ the line lives in `…/subagents/…`. The two facts are redundant on this
machine; the cheap test (path) and the expensive test (field) agree.

### 3.3 `parentUuid` chains

`parentUuid` is a uuid or `null`; `uuid` identifies the line. The chain is
**not a single list per file**: the 21,211-line pact session
`416610a6-….jsonl` has **9 roots** (`parentUuid == null`), the 4,630-line
`0bb1638c-….jsonl` has 2, and eight other pact sessions have exactly 1. Roots
appear at compaction boundaries and resumes. A reader must not assume one root
or a fully-connected chain. Recount does not need the chain at all — file order
is sufficient — so **don't build it**.

### 3.4 Fleet-identity fields (the join's best friends)

> **The heading oversells two of them.** `attributionAgent` and
> `attributionSkill` label the population without discriminating inside it,
> because a pact fleet is all `general-purpose`; `gitBranch` stays `master` for
> a `wt/<agent>` worktree. The connector reads neither, on purpose. The signals
> that actually separate siblings are `agentId`, the sidecar `description` and
> a transcript mention — §4.3 ranks them, §9.3 argues it out. *Added
> 2026-08-19 10:05.*

Present on `assistant`/`user` lines in subagent files:

- `attributionAgent` — the subagent type. 13 distinct: `workflow-subagent`
  (27,312), `general-purpose` (22,250), `docs-curator` (958), `Explore` (693),
  `audit-context-building:function-analyzer` (334), `cli-surface-auditor`,
  `cross-platform-auditor`, `zsh-config-reviewer`, …
- `attributionSkill` — 23 distinct, e.g. `antithesis-research` (1,576),
  `audit-context-building:audit-context` (384).
- `agentId` — see §3.1.
- `gitBranch` — 64 distinct. `master` (125,559), `worktree-antithesis-research`
  (4,886), `main`, `worktree-finops-kb-review`, `HEAD` (2,625, detached),
  `feat/redis-http-cache`, …
- `slug` — 5 distinct, and **two of them are worktree names that also appear as
  project-directory suffixes** (`valiant-whistling-pebble`,
  `sunny-wondering-matsumoto`). The other three (`rosy-bubbling-hippo`,
  `squishy-mixing-yao`, `buzzing-marinating-boole`) are not. *Low confidence*
  on what `slug` means; 5 samples.
- `entrypoint` — `cli` (140,388) or `sdk-cli` (6,108). `sessionKind` — only
  `bg` (18,477), absent otherwise. `userType` — only `external`, 146,496/146,496.
- `effort` — `xhigh` (41,635), `high` (41,308), `medium` (234). Assistant only.

Each subagent `.jsonl` has a `<same-name>.meta.json` sidecar (604/604):
`{"agentType":"general-purpose","description":"W0 empirical format-notes bead","toolUseId":"toolu_01…","spawnDepth":1}`
(sometimes `+model`, once `+stoppedByUser`). **`description` is the spawning
task's one-line label** — the cheapest possible fleet-identity signal, and it
costs one small file read. Strongly recommend `discover()` read it.

---

## 4. `cwd`: present, but it does not mean what the join wants

### 4.1 Presence

`cwd` is present on **100%** of `assistant`, `user`, `attachment`, `system`
lines — 0 missing. It is absent on 100% of the other 15 kinds. 842 of 871 files
have at least one `cwd`; the 29 without are all `journal.jsonl` (§2.4).

### 4.2 It varies within a single file — 46 of 871 files

Real examples:

- `-home-chussenot-Documents-waybar-pwetty-box--claude-worktrees-sunny-wondering-matsumoto/353d008f-….jsonl`:
  **167 lines** say `/home/chussenot/Documents/waybar-pwetty-box`, **175 lines**
  say `/home/chussenot/Documents/waybar-pwetty-box/.claude/worktrees/sunny-wondering-matsumoto`.
  One file, two cwds, roughly half and half.
- `-home-chussenot/165211c6-….jsonl` (982 lines) visits **9 distinct cwds**
  including `/home/chussenot/.claude/jobs/165211c6/tmp` (307 lines) and
  `/home/chussenot/Documents/datadog-templator/.claude/worktrees/otel-flag`
  (90 lines) — a session filed under `-home-chussenot` that did most of its
  work in a worktree of a *different repo entirely*.

So: **a session's "the cwd" does not exist.** `recount sessions --repo` must
treat cwd as a *set* per file, and match if **any** cwd resolves into the repo
or its worktrees. `recount at` must match on the cwd of the lines *inside the
time window*, not a file-level summary.

### 4.3 The failure mode the join must survive — measured on recount itself

I am the subagent that wrote this document. My shell cwd is
`/home/chussenot/Documents/recount/wt/formatnotes`. My transcript is
`~/.claude/projects/-home-chussenot-Documents-recount/cd7c75eb-c39f-4be4-92ae-29efda5d7d69/subagents/agent-a34d3106441943c34.jsonl`
and **every `cwd` line in it says `/home/chussenot/Documents/recount`.** The
worktree appears nowhere in any `cwd`.

The reason: `cwd` is the Claude Code *process's* working directory. A pact
fleet agent enters its worktree by running `cd` inside `Bash` tool calls; the
process cwd never moves. Only Claude Code's own `/worktree` feature (which
writes `.claude/worktrees/<name>` and emits a `worktree-state` line) actually
relocates `cwd`.

**Consequence for the spec's join** (`invoked_from -> worktree path -> session
cwd`): for pact-style `wt/<agent>` worktrees — i.e. the exact fleet recount was
built to audit — **that chain resolves to nothing.** All fleet agents of one
run share one cwd (the repo root) and one project directory. The connector must
therefore surface, and the join must use:

1. `cwd` — coarse, correct at the repo level, useless at the worktree level for
   pact worktrees. Sufficient to select *candidate* sessions.
2. `worktree-state.worktreePath` / `worktreeName` / `worktreeBranch` and
   `relocated.relocatedCwd` — authoritative when present (420 + 387 lines), but
   only for Claude-Code-native worktrees.
3. `gitBranch` — moves with the worktree in the native case
   (`worktree-antithesis-research`), stays `master` in the pact case.
4. **The transcript-mention tiebreaker is not a tiebreaker here — it is the
   primary key.** The string `formatnotes` appears in my transcript (my
   assignment names `PACT_AGENT=formatnotes` and the worktree path). Grepping
   `agent-*.jsonl` for the pact agent name plus the bead id is the only signal
   that separates sibling fleet agents. The spec already lists it; it needs to
   be *weighted first*, not last, and `at` must be able to say "matched on
   mention + time window, cwd was repo-level only" as its evidence.
5. `<file>.meta.json.description` — for me, `"W0 empirical format-notes bead"`.
   Independent, cheap, and it discriminates siblings without opening the
   transcript.

---

## 5. Pathologies — measured, and mostly absent

Both passes. Pass A: 871 files, 169,775 lines. Pass B: 857 files, 163,633
lines, re-run with the shipped connector's own rules.

| Pathology | Pass A | Pass B | Notes |
|---|---:|---:|---|
| Lines that fail `serde_json` / `json.loads` | **0** | **0** | not one, in 333,408 lines across two passes |
| Files whose **last** line is unparseable (truncated tail) | **0** | **0** | |
| Zero-byte files | **0** | **0** | |
| Files with no `cwd` at all | 29 | 29 | all `journal.jsonl` (§2.4) |
| Non-object JSON lines | 0 | 0 | every line is a JSON object |
| Lines missing `type` | 0 | 0 | |
| Lines over 1 MB | 31 | 31 | |
| **Lines over the connector's 8 MB cap** | 12 | **12** | §5.1 — this is every parse failure `doctor` reports |
| Largest single line | 17,185,957 B (16.4 MB) | 17,185,957 B | lines 1 and 5 of **six** sibling files, not two (pass A undercounted) |

Line length distribution, pass B, in real byte ranges — pass A gave these
buckets by decimal digit count and then labelled them one power of ten low, so
its "12 ≥1 MB" meant 12 ≥10 MB and its "31 over 1 MB" was the correct figure:

| Line length | Lines |
|---|---:|
| 10–99 B | 3,782 |
| 100–999 B | 36,094 |
| 1–9.99 KB | 115,247 |
| 10–99.9 KB | 8,362 |
| 100–999 KB | 117 |
| 1–9.99 MB | 19 |
| **≥10 MB** | **12** |

**Truncated tails were still not observed on the second pass. That is not
evidence they cannot happen** — the corpus contains no session killed
mid-write, and a live file was being appended to during both scans. Implement
the tolerance; just do not expect the fixture to occur naturally.

### 5.1 The 12 parse failures `doctor` reports are one bug in one prompt, on one day

`recount --json doctor`, 2026-08-19 10:05, says:

> `connector claude_code: 12 unparseable lines across 6 of 828 sessions — those
> transcripts are damaged, not merely skipped`

Pass A said zero bad lines and pass A was right. **All 12 are lines the
connector refused to read, not lines that failed to parse.** A Python walk
applying the connector's own rules — 8 MB cap, `json.loads`, object check,
`timestamp` check on `assistant`/`user`/`system` — reproduces `doctor` exactly:
12 failures, 6 files, 828 sessions, and every one of the 12 is the **oversized**
branch. Zero JSON errors, zero non-objects, zero unparseable timestamps.

The six files, all in `~/.claude/projects/-home-chussenot/`, all 14 lines long,
all created within 6m27s of each other on 2026-08-17:

| Session | First line at | Line 1 | Line 5 |
|---|---|---:|---:|
| `8a22db12-…` | 07:05:20.885Z | 17,181,270 B | 17,181,584 B |
| `1eaed69c-…` | 07:06:03.545Z | 17,181,270 B | 17,181,584 B |
| `9a5ce2b6-…` | 07:06:46.092Z | 17,181,270 B | 17,181,584 B |
| `1c27deb2-…` | 07:10:21.007Z | 17,185,643 B | 17,185,957 B |
| `c80327ba-…` | 07:11:03.746Z | 17,185,643 B | 17,185,957 B |
| `1b4c58fa-…` | 07:11:47.030Z | 17,185,643 B | 17,185,957 B |

Two big lines per file, and they are the *same* prompt twice: line 1 is a
`queue-operation` whose `content` holds it, line 5 is the `user` line that
`queue-operation` became, holding it again as a **bare-string
`message.content`** (§2.3).

What the prompt is: a small, ordinary instruction — "You are turning a
machine-generated recap of my Claude Code sessions into a report…" — piped a
digest on stdin, which **re-quoted itself**. The 9,610,205-character string is
**78.6% backslashes**; the longest unbroken backslash run is **8,191 = 2¹³−1**,
i.e. thirteen rounds of escape doubling, and the original instruction text
appears **142 times** nested inside itself. A shell/JSON quoting layer fed its
own output back in.

Claude Code wrote it down faithfully. The API refused it: each of the six files
ends with an `assistant` line carrying `isApiErrorMessage: true`,
`error: "invalid_request"`, `apiErrorStatus: 400`, `message.model:
"<synthetic>"` and the text

> `Prompt is too long · the request is ~8391167 tokens (limit 1000000)…`

Six sessions in six and a half minutes is a human (or a wrapper script)
retrying.

**Three things follow, and the third is a defect.**

1. **The 8 MB cap is right and the notes' old justification for it was wrong.**
   Pass A wrote "a 16 MB line is a pasted binary dump, never testimony". It is
   not a binary dump. It is **the human's own prompt** — the bare-string user
   content §2.3 calls "precisely the testimony recount exists to find" — and it
   is the single most audit-worthy line in the corpus, because it is a
   reproducible failure with the error message attached. Still skip it: 9.6 MB
   of backslashes helps nobody and holding it costs 16 MB. But skip it knowing
   what is being dropped. *If the cap ever needs a better answer than "drop",
   the answer is a truncated excerpt with a marker, not a bigger buffer.*
2. **`parse_failures` is a lossy channel and this is the proof.** §7.4 already
   records that a truncated tail and real damage are indistinguishable through
   one `usize`. Add a third member: **a line the reader declined to read.**
   Nothing in the corpus is damaged; the number 12 is a fact about recount's
   configuration, not about `~/.claude`.
3. **`doctor`'s wording is therefore wrong, and it is the user-visible half.**
   "12 unparseable lines … those transcripts are damaged, not merely skipped"
   is false on all three counts: the lines parse, the transcripts are
   byte-perfect JSON, and skipping is exactly what happened. A user shown that
   finding will go looking for corruption that does not exist. The honest
   finding is *"12 lines over the 8 MB read cap in 6 of 828 sessions — recount
   declined to read them; the files themselves are intact"*, which needs
   `SessionMeta` to separate "declined" from "damaged" (§7.4) and is a
   `doctor`/`model` change, not a notes change. **Recorded here rather than
   worked around.**

---

## 6. Time

### 6.1 Format and ordering

- **One format, corpus-wide**: `YYYY-MM-DDTHH:MM:SS.sssZ` — RFC 3339, UTC,
  literal `Z`, millisecond precision. All 149,297 timestamped lines match; zero
  other formats, zero non-`Z` offsets. Parse with `chrono::DateTime<Utc>`;
  string comparison also happens to sort correctly, but don't rely on that.
- 149,297 of 169,775 lines (87.9%) carry a timestamp. The 20,478 without are
  exactly the 12 metadata kinds in §2.

**Timestamps are NOT monotonic within a file.** 246 of 842 files contain at
least one inversion; 1,584 inversions total. Median inversion **0.16 s** (write
interleaving, harmless), but 47 exceed 60 s and 3 exceed an hour — worst is
**43,261 s (12 hours)** in `-home-chussenot-Documents-quern/88480c5c-….jsonl`,
and 32,473 s in the terrasync session. Cause is resume: a session picked up the
next day appends lines that follow older ones physically but not
chronologically.

Consequences for `recount at --ts … --window 5m`:
- **Do not binary-search a file by timestamp.** Scan it.
- **Do not stop early** on "we're past the window" — a later line can be
  earlier. Read the whole file (after mtime pruning) or accept misses.
- The first and last timestamps in a file are not min and max. If `sessions`
  reports a time range, compute it as an actual min/max, and be aware a range
  can be 12 h wider than the visible work.

### 6.2 mtime versus the last turn — the pruning question

Measured over 842 files: `mtime − last_timestamp` has **min 0.0 s, median
0.1 s, p99 106,800 s (~30 h), max 1,963,022 s (22.72 days)**.

**Zero files have an mtime earlier than their last timestamp.** Not one, in
842. So:

> **mtime is a safe upper bound on the last turn.** Pruning "skip files with
> `mtime < since`" is sound on this corpus: it can never discard a file whose
> last turn is at or after `since`.

*Confidence: high for the direction, moderate as a guarantee.* It is an
arithmetic near-certainty (the file is written when the line is written) but a
clock step or an NTP correction could invert it, and no such event is in this
corpus. Keep the prune; do not assert it in a way that panics.

The converse is badly false: mtime can run **weeks** past the last real turn.
`-home-chussenot--dotfiles/86767ddd-….jsonl` (80 lines) has an mtime 22.72 days
after its last timestamp, and its final six lines are `last-prompt`,
`ai-title`, `agent-name`, `agent-setting`, `mode`, `permission-mode` — all
timestamp-less metadata, rewritten when the session was resumed or renamed in
the UI. So mtime pruning is **conservative, not precise**: it will admit files
with nothing in the window. Never report mtime as "last activity" to the user.

---

## 7. What a tolerant reader must do — concretely, against this format

The spec says "unknown fields ignored, truncated tails counted not fatal, one
bad line never fails a file". Against the format above that means:

1. **Deserialize into a struct with `#[serde(default)]` on everything except
   `type`, and no `deny_unknown_fields`, ever.** The corpus has ≥25 top-level
   field names, 22 producer versions, and key-sets that differ by version
   within the same session kind. Better still: `Value`-first, pull the six
   fields you need, drop the rest. Both `sessionId` and `session_id` occur;
   read `sessionId` and fall back.
2. **Match `type` as a string with a catch-all arm.** Nineteen kinds today; two
   of them (`atis-latch`, `frame-link`) have 1 and 5 occurrences and are
   plainly new. An unknown `type` is a *skip*, tallied in `doctor`, never an
   error. Likewise `block.type` (`fallback` already exists and is undocumented).
3. **Per-line error containment.** `for line in reader.lines()`: a JSON error
   increments `parse_failures` for that file and `continue`s. A file yields
   `(Vec<Turn>, ParseStats)`, never `Result<Vec<Turn>>`. Zero bad lines were
   observed, so this path will be exercised only by fixtures — write the
   fixtures.
4. **Truncated tail = the last line failing to parse.** Detect it specifically
   (last line, parse error) and report it as `truncated_tail: true` rather than
   as a generic bad line, because it is the benign case: the session was killed
   mid-write and the preceding 4,629 lines are perfect.
   *Not implemented, and the reason is recorded rather than worked around
   (recount-hwk.29):* `SessionMeta` (`src/model.rs`, another bead) has one
   `parse_failures: usize` and no `truncated_tail` field, so the connector
   cannot report the distinction without changing a type it does not own. A
   truncated tail is therefore exactly **one parse failure, on the last line**.
   The spec only asks that truncated tails be "counted not fatal", which that
   satisfies; the benign/damaged distinction is a `SessionMeta` change if
   anyone ever wants it.
   *And there is a third case, measured in §5.1 and not hypothetical:* a line
   **over the 8 MB cap** is also one `parse_failure`. On this machine that is
   **all 12 of them** — every failure `doctor` reports is a line recount
   declined to read, in a file with nothing wrong with it. So one `usize`
   currently conflates damage, a benign truncated tail, and recount's own
   configuration, and `doctor` reports the union as "damaged". If any of the
   three is ever worth acting on differently, this is the field that has to
   grow.
5. **Cap line length.** 8 MB, skip-and-count above it. **31** real lines
   exceed 1 MB; **12** exceed 10 MB and all 12 are over the cap; the largest is
   16.4 MB. Those 12 are not binary dumps — they are one runaway prompt,
   retried six times (§5.1) — so count them somewhere a human can tell "we
   didn't read this" from "this is broken".
6. **`content` is `String | Vec<Block>`.** An untagged enum, or match on
   `Value::String` vs `Value::Array`. Same again one level down for
   `tool_result.content`.
7. **cwd is a set, not a value** (§4.2), and `Option`, not a value (absent on
   15 kinds).
8. **Never assume timestamps ascend** (§6.1). Never assume one root in the
   uuid chain (§3.3). Never assume `sessionId` is unique across files (§3.1).
9. **Skip `journal.jsonl` at discovery**, and skip `attachment` lines when
   emitting testimony (§2.3) — 10,309 lines of injected boilerplate that will
   otherwise dominate every `grep` hit.
10. **Read files as bytes, lossily.** All content here decoded as UTF-8, but a
    tool result can contain arbitrary program output; `from_utf8_lossy` on the
    slice you print, never `expect`.
11. **Tolerate growth.** The file may be longer than the `stat` you took
    (observed, §0). Do not pre-allocate from `st_size` and assert on a short
    read; do not fail if the last line has no trailing newline yet.

---

## 8. What I could NOT determine

Flagged so the `claude_code` connector bead knows where it is guessing.
**Updated 2026-08-19 10:05** after the connector and `doctor` shipped: two
entries moved to §8.1 because they are now determined, and three were added
that only building the thing could reveal.

- **How non-ASCII path characters encode.** Every path on this machine is
  ASCII. Whether `é` becomes one `-` or two (per byte) is unknown. *Guess:*
  per-`char`, but untested. If `--repo` is ever given a non-ASCII path, the
  encoding may be wrong and the connector should say so rather than silently
  find nothing.
- **Whether a trailing `/` on a path produces a trailing `-`.** No example.
  Canonicalise (strip trailing separators) before encoding.
- **Whether the rule is `[A-Za-z0-9]`-keep or something narrower/wider.** Proven
  kept: `A-Z`, `a-z`, `0-9`. Proven replaced: `/ . _ + -`. Unproven: `~ @ % ' ( )
  space` and every other punctuation character — none occur in a path here.
- **Whether directory-name collisions are handled at all** (suffix? overwrite?
  shared directory?). No collision exists on this machine to observe. Assume
  shared directory; assume `cwd` inside is the only disambiguator.
- **Whether `summary` lines exist in other versions.** Absent here.
- **Whether truncated tails / non-JSON lines / empty files ever occur.** Zero
  observed. The tolerance is required by spec and by prudence, not by evidence.
- **Whether mtime can precede the last timestamp.** Zero of 842. Argued, not
  proven (§6.2).
- **Semantics of `slug` (5 samples), `effort`, `atis-latch` (1 line),
  `frame-link` (5 lines), `sessionKind: "bg"`, `promptId` vs `requestId`.**
  Read them, store them if free, key nothing on them.
- **Whether `.meta.json` is always written** — 604/604 here, but a subagent
  killed before its sidecar is flushed is plausible. Treat as optional.
- **`~/.claude` outside `projects/`** (`history.jsonl`, `shell-snapshots/`,
  `todos/`, per-project `memory/`) — out of scope for this bead and not examined.

New, from building the connector and `doctor` (waves 2–3):

- **Why the corpus shrinks** (§0.0). 22 top-level sessions and seven project
  directories vanished between two passes 2h45m apart. The surviving mtimes
  bunch at the 30-day mark, which points at Claude Code's own
  `cleanupPeriodDays` retention, but no setting is written in
  `~/.claude/settings.json` on this machine and I did not watch a deletion
  happen. **Assume sessions expire; do not assume when.**
- **Whether the 23 top-level field names pass B found (§2.2) are new to the
  format or merely unsampled by pass A.** Both passes ran the same day on
  overlapping corpora, which argues "unsampled" — pass A read a rarer slice, not
  an older format. Either way the lesson is the same and it is already in §7.1.
- **Whether any line over the 8 MB cap has ever been worth reading.** Twelve
  exist and all twelve are one runaway prompt (§5.1). One specimen is not a
  distribution. If a second kind ever shows up, the cap needs an excerpt, not a
  higher number.
- **What `parse_failures > 0` should mean to a human.** Today it fuses damage,
  a benign truncated tail, and "recount declined to read this" into one
  `usize`, and on this machine it is 100% the third (§5.1, §7.4). Unresolved by
  design, recorded so nobody re-derives it.

### 8.1 Determined since pass A — removed from the list above

- **~~Which field points at `tool-results/*.txt`~~ — determined 2026-08-19.**
  **No field does.** The path is prose inside the `tool_result` block's own
  text, and duplicated in the top-level `toolUseResult` string:
  `"Error: result (50,131 characters across 6 lines) exceeds maximum allowed
  tokens. Output has been saved to /home/chussenot/.claude/projects/<encoded>/
  <session>/tool-results/<id>.txt."` Two id styles, 91 files at
  pass B and 87 at pass A: 79 opaque nine-character ones (`bw1n34x04.txt`) and
  12 self-describing MCP ones, `mcp-<server>-<tool>-<epoch-millis>.txt`, e.g.
  `mcp-datadog-search_datadog_metrics-1784535745222.txt`. So a reader *can* find them by regex on the tool_result text,
  and `recount grep` will match that sentence but not the file's contents. The
  README should still scope the file contents out explicitly — but "we don't
  know how to find them" is no longer the reason.
- **~~Whether a `system` line always carries a string `content`~~ — determined
  2026-08-19, and it does not.** See the correction in §2.3: 1,618 of 1,963
  carry none. Folded into the table there.

---

## 9. Field ledger: what the connector reads, and what it deliberately does not

Added 2026-08-19 10:05, cross-checked line by line against
`src/connector/claude_code.rs` in **both** directions. The rule this table
enforces: *every field the connector reads is documented above, and every field
documented above is either read or listed here with the reason it is not.*
Result of the check: **no field is read that this document does not describe.**
The gaps all run the other way, and two of them are worth an argument.

### 9.1 Read

| Field | Where it is read | Becomes | §|
|---|---|---|---|
| `type` (top level) | kind switch; anything but `assistant`/`user`/`attachment`/`system` is skipped **without** being tallied | the line's fate | §2 |
| `cwd` | tallied per distinct value, most-frequent-first | `SessionMeta.cwd` + `Identity.cwds` | §4.1, §4.2 |
| `timestamp` | on a line that would become a turn; missing or unparseable is a counted failure | `Turn.ts` | §2.2, §6.1 |
| `sessionId`, then `session_id` | first non-empty on any line | `Identity.session_id` | §3.1, §7.1 |
| `agentId` | first non-empty on any line | `Identity.agent_id` | §3.1, §3.4 |
| `message.content` | `assistant`, `user` | `Turn.text` | §2.2, §2.3 |
| `content` (top level, string) | `system` only; **absent ⇒ no turn at all** | `Turn.text` | §2.3 |
| `subtype` | `system` only | the `[system:<subtype>]` tag | §2.3 |
| block `.type` | every block | which renderer | §2.3 |
| block `.text` / `.thinking` | `text`, `thinking` | verbatim body | §2.3 |
| block `.name` / `.input` | `tool_use` | `[tool_use <name>]` + compact JSON | §2.3 |
| block `.is_error` / `.content` | `tool_result`, recursing one level | `[tool_result]` / `[tool_result error]` | §2.3 |
| sidecar `agentType`, `description`, `toolUseId`, `spawnDepth` | `agent-<hex>.meta.json` | `Identity.sidecar` | §3.4 |

### 9.2 Not read, on purpose

| Field | Why not |
|---|---|
| `message.role` | §2.2: agrees with `.type` on all 130,955 lines, and `type` is on 100% of lines where `message` is on 80%. Redundant. |
| `uuid`, `parentUuid`, `logicalParentUuid` | §3.3: the chain has multiple roots and recount does not need it — file order is sufficient. **Don't build it.** |
| `isSidechain` | §3.2: exactly equivalent to "the path contains `/subagents/`", which is free. |
| `attachment.*` | §2.3, §7.9: read for `cwd`, then skipped. Its 23 subtypes only sub-classify boilerplate. |
| `toolUseResult` | §2.3: redundant with the `tool_result` block, shape varies per tool. |
| `queue-operation.content` | Queued user text with a `timestamp` but **no `cwd`**, so it can never take part in a topological join. (It is also where half the oversized lines live — §5.1.) |
| `compactMetadata`, `preTokens`, `postTokens` | Compaction telemetry, nobody's testimony. |
| `aiTitle`, `agentName`, `agentSetting`, `mode`, `permissionMode`, `atis`, `frame-link`, `file-history-*`, `last-prompt` | §2.1: whole kinds skipped. No `cwd`, no `timestamp`, not in the uuid chain. |
| `thinking.signature`, `tool_use.id`, `tool_result.tool_use_id` | Identifiers for pairing blocks; recount renders turns and never pairs them. |
| `slug` | §3.4, §8: five samples, meaning unknown. Keying anything on it would be a guess. |
| `effort`, `version`, `entrypoint`, `userType`, `sessionKind`, `promptId`, `requestId`, `sourceToolAssistantUUID`, and all 23 names pass B added in §2.2 | §8: "read them, store them if free, key nothing on them". Nothing is free here — `SessionMeta` and `Identity` have no field for them and no caller wants one. Adding a field per name would be inventing a schema for a format that changes between point releases. |
| sidecar `model`, `stoppedByUser` | Observed (§3.4) but absent from the `Sidecar` struct. Serde ignores them, which is the correct outcome; add them the day something reads them. |

### 9.3 Not read, and it is arguable — two entries

These two are documented above in flattering terms and the connector reads
neither. The code is defensible; the *notes* were overstated, and this is
written down rather than quietly fixed in one direction.

**`attributionAgent` (52,345 lines) and `attributionSkill` (2,684).** §3.4
titles them "the join's best friends". They are not. For the fleet recount was
built to audit, every agent is spawned `general-purpose`, so `attributionAgent`
labels the population without discriminating inside it — which is precisely the
question the join asks. §4.3 already gets this right and ranks the signals that
work: `agentId`, the sidecar `description`, and a transcript mention. Those are
exactly the three the connector reads. **Read §3.4's heading as "the fields you
would reach for first, and be wrong".** They remain a good *label* for output —
"this was an `Explore` subagent" — and if `sessions` ever prints one, this is
where it lives.

**`gitBranch` (100% of lines), `worktree-state.worktreePath` / `worktreeName` /
`worktreeBranch` (212 lines), `relocated.relocatedCwd` (205).** §4.3 signals 2
and 3, and it calls them *authoritative when present*. They are — for Claude
Code's own `/worktree` feature, which relocates the process `cwd` and writes
these records. They never appear for a pact `wt/<agent>` worktree, where the
agent `cd`s inside `Bash` and the process cwd never moves, so the connector
surfaces none of them and says so in its module doc. **This is a real
capability gap, not a nothing:** a fleet that used Claude Code's native
worktrees would be joinable exactly and precisely, and recount would miss it.
The 417 lines are here whenever that is worth the field on `Identity`.
