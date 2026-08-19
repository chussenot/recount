#!/usr/bin/env bash
# Orchestrator helpers. Agents never run this; it creates and tears down the
# worktree per agent per bead, and lands proven branches under the merge mutex.
set -euo pipefail
ROOT=/home/chussenot/Documents/recount
cd "$ROOT"
case "${1:?open|land|status}" in
  open)   # open <agent>
    a=$2
    git worktree add "wt/$a" -b "wt/$a" -q master
    echo "$ROOT/wt/$a" ;;
  land)   # land <agent> [verify-cmd]
    a=$2; v=${3:-cargo test --all}
    PACT_AGENT=orchestrator pact merge "wt/$a" --verify "$v" ;;
  close)  # close <agent> — REFUSES to delete an unmerged branch
    a=$2
    if ! git merge-base --is-ancestor "wt/$a" master 2>/dev/null; then
      echo "refusing to close wt/$a: not merged into master (commit would be lost)" >&2
      exit 1
    fi
    git worktree remove --force "wt/$a" 2>/dev/null || true
    git branch -D "wt/$a" -q 2>/dev/null || true ;;
  status)
    git worktree list; echo; PACT_AGENT=orchestrator pact lease ls ;;
esac
