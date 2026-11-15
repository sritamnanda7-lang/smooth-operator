// SPDX-License-Identifier: FSL-1.1-MIT
// Popup text per state. Fixed texts only; the popup renders them with textContent.
import { S } from "./state.js";

const TEXT = {
  [S.CONNECTED]: "Connected to Smooth Operator",
  [S.UNPAIRED]: "Not paired. In Smooth Operator: Settings → General → Chrome extension → Pair, then type the code here.",
  [S.APP_NOT_FOUND]: "Fix connection: Smooth Operator isn't running. Open it, then click Retry.",
  [S.REJECTED]: "Fix connection: pair again",
  [S.WRONG_CODE]: "Wrong code: check Smooth Operator",
  [S.EXPIRED]: "Code expired: click Pair in Smooth Operator for a new code",
  [S.PAIRING]: "Pairing…",
  [S.CONNECTING]: "Connecting…",
};
const SHOW_CODE = [S.UNPAIRED, S.REJECTED, S.WRONG_CODE, S.EXPIRED];

/** @returns {{text: string, fix: boolean, showCode: boolean, showRetry: boolean}} */
export function popupView(link) {
  const state = link && typeof link === "object" ? link.state : undefined;
  let text = Object.hasOwn(TEXT, state) ? TEXT[state] : "Checking the connection…";
  if (state === S.LOCKED) {
    const n = Number.isInteger(link.retry_s) && link.retry_s >= 0 ? `${link.retry_s} s` : "a moment";
    text = `Too many tries: wait ${n}, then click Pair in the app again`;
  }
  return {
    text,
    fix: state === S.APP_NOT_FOUND || state === S.REJECTED,
    showCode: SHOW_CODE.includes(state),
    showRetry: state === S.APP_NOT_FOUND,
  };
}
