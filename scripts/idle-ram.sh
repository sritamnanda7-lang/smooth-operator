#!/usr/bin/env bash
# SPDX-License-Identifier: FSL-1.1-MIT
# Gate 4 (macOS): idle RSS sum of the app process tree + NEW com.apple.WebKit.* helpers, and CPU %.
# Usage: scripts/idle-ram.sh "/path/Smooth Operator.app" [--launch]   (default: attach to running app)
# Quit Safari/Mail/other WebKit apps first. Pass = RSS < 150 MB and CPU < 1% of one core.
# Writes no secrets; output only numbers and process names.
set -euo pipefail
APP=${1:?usage: idle-ram.sh <app-bundle> [--launch]}
LAUNCH=${2:-}
EXE_NAME=${SO_EXE_NAME:-smooth-operator}
WAIT=${SO_WAIT:-60}
CPU_WINDOW=${SO_CPU_WINDOW:-120}
OUT=${SO_OUT:-}

webkit_pids() { pgrep -f 'com\.apple\.WebKit\.(WebContent|Networking|GPU)' 2>/dev/null | sort -n || true; }
kids() { # all descendants of pid $1
  local p=$1 c
  for c in $(pgrep -P "$p" 2>/dev/null || true); do echo "$c"; kids "$c"; done
}
rss_kb() { ps -o rss= -p "$1" 2>/dev/null | tr -d ' ' || echo 0; }
cpu_sec() { # cumulative cputime of pid -> seconds
  ps -o cputime= -p "$1" 2>/dev/null | tr -d ' ' | awk -F'[:.]' '{ if (NF==3) print $1*60+$2+$3/100; else if (NF==4) print $1*3600+$2*60+$3+$4/100; else print 0 }'
}

before=$(webkit_pids)
if [ "$LAUNCH" = "--launch" ]; then open -n "$APP"; fi
echo "waiting ${WAIT}s for idle..."; sleep "$WAIT"

app_pid=$(pgrep -x "$EXE_NAME" | head -1 || true)
[ -n "$app_pid" ] || { echo "app process '$EXE_NAME' not found (set SO_EXE_NAME)"; exit 2; }
tree="$app_pid $(kids "$app_pid" | tr '\n' ' ')"
newwk=""
for w in $(webkit_pids); do echo "$before" | grep -qx "$w" || newwk="$newwk $w"; done
all_pids="$tree $newwk"

cpu0=0; for p in $all_pids; do cpu0=$(awk -v a="$cpu0" -v b="$(cpu_sec "$p")" 'BEGIN{print a+b}'); done
sleep "$CPU_WINDOW"
total_kb=0; cpu1=0
echo "pid      rss_MB  name"
for p in $all_pids; do
  kb=$(rss_kb "$p"); [ -n "$kb" ] || kb=0
  total_kb=$((total_kb + kb))
  cpu1=$(awk -v a="$cpu1" -v b="$(cpu_sec "$p")" 'BEGIN{print a+b}')
  printf '%-8s %6.1f  %s\n' "$p" "$(awk -v k="$kb" 'BEGIN{print k/1024}')" "$(ps -o comm= -p "$p" 2>/dev/null | xargs -I{} basename "{}")"
done
mb=$(awk -v k="$total_kb" 'BEGIN{printf "%.1f", k/1024}')
cpu=$(awk -v a="$cpu0" -v b="$cpu1" -v w="$CPU_WINDOW" 'BEGIN{printf "%.2f", (b-a)/w*100}')
echo "TOTAL_RSS_MB=$mb CPU_PCT_ONE_CORE=$cpu (window ${CPU_WINDOW}s)"
if command -v footprint >/dev/null; then
  echo "cross-check footprint (phys_footprint, may differ from RSS):"
  footprint "$app_pid" 2>/dev/null | tail -4 || true
fi
res=PASS
awk -v m="$mb" -v c="$cpu" 'BEGIN{exit !(m<150 && c<1)}' || res=FAIL
echo "GATE 4 (mac): $res  os=macOS $(sw_vers -productVersion) date=$(date +%F)"
[ -z "$OUT" ] || echo "GATE 4 (mac): $res rss_mb=$mb cpu=$cpu os=macOS $(sw_vers -productVersion) date=$(date +%F)" >> "$OUT"
[ "$res" = PASS ]
