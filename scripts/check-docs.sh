#!/usr/bin/env bash
# Fail if the documentation and the binary disagree, or if a link is dead.
#
# Two checks, both in BOTH directions, because one-directional checks only catch
# the half of drift you happened to think of:
#
#   1. Every subcommand and every long flag the binary exposes appears in
#      docs/cli.md's `Commands` block, AND every subcommand and long flag that
#      block names still exists in the binary. The second direction is the one
#      that matters most: a doc offering a flag the parser rejects sends a user
#      to a usage error while believing the tool is broken.
#   2. Every relative link and every #anchor in README.md and docs/ resolves.
#      Anchors rot exactly like filenames, and silently.
#
# The command tree is read out of `--help` at runtime, never hardcoded here. A
# hardcoded list is the same drift problem one level down.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=${BIN:-./target/debug/recount}
CLI_DOC=docs/cli.md
fail=0
note() { printf '  ✗ %s\n' "$1"; fail=1; }

[ -x "$BIN" ] || { echo "check-docs: $BIN not built — run \`cargo build\` first" >&2; exit 2; }
[ -f "$CLI_DOC" ] || { echo "check-docs: $CLI_DOC is missing" >&2; exit 2; }

# The block between the `Commands` heading and the next h2. Everything the
# checker compares against lives there, so a flag documented in prose elsewhere
# does not count — that is deliberate: one place to look.
block=$(awk '/^## Commands$/{f=1;next} /^## /{f=0} f' "$CLI_DOC")
[ -n "$block" ] || { echo "check-docs: $CLI_DOC has no '## Commands' block" >&2; exit 2; }

echo "commands and flags"
subs=$("$BIN" --help | awk '/^Commands:/{f=1;next} /^Options:/{f=0} f && NF {print $1}' | grep -v '^help$')
for s in $subs; do
  grep -qE "(^|[^a-z-])recount $s([^a-z-]|$)" <<<"$block" \
    || note "$CLI_DOC does not document the \`$s\` subcommand"
  # Long flags, per subcommand, from the binary. --help/--version are clap's own.
  for f in $("$BIN" "$s" --help | grep -oE '(^|[ ,])--[a-z][a-z0-9-]*' | tr -d ' ,' | sort -u); do
    case "$f" in --help|--version) continue ;; esac
    grep -qF -- "$f" <<<"$block" || note "$CLI_DOC does not document \`$f\` on \`recount $s\`"
  done
done
# The other direction: a flag the doc offers that the binary does not take.
# Link targets are stripped first: an anchor like `cli.md#the---json-contract`
# contains something shaped exactly like a long flag, and reading it as one
# reported a phantom `--json-contract` nothing accepts.
for f in $(sed -E 's/\]\([^)]*\)//g' <<<"$block" | grep -oE -- '--[a-z][a-z0-9-]+' | sort -u); do
  "$BIN" --help >/dev/null 2>&1
  found=0
  for s in $subs; do "$BIN" "$s" --help 2>/dev/null | grep -qF -- "$f" && { found=1; break; }; done
  "$BIN" --help | grep -qF -- "$f" && found=1
  [ "$found" = 1 ] || note "$CLI_DOC documents \`$f\`, which no command accepts"
done

echo "links and anchors"
# Slugs the way GitHub makes them: lowercase, punctuation dropped, spaces to -.
slugs_of() { grep -E '^#{1,6} ' "$1" | sed -E 's/^#+ //' \
  | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9 _-]//g; s/ /-/g'; }
while read -r src; do
  # `|| true`: a file with no links makes grep exit 1, and under `set -o pipefail`
  # that aborted the whole pass — silently, because the abort looked like the
  # failure it was meant to report. docs/spec.md and docs/worked-example.md went
  # unchecked entirely until this was found.
  #
  # Process substitution, NOT a pipe into `while`: a piped loop is a subshell, so
  # every `fail=1` it set was discarded and this half of the checker could not
  # fail the script no matter what it found.
  while read -r t; do
    case "$t" in http*|mailto:*) continue ;; esac
    path=${t%%#*}; frag=${t#*#}; [ "$frag" = "$t" ] && frag=""
    if [ -z "$path" ]; then target="$src"; else target="$(dirname "$src")/$path"; fi
    [ -e "$target" ] || { note "$src: link target does not exist: $t"; continue; }
    if [ -n "$frag" ] && [ -f "$target" ]; then
      slugs_of "$target" | grep -qxF "$frag" || note "$src: no heading matches anchor #$frag in $path"
    fi
  done < <(grep -oE '\]\([^)#][^)]*\)|\]\(#[^)]*\)' "$src" | sed -E 's/^\]\(//; s/\)$//' || true)
  # `find`, not `git ls-files`: an untracked page is a brand-new one, which is
  # exactly the page most likely to carry a broken link.
done < <(find README.md docs -name '*.md' | sort)

if [ "$fail" -eq 0 ]; then echo "docs: the CLI reference matches the binary, and every link resolves"; fi
exit "$fail"
