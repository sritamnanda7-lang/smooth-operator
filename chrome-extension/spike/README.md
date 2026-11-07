<!-- SPDX-License-Identifier: FSL-1.1-MIT -->
# Smooth Operator Spike extension

Test-only MV3 extension for step 0, gate 7. Connects to `ws://127.0.0.1:51717`, sends `{"type":"hello","ext":"so-spike"}`, expects `{"type":"ack","app":"so-spike"}`. Retries every 5 s.

## Load it
1. Open Chrome, go to `chrome://extensions`.
2. Turn on **Developer mode** (top right).
3. Click **Load unpacked**, pick this `spike` folder.
4. Click the extension icon to see: connected / not connected, last error, Chrome version, time.

## Gate 7 note
Record any Local Network Access prompt or firewall prompt (Chrome or OS) exactly as shown: text, which button you clicked, and the OS/Chrome version.
