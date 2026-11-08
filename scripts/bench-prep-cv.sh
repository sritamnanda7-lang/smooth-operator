#!/usr/bin/env bash
# SPDX-License-Identifier: FSL-1.1-MIT
# Pick N Common Voice clips of one accent for the STT bench (plan step 2). Reads a Mozilla Data
# Collective .tar.gz, extracts ONLY the TSVs and the N chosen mp3s, converts them to 16 kHz mono WAV,
# and writes <outdir>/refs.tsv (path<TAB>sentence). Output stays on this Mac, never in a repo.
# Usage: scripts/bench-prep-cv.sh <file.tar.gz> <accent-label> <outdir> <n>
#   accent-label = exact text of the `accents` column, e.g. "Australian English".
# Needs: macOS tar (bsdtar), afconvert, python3. Prefers test.tsv (speaker-disjoint), else validated.tsv.
set -euo pipefail
[ $# -eq 4 ] || { echo "usage: $0 <file.tar.gz> <accent-label> <outdir> <n>" >&2; exit 2; }
ARCHIVE=$1; LABEL=$2; OUT=$3; N=$4
[ -f "$ARCHIVE" ] || { echo "no such file: $ARCHIVE" >&2; exit 2; }
case "$N" in ''|*[!0-9]*|0) echo "n must be a positive integer" >&2; exit 2;; esac
for t in tar afconvert python3; do command -v "$t" >/dev/null || { echo "missing tool: $t" >&2; exit 2; }; done

WORK=$(mktemp -d)   # our own scratch dir, removed on exit
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

echo "pass 1: extracting TSV files only"
tar -xzf "$ARCHIVE" -C "$WORK" --include '*.tsv'
TSV=$(find "$WORK" -name test.tsv | head -n 1)
[ -n "$TSV" ] || TSV=$(find "$WORK" -name validated.tsv | head -n 1)
[ -n "$TSV" ] || { echo "no test.tsv or validated.tsv in archive" >&2; exit 1; }

# Choose rows: exact accent match, distinct client_id, 5-15 word sentences first. Deterministic.
# -I = isolated mode (ignores env vars and the current directory).
python3 -I - "$TSV" "$LABEL" "$N" "$WORK/picked.tsv" <<'PY'
import csv, hashlib, sys
tsv, label, n, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
csv.field_size_limit(1 << 24)
with open(tsv, newline="", encoding="utf-8") as f:
    rd = csv.DictReader(f, delimiter="\t", quoting=csv.QUOTE_NONE)
    need = {"path", "sentence", "client_id", "accents"}
    missing = need - set(rd.fieldnames or [])
    if missing:
        sys.exit("TSV header lacks column(s): %s (has: %s)" % (sorted(missing), rd.fieldnames))
    rows = [r for r in rd if r["accents"] == label]
if not rows:
    sys.exit("no rows with accents == %r" % label)
rows.sort(key=lambda r: hashlib.sha1(r["path"].encode()).hexdigest())
def words(r): return len(r["sentence"].split())
picked, seen = [], set()
for ok in (lambda r: 5 <= words(r) <= 15, lambda r: words(r) >= 1):
    for r in rows:
        if len(picked) == n: break
        if r["client_id"] in seen or not ok(r): continue
        seen.add(r["client_id"]); picked.append(r)
if len(picked) < n:
    print("warning: only %d distinct speakers for %r (wanted %d)" % (len(picked), label, n), file=sys.stderr)
with open(out, "w", encoding="utf-8") as o:
    for r in picked:
        s = " ".join(r["sentence"].split())   # no tabs or newlines
        o.write("%s\t%s\n" % (r["path"], s))
PY

PICKED=$(wc -l < "$WORK/picked.tsv" | tr -d ' ')
echo "pass 2: extracting $PICKED mp3 files"
ARGS=()
while IFS=$'\t' read -r p _; do ARGS+=(--include "*/$p"); done < "$WORK/picked.tsv"
mkdir -p "$WORK/clips"
tar -xzf "$ARCHIVE" -C "$WORK/clips" "${ARGS[@]}"

: > "$OUT/refs.tsv"
while IFS=$'\t' read -r p sentence; do
  src=$(find "$WORK/clips" -name "$p" | head -n 1)
  [ -n "$src" ] || { echo "missing in archive: $p" >&2; exit 1; }
  stem=${p%.*}
  afconvert -f WAVE -d LEI16@16000 -c 1 "$src" "$OUT/$stem.wav"
  printf '%s\t%s\n' "$stem.wav" "$sentence" >> "$OUT/refs.tsv"
done < "$WORK/picked.tsv"
echo "done: $PICKED clips in $OUT (refs.tsv has sentences; keep it local)"
