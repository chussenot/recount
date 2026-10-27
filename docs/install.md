# Installing recount

There is no package in any registry. Each GitHub release carries prebuilt
binaries for three targets — see [Prebuilt binaries](#prebuilt-binaries) —
and everywhere else recount builds from source. It is a single Rust binary with
no runtime state, no config file and no network access, so building it is the
whole installation.

## From source

```bash
git clone <this repo> && cd recount
cargo build            # ./target/debug/recount
cargo install --path . # ~/.cargo/bin/recount
```

`cargo build --release` if you are measuring anything: on the same corpus and
the same machine the bench's grep read 35.9 s in debug against 2.83 s in
release — 13x, a fact about the optimiser and not about recount. Release has
been faster since (~1.0-1.2 s); the ratio is the point. See [performance.md](performance.md).

Check what you got:

```bash
recount --version   # recount 0.2.0
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

## Prebuilt binaries

Every version tag — bare `X.Y.Z`, no `v` prefix, the first being `0.1.0` —
publishes a tarball per target, `recount-<tag>-<triple>.tar.gz`, holding the
stripped binary, `README.md` and `CHANGELOG.md`, plus a `SHA256SUMS` file:

| Target | Runner it was built and smoke-tested on |
|---|---|
| `x86_64-unknown-linux-musl` | `ubuntu-latest` |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
| `aarch64-apple-darwin` | `macos-latest` |

musl means static, so the Linux binaries run on any distribution regardless of
glibc version, and inside `FROM scratch` images.

Verify what you downloaded. The checksums file covers every tarball:

```
sha256sum -c SHA256SUMS
```

Releases also carry a build provenance attestation, so you can confirm which
workflow, commit and run produced a tarball rather than trusting the filename:

```
gh attestation verify recount-<tag>-<triple>.tar.gz --repo <owner>/recount
```

**Intel macOS and Windows have no binary.** Intel macOS because GitHub retired
the runner image and cross-compiling would ship a binary nobody executed;
Windows for the reason below. Both build from source.

## What is not promised

- **No MSRV.** Neither CI nor the release pins a toolchain: CI uses whatever
  stable Rust the `ubuntu-latest` image ships, and the release installs
  whatever stable is current when it runs. Pinning one would be making a promise
  nobody made.
- **No Windows.** Not an omission. recount resolves
  `~/.claude/projects/<encoded-path>` from POSIX absolute paths, shells out to
  `git worktree list --porcelain` and compares the results as unix paths, and
  every test fixture builds a POSIX tree. A Windows binary would compile and
  then be wrong in ways no test here would catch.
- **The full test suite runs on Linux only.** The release builds each target on
  its own native runner and smoke-tests the binary it just produced — version,
  the presence of `sessions`, `at`, `grep`, `explain`, `doctor` and
  `completion` in `--help` (a list written into `release.yml`, which `testify`,
  added in 0.2.0, is not on), `doctor` in an empty directory, and completion
  generation — but `cargo test` itself runs in one CI job on `ubuntu-latest`.
  A macOS binary is therefore known to *run*, not known to *pass the suite*.

These are stated rather than quietly implied, because a reader who assumes
otherwise finds out at the wrong moment.

## See also

- [cli.md](cli.md) — every command, flag, exit code and JSON shape
- [development.md](development.md) — building, testing, and why each gate exists
