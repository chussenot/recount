#!/bin/bash
# SessionStart hook for a Claude Code cloud session: put the mbx build cache
# (mise.toml, "mr boxington") under every Cargo command the session runs, so a
# rebuild restores what an earlier one compiled instead of compiling it again.
# Runs only in the cloud (CLAUDE_CODE_REMOTE=true); a laptop keeps its own setup
# (docs/development.md, "The build cache").
#
# Idempotent: every step is a no-op when already done, and the container's
# state is snapshotted after the hook, so a later session pays for nothing that
# is already installed, configured or built.
#
#   1. mise, pinned, from its GitHub release (mise.run is not always reachable).
#   2. MISE_ENV=cloud, so mise.cloud.toml replaces what the sandbox cannot
#      resolve (mr-boxington's version list, rust's channel) with a pinned
#      release URL; Rust stays the image's rustup.
#   3. mbx's Cargo shim, first on PATH. mise.toml's [wrappers.cargo] covers
#      `mise run` and `mise exec`, but a session types plain `cargo test` —
#      CLAUDE.md tells it to — and a plain cargo that bypasses mbx gets no
#      cache and can fail on the read-only artifacts mbx restores into
#      `target`. The shim makes plain `cargo` mbx's.
#   4. A budget on the cache. mbx sizes its defaults from the disk it sees,
#      and a session sees the host's disk while being allowed a fixed slice of
#      it: the default free-space floor (10% of the disk) can sit above what
#      the session is allowed to use at all. Explicit numbers, set only when
#      the user has not set them.
#   5. A warm build of every target, so the session's first `cargo test` or
#      `mise run check` is incremental. RECOUNT_SESSION_SKIP_BUILD=1 skips it.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

# >= 2026.8.16 is what [wrappers.cargo] needs; this one is the version tested.
MISE_VERSION="2026.9.12"
root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$root"

log() { printf '[session-start] %s\n' "$*" >&2; }

export MISE_ENV=cloud
export MISE_YES=1 # never prompt (trust, install)
export MBX_DISPLAY=plain
shim_dir="$HOME/.local/share/mbx/bin"
export PATH="$shim_dir:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# Every later shell of the session sees the same environment.
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  {
    echo 'export MISE_ENV=cloud'
    echo 'export MISE_YES=1'
    echo 'export MBX_DISPLAY=plain'
    echo "export PATH=\"$shim_dir:\$HOME/.local/bin:\$HOME/.cargo/bin:\$PATH\""
  } >>"$CLAUDE_ENV_FILE"
fi

# 1. mise.
if ! command -v mise >/dev/null 2>&1; then
  log "installing mise $MISE_VERSION"
  mkdir -p "$HOME/.local/bin"
  tmp=$(mktemp -d)
  curl -fsSL --retry 3 \
    "https://github.com/jdx/mise/releases/download/v${MISE_VERSION}/mise-v${MISE_VERSION}-linux-x64.tar.gz" \
    | tar -xz -C "$tmp"
  install -m 0755 "$tmp/mise/bin/mise" "$HOME/.local/bin/mise"
  rm -rf "$tmp"
fi
for f in mise.toml mise.cloud.toml; do mise trust "$f" >/dev/null 2>&1 || true; done

# 2. mbx (mise.cloud.toml), a release binary: seconds, nothing compiled.
mise install
mbx=$(mise which mbx)
ln -sfn "$mbx" "$HOME/.local/bin/mbx"
log "$(mise --version 2>/dev/null | head -1 | cut -d' ' -f1-2), $("$mbx" --version), $(cargo --version)"

# 3. The Cargo shim. Run outside an activated mise shell, `mbx setup` installs
#    the shim and edits no mise config; PATH above already puts it first.
"$mbx" setup </dev/null >/dev/null 2>&1 || log "mbx setup failed; plain cargo will bypass the cache"

# 4. The cache budget, unless the user set one.
[ -n "$("$mbx" settings get gc.max_total_size 2>/dev/null)" ] ||
  "$mbx" settings set gc.max_total_size 8GiB >/dev/null
[ -n "$("$mbx" settings get gc.min_free_size 2>/dev/null)" ] ||
  "$mbx" settings set gc.min_free_size 2GiB >/dev/null
"$mbx" gc >/dev/null 2>&1 || true

# 5. The warm build, through the shim: debug binary, every test target, and
#    clippy's check lane — what `mise run check` builds.
if [ "${RECOUNT_SESSION_SKIP_BUILD:-}" != "1" ]; then
  log "warming the build (cargo test --no-run, cargo clippy --all-targets)"
  cargo test --no-run --quiet
  cargo clippy --all-targets --quiet
fi
log "mbx: $("$mbx" cache stats 2>/dev/null | grep -E '^(action store total|target directories)' | paste -sd ';' -)"
log "done"
