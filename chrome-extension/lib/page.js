// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 stub: functions injected into a page with chrome.scripting.executeScript. Each must stay
// self-contained (no imports, no closures, no chrome.*) because only its source text is sent.
export const SEL = {};
export function ytScan(_sel, _expectedQuery) {
  throw new Error("step 6 stub");
}
export function ytPlayer(_sel, _expectedId) {
  throw new Error("step 6 stub");
}
// ytPlay: self-contained (no closures/imports), rebuilt via fn.toString() for scripting.executeScript,
// ISOLATED world. Injected only when TRY_PLAY === true (default false, spike Z1). Must call the page's
// own video.play() once, with no click or dispatched events, and return a small enum string.
export function ytPlay(_sel, _expectedId) {
  throw new Error("step 6 stub");
}
