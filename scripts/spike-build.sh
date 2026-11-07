#!/usr/bin/env bash
# SPDX-License-Identifier: FSL-1.1-MIT
# Step 0 gate 5 helper (Mac): build a spike app of a given version, signed for the updater test.
# Usage: scripts/spike-build.sh <version> [--adhoc]
#   <version>  e.g. 0.0.1 (the app) or 0.0.2 (the update). Overrides "version" in tauri.spike.conf.json.
#   --adhoc    sign the .app ad-hoc ("-") instead of the local self-signed identity.
# Needs: ~/.so-keys/spike-updater.key (private updater key, empty password; only its PATH is used here)
#        login keychain identity "Smooth Operator Self-Signed" (not needed with --adhoc).
# Output: desktop/src-tauri/target/spike-out/<version>[-adhoc]/ with the .app.tar.gz, .sig, a plain dmg and a
#         latest.json for `python3 -m http.server 8765 --bind 127.0.0.1` run in that folder.
# Signing: Tauri signs the .app while bundling when APPLE_SIGNING_IDENTITY is set, so the updater tarball
#          and the dmg both hold the signed app (signing afterwards would leave the tarball unsigned).
set -euo pipefail
export PATH="$HOME/.bun/bin:$HOME/.cargo/bin:$HOME/Applications/cmake-4.4.5-macos-universal/CMake.app/Contents/bin:$PATH"

VERSION=${1:?usage: spike-build.sh <version> [--adhoc]}
ADHOC=0
[ "${2:-}" = "--adhoc" ] && ADHOC=1
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 0.0.1" >&2; exit 2; }

ROOT=$(cd "$(dirname "$0")/.." && pwd)
KEY="$HOME/.so-keys/spike-updater.key"
[ -f "$KEY" ] || { echo "missing $KEY" >&2; exit 2; }

if [ "$ADHOC" = 1 ]; then IDENTITY="-"; TAG="$VERSION-adhoc"; else IDENTITY="Smooth Operator Self-Signed"; TAG="$VERSION"; fi

# Tauri 2 reads the key from TAURI_SIGNING_PRIVATE_KEY; a file path is accepted. Only the path is set here.
export TAURI_SIGNING_PRIVATE_KEY="$KEY"
export TAURI_SIGNING_PRIVATE_KEY_PATH="$KEY"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
export APPLE_SIGNING_IDENTITY="$IDENTITY"

cd "$ROOT/desktop"
bun run tauri build --bundles app --features spike \
  --config src-tauri/tauri.spike.conf.json \
  --config "{\"version\":\"$VERSION\"}" -- --locked

BUNDLE="$ROOT/desktop/src-tauri/target/release/bundle"
APP="$BUNDLE/macos/Smooth Operator.app"
OUT="$ROOT/desktop/src-tauri/target/spike-out/$TAG"
mkdir -p "$OUT"
cp "$BUNDLE/macos/Smooth Operator.app.tar.gz" "$BUNDLE/macos/Smooth Operator.app.tar.gz.sig" "$OUT/"
# Plain dmg with hdiutil (Tauri's own dmg step scripts Finder and fails without a GUI session).
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "Smooth Operator" -srcfolder "$STAGE" -ov -format UDZO "$OUT/Smooth Operator_${TAG}_aarch64.dmg"
mv "$STAGE" "$HOME/.Trash/spike-dmg-stage-$$"

echo "--- codesign result ($TAG)"
codesign -dv "$APP" 2>&1 | grep -E "Identifier|Authority|Signature|flags|TeamIdentifier" || true
codesign --verify --deep --strict "$APP" && echo "codesign verify: OK"
codesign -d -r- "$APP" 2>&1 | grep designated || true

# latest.json for the local update server (signature file content is public).
SIG=$(cat "$OUT/Smooth Operator.app.tar.gz.sig")
python3 - "$VERSION" "$SIG" "$OUT/latest.json" <<'PY'
import json, sys, datetime
version, sig, path = sys.argv[1:4]
url = "http://127.0.0.1:8765/Smooth%20Operator.app.tar.gz"
doc = {"version": version, "notes": "spike", "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
       "platforms": {"darwin-aarch64": {"signature": sig, "url": url}}}
open(path, "w").write(json.dumps(doc, indent=2) + "\n")
PY
# Never leave a runnable .app in the build tree (LaunchServices would launch it): Trash it.
mv "$APP" "$HOME/.Trash/Smooth Operator-spike-$TAG-$$.app"
echo "built: $OUT"
ls -l "$OUT"
