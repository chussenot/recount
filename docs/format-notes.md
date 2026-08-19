# format-notes: the real `~/.claude` layout, observed

**Observed: 2026-08-19.** Machine: this one — Linux 6.17 (Ubuntu OEM kernel),
`/home/chussenot`, running the `recount` repo itself. Claude Code versions seen
in the corpus: 22 distinct, `2.1.215` … `2.1.234`, dominated by `2.1.220`
(38,797 lines) and `2.1.233` (26,058).

> **REVERSE-ENGINEERED, EXPECT DRIFT.** Nothing here comes from a published
> schema. Every statement below was measured on one machine, on one day, over
> one corpus. Anthropic changes this format between point releases — the corpus
> already spans 22 versions and the field sets differ between them (see
> §3.1). Treat this document as a snapshot to be re-measured, never as a
> contract. The connector must degrade, not assert. In one phrase: this
> document is reverse-engineered, expect drift.

## 0. The corpus this was measured on

| Thing | Count |
|---|---|
| Project directories under `~/.claude/projects/` | 48 |
| …of which contain at least one `.jsonl` | 43 |
| `.jsonl` files (all depths) | 871 |
| …top-level session files (`<project>/<uuid>.jsonl`) | 238 |
| …subagent files (`<project>/<uuid>/subagents/agent-<hex>.jsonl`) | 604 |
| …workflow journals (`…/subagents/workflows/wf_<id>/journal.jsonl`) | 29 |
| `.meta.json` sidecars (one per subagent file) | 604 |
| Total JSONL lines | 169,775 |
| Total bytes | 754.7 MB (mean 887 KB/file, max 47.7 MB) |
| Timestamp range | `2026-07-02T12:33:20.334Z` … `2026-08-19T07:20:00.504Z` |

Reproduce: `/tmp/claude-1000/…/scratchpad/scan.py` and `probe.py`,
`probe2.py` (single-pass Python walks, ~7 s over the whole corpus). Every
number in this document comes from those passes.

Two independent passes minutes apart counted 149,297 then 149,351 timestamped
lines. The 54-line delta is not an error: the live session file
`-home-chussenot-Documents-recount/cd7c75eb-….jsonl` grew between the passes.
**The corpus is live and append-only while you read it.** A reader that stats a
file and then reads it may see more bytes than it stat'd.

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
      tool-results/bahd13vhz.txt                       # externalised tool output (87 files, up to 120 KB)
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
| `system` | 2,108 | 1.2% | always | always | no (`content` str) | hooks/compaction |
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

Full top-level field inventory seen on `assistant`/`user` lines:
`parentUuid`, `isSidechain`, `type`, `message`, `uuid`, `timestamp`, `cwd`,
`sessionId`, `session_id` (yes, both spellings on newer versions, same value),
`userType`, `entrypoint`, `version`, `gitBranch`, `requestId` (assistant),
`promptId` (user), `sourceToolAssistantUUID` (user), `toolUseResult` (user),
`effort` (assistant), `slug`, `sessionKind`, `agentId`, `attributionAgent`,
`attributionSkill`, `logicalParentUuid` (compaction).

Field-set drift within the corpus, for the "expect drift" claim: the single
most common `assistant` key-set has 16 keys and occurs 27,638 times; the
second-most-common differs by one key (`slug`) and occurs 17,835 times; a
third variant swaps `agentId,attributionAgent` for `session_id` (11,894). All
are the same version family. **Unknown/missing fields are the norm, not an
anomaly.**

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

Full scan of all 871 files, 169,775 lines:

| Pathology | Count | Notes |
|---|---:|---|
| Lines that fail `serde_json` / `json.loads` | **0** | |
| Files whose **last** line is unparseable (truncated tail) | **0** | |
| Zero-byte files | **0** | |
| Files with no `cwd` at all | 29 | all `journal.jsonl` (§2.4) |
| Non-object JSON lines | 0 | every line is a JSON object |
| Lines missing `type` | 0 | |
| **Lines over 1 MB** | **31** | |
| **Largest single line** | **17,185,957 bytes (16.4 MB)** | line 5 of `-home-chussenot/1c27deb2-….jsonl` (and lines 1 and 5 of two sibling files) |

Line length distribution (by digit count): 4,121 lines <10 B, 38,834 <100 B,
118,203 <1 KB, 8,496 <10 KB, 128 <100 KB, 19 <1 MB, **12 ≥1 MB**.

**Truncated tails were not observed. That is not evidence they cannot happen** —
the corpus contains no session killed mid-write, and one live file was being
appended to during the scan. Implement the tolerance; just do not expect the
fixture to occur naturally.

The 16 MB line is the real pathology. A reader using a fixed-size buffer, or
`BufRead::lines()` without a cap, on `-home-chussenot/*.jsonl` will either
truncate or allocate 16 MB per line. Recount should read with an explicit
max-line budget (say 8 MB), and **count-and-skip** oversized lines rather than
buffering them — a 16 MB line is a pasted binary dump, never testimony.

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
5. **Cap line length.** 8 MB, skip-and-count above it. 12 real lines exceed
   1 MB and one is 16.4 MB.
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

Flagged so the `claude_code` connector bead knows where it is guessing:

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
- **`~/.claude/projects/*/…/tool-results/*.txt`** — 87 externalised tool
  outputs, referenced from transcripts by an opaque id (`bahd13vhz`). I did not
  determine which field points at them. If `grep` is expected to find text that
  only lives in those files, that is unimplemented and unmeasured. *Recommend
  scoping it out explicitly in the README.*
- **`~/.claude` outside `projects/`** (`history.jsonl`, `shell-snapshots/`,
  `todos/`, per-project `memory/`) — out of scope for this bead and not examined.
