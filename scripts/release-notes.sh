#!/usr/bin/env bash
# Print the CHANGELOG.md section for one version, for `gh release create
# --notes-file`.
#
# The changelog is written by cog (`cog bump`), so the shape is fixed: a
# `## <version> - <date>` heading, then `#### <Category>` blocks, until the next
# `## ` heading or a `- - -` separator. This extracts one such section and
# nothing else.
#
# Why a script and not `gh release create --generate-notes`: generated notes are
# a list of merged PR titles, which for this repository would be a list of merge
# commits made by an orchestrator. The changelog is grouped by conventional-commit
# type and carries the scope, which is the part a reader wants. It also means an
# empty result is a real signal — see the caller in .github/workflows/release.yml,
# which fails the release rather than publishing a release with no notes.
#
# `--self-test` runs the extraction against a synthetic changelog covering the
# cases that actually broke: a middle section, the newest section, the oldest,
# and a version that is a prefix of another (0.1.0 vs 0.1.0-rc1).
set -euo pipefail

extract() { # <file> <version>
  awk -v want="$2" '
    # A section starts at its own heading and ends at the next heading or the
    # cog separator. Anchored on the exact version, so 0.1.0 does not match
    # 0.1.0-rc1 or 10.1.0.
    /^## / {
      v = $2
      infile = (v == want)
      if (infile) { print; next }
    }
    infile && /^- - -$/ { infile = 0 }
    infile { print }
  ' "$1" | sed -e :a -e '/^\n*$/{$d;N;};/\n$/ba'  # trim trailing blank lines
}

self_test() {
  tmp="$(mktemp)"; trap 'rm -f "$tmp"' RETURN
  cat >"$tmp" <<'EOF'
# Changelog
Preamble that must never appear in notes.

- - -
## 0.2.0 - 2026-09-01
#### Features
- newest thing

- - -
## 0.1.0 - 2026-08-20
#### Bug Fixes
- middle thing

- - -
## 0.1.0-rc1 - 2026-08-19
#### Features
- prefix trap
EOF
  fail=0
  check() { # <desc> <expected-substring> <version> [<must-not-contain>]
    got="$(extract "$tmp" "$3")"
    if ! grep -qF "$2" <<<"$got"; then echo "self-test: $1 — missing '$2'"; fail=1; fi
    if [ -n "${4:-}" ] && grep -qF "$4" <<<"$got"; then echo "self-test: $1 — leaked '$4'"; fail=1; fi
  }
  check "newest section"     "newest thing" 0.2.0      "middle thing"
  check "middle section"     "middle thing" 0.1.0      "newest thing"
  check "oldest section"     "prefix trap"  0.1.0-rc1  "middle thing"
  # The prefix trap in the other direction: 0.1.0 must not swallow 0.1.0-rc1.
  check "exact version only" "middle thing" 0.1.0      "prefix trap"
  [ -z "$(extract "$tmp" 9.9.9)" ] || { echo "self-test: an absent version produced output"; fail=1; }
  grep -qF "Preamble" <<<"$(extract "$tmp" 0.2.0)" && { echo "self-test: preamble leaked"; fail=1; }
  [ "$fail" -eq 0 ] && echo "release-notes.sh: self-test passed"
  return "$fail"
}

cd "$(dirname "$0")/.."
case "${1:-}" in
  --self-test) self_test ;;
  "") echo "usage: $0 <version> | --self-test" >&2; exit 2 ;;
  *) extract CHANGELOG.md "$1" ;;
esac
