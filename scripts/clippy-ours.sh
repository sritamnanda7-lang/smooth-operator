#!/usr/bin/env bash
# SPDX-License-Identifier: FSL-1.1-MIT
# Clippy gate on OUR code only: fails if any warning/error points into desktop/src-tauri/src/so/.
# Handy's own pre-existing clippy findings are tracked, not gated (founder decision, step 1).
# Usage: scripts/clippy-ours.sh   (needs cargo + jq)
set -uo pipefail
cd "$(git rev-parse --show-toplevel)/desktop/src-tauri"
out=$(mktemp); trap 'rm -f "$out"' EXIT
cargo clippy --locked --all-targets --message-format=json > "$out" 2>/dev/null || true
if ! grep -q '"reason":"compiler-artifact"' "$out"; then echo "clippy produced no output: build broke"; exit 2; fi
hits=$(jq -r 'select(.reason=="compiler-message") | .message | select(.level=="warning" or .level=="error")
  | .spans[]? | select(.file_name | test("(^|/)src/so/")) | "\(.file_name):\(.line_start): \(.text[0].text // "")"' "$out" | sort -u)
if [ -n "$hits" ]; then echo "Clippy findings in our code:"; echo "$hits"; exit 1; fi
echo "clippy: 0 findings under src/so/"
