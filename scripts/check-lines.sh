#!/usr/bin/env bash
# docs/spec.md house rule: "files sized ~200-800 lines (the lease/context/diff
# unit)". The parenthetical is the rationale, and it is about the code a person
# leases, reads and diffs — not a co-located `#[cfg(test)]` module, which is
# where Rust's tests idiomatically belong and which nobody edits to change
# behaviour. So the count stops at the first top-level `#[cfg(test)]`, and the
# whole file counts when there is none.
#
# Most of this repo's long files are long because of their tests: several are
# well over 800 TOTAL lines and comfortably under it in code. A check that
# warned on those would be switched off within a week, which is worse than no
# check. The ones it does warn on are over in code, and the warning is meant.
#
# A warning, never a gate: an 850-line file is a conversation, not a build
# break. Exit is always 0.
#
# One script, two callers — `mise run lint-lines` and the ci workflow. It was
# inline in the workflow first; a local gate that can drift from the CI one is
# not a gate, so there is exactly one copy of the awk.
set -euo pipefail
cd "$(dirname "$0")/.."

fmt=${1:-human} # `human` or `github` (::warning annotations on the diff view)
found=0
while read -r f; do
  n=$(awk '/^#\[cfg\(test\)\]/ { print NR - 1; found = 1; exit }
           END { if (!found) print NR }' "$f")
  if [ "$n" -gt 800 ]; then
    found=1
    msg="$f is $n lines of code before its tests. docs/spec.md house rule: files sized ~200-800 lines (the lease/context/diff unit)."
    if [ "$fmt" = github ]; then echo "::warning file=$f::$msg"; else echo "warning: $msg"; fi
  fi
done < <(git ls-files '*.rs')

[ "$found" -eq 0 ] && echo "file length: every file is inside the ~200-800 line house rule (code before tests)"
exit 0
