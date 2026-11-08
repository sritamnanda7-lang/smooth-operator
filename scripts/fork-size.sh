#!/usr/bin/env bash
# SPDX-License-Identifier: FSL-1.1-MIT
# Gate 9: how much of Handy core did we modify?  PASS = <=15 files and <1500 added lines.
# Method (draft/step-0-plan.md gate 9): numstat vs base tag, rename-aware, only paths that
# exist in the base tag, minus files listed in scripts/fork-size-exclude.txt. Lines = added
# (column 1 of numstat); deletions are info only.
# Usage: scripts/fork-size.sh [--base <ref>] [HEAD-ref]   (default base: handy-v0.10.0-base)
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
BASE=handy-v0.10.0-base
if [ "${1:-}" = "--base" ]; then BASE=${2:?--base needs a ref}; shift 2; fi
HEAD_REF=${1:-HEAD}
MAX_FILES=15
MAX_LINES=1500
EXCL=scripts/fork-size-exclude.txt
PATHS=(desktop/src-tauri/src desktop/src desktop/src-tauri/Cargo.toml desktop/package.json)

basefiles=$(mktemp); trap 'rm -f "$basefiles"' EXIT
git ls-tree -r --name-only "$BASE" | LC_ALL=C sort > "$basefiles"

git diff --numstat -M --diff-filter=MR "$BASE" "$HEAD_REF" -- "${PATHS[@]}" | awk -F'\t' '
  function basepath(p,   a, b) {
    # rename forms: "dir/{old => new}/rest"  or  "old => new"
    if (match(p, /\{[^}]* => [^}]*\}/)) {
      a = substr(p, 1, RSTART - 1); b = substr(p, RSTART + RLENGTH)
      inner = substr(p, RSTART + 1, RLENGTH - 2)
      split(inner, parts, " => ")
      r = a parts[1] b
      gsub("//", "/", r)
      return r
    }
    if (index(p, " => ")) { split(p, parts, " => "); return parts[1] }
    return p
  }
  { add = ($1 == "-") ? 0 : $1; del = ($2 == "-") ? 0 : $2; print basepath($3) "\t" add "\t" del }
' | LC_ALL=C sort > "$basefiles.n"

files=0; added=0; deleted=0
excluded=$(grep -v '^[[:space:]]*#' "$EXCL" 2>/dev/null | sed '/^[[:space:]]*$/d' || true)
while IFS=$'\t' read -r path add del; do
  [ -n "$path" ] || continue
  grep -qxF -- "$path" "$basefiles" || continue
  # Founder decision 2026-11-08: our own code is never "Handy core" (plan of record: it lives in new
  # directories and is not counted). Skip so/ dirs here, in logic, not in the frozen exclude list.
  case "$path" in desktop/src-tauri/src/so/*|desktop/src/so/*) continue ;; esac
  if [ -n "$excluded" ] && printf '%s\n' "$excluded" | grep -qxF -- "$path"; then continue; fi
  files=$((files + 1)); added=$((added + add)); deleted=$((deleted + del))
  printf '  %5s +  %5s -  %s\n' "$add" "$del" "$path"
done < "$basefiles.n"
rm -f "$basefiles.n"

echo "files modified: $files (max $MAX_FILES)"
echo "added lines:    $added (limit <$MAX_LINES)"
echo "deleted lines:  $deleted (info only)"
if [ "$files" -le "$MAX_FILES" ] && [ "$added" -lt "$MAX_LINES" ]; then echo "GATE 9: PASS"; else echo "GATE 9: FAIL"; exit 1; fi
