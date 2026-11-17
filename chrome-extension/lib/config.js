// SPDX-License-Identifier: FSL-1.1-MIT
// Constants shared by the service worker and the popup. The port must match LINK_PORT in the app.
export const PORT = 47815;
export const WS_URL = "ws://127.0.0.1:47815/";
export const MAX_MSG = 4096;
export const PING_MS = 20000;
export const ALARM = "so-reconnect";
export const ALARM_MIN = 0.5;

// Step 6: page jobs. Times in ms (spike Z1/Z7: results ~1 s, playing ~3 s in a foreground tab).
export const RESULTS_WAIT = 8000;
export const WATCH_WAIT = 8000;
export const PLAY_WAIT = 6000;
export const AD_WAIT = 15000;
export const PAGE_WAIT = 15000;
export const SCAN_POLL = 200;
export const PLAYER_POLL = 500;
export const PAGE_POLL = 250;
export const LOGIN_CHECKS = 2; // tab complete but URL unreadable this many polls in a row = Google wants the user
export const EMPTY_WAIT = 2000; // results list empty this long without a break = no video (it may still be rendering)
export const ACTIVE_TAB = true; // spike Z1: a background tab did not load
export const TRY_PLAY = false; // spike Z1: the video played by itself in a foreground tab, so no video.play()
export const DOCS_URL = "https://docs.new/";
export const DOCS_EDIT_RE = /^https:\/\/docs\.google\.com\/document\/(u\/\d+\/)?d\/[\w-]+\/edit/;
export const CAL_EDIT_RE = /^https:\/\/calendar\.google\.com\/calendar\/(u\/\d+\/)?r\/eventedit/;
export const VIDEO_ID_RE = /^[A-Za-z0-9_-]{11}$/;
export const MAX_ID = 2147483647;
export const MAX_MS = 120000;
export const MAX_DAYS = 400;
export const JOB_MAX = 75000; // longest a job may run: more than the slowest honest path (about 61 s), less than the app's 90 s
export const JOB_TICK = 250; // slice for waiting on the storage lockdown
