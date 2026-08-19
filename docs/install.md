# Installing recount

There is no release binary and no package. recount is a single Rust binary with
no runtime state, no config file and no network access, so building it from
source is the whole installation.

## From source

```bash
git clone <this repo> && cd recount
cargo build            # ./target/debug/recount
cargo install --path . # ~/.cargo/bin/recount
```

`cargo build --release` if you are measuring anything: the same grep is 35.9 s
in debug against ~1.06 s in release on the same corpus, which is a fact about
the optimiser and not about recount. See [performance.md](performance.md).

Check what you got:

```bash
recount --version   # recount 0.1.0
recount doctor      # can a join even run here?
```

`recount doctor` is the right first command on a new machine. It always exits
0, so it is safe in a provisioning script, and it answers the only question
that matters before anything else: whether the connector roots exist, whether
there is a ledger, and whether the worktrees are alive. The machine-readable
verdict is `.join_readiness.ready` under `--json`. See
[cli.md](cli.md#recount-doctor).

## With mise

`mise tasks` lists them all. The two that matter for installing:

| task | what it does |
|---|---|
| `mise run build` | `cargo build` |
| `mise run install` | `cargo install --path . --force`, then refreshes shell completions |

`mise run install` regenerates the completion scripts after installing, into
whichever of these directories **already exists** — it creates none, so it is a
no-op on a machine without them:

| shell | directory | file |
|---|---|---|
| zsh | `${ZDOTDIR:-$HOME}/.zsh/completions` | `_recount` |
| bash | `$HOME/.local/share/bash-completion/completions` | `recount` |
| fish | `$HOME/.config/fish/completions` | `recount.fish` |

The refresh is part of install rather than a separate step because a shell
caches the completion file. Completions are generated from the same clap
command tree the parser uses, so they cannot name a command that does not
exist — but an install that skipped the refresh would leave yesterday's
commands completing until somebody regenerated it by hand, and nobody does.

## Completions by hand

```bash
recount completion zsh > "${ZDOTDIR:-$HOME}"/.zsh/completions/_recount
recount completion bash > "$HOME"/.local/share/bash-completion/completions/recount
recount completion fish > "$HOME"/.config/fish/completions/recount.fish
```

`elvish` and `powershell` are also accepted. The script goes to stdout alone —
this is the one command that does not print a human digest beside its output,
because the output is a shell program and a digest would be sourced as code.
See [cli.md](cli.md#the---json-contract).

## What it depends on

The spec sets a dependency budget: **serde/serde_json, clap, chrono, anyhow**.
Anything beyond it needs a justifying comment in `Cargo.toml`, at the
dependency. Two exist today:

- `clap_complete` — generating five shells' completion scripts from the parser's
  own command tree. It is clap's companion crate, pulls in nothing beyond clap,
  and the alternative is several hundred lines of hand-written shell that goes
  stale on the next flag.
- `tempfile` — dev-dependency only, for the fixture trees the degradation tests
  build.

There are no cargo features. The spec refuses a TUI, a daemon, network,
embeddings and persistence, so there is nothing to gate, and there is exactly
one build.

## What is not promised

- **No MSRV.** CI does not pin a toolchain; it uses whatever stable Rust the
  `ubuntu-latest` image ships. Pinning one would be making a promise nobody
  made.
- **No cross-platform support.** CI is one job on one platform, deliberately: a
  matrix is three times the minutes for a promise that was never made. The
  `claude_code` connector reads `~/.claude/projects`, and nothing in recount has
  been tested anywhere but Linux.

Both are stated rather than quietly implied, because a reader who assumes
otherwise finds out at the wrong moment.

## See also

- [cli.md](cli.md) — every command, flag, exit code and JSON shape
- [development.md](development.md) — building, testing, and why each gate exists
