// SPDX-License-Identifier: FSL-1.1-MIT
// Pure view logic for the "Chrome extension" settings group (step 5): fixed texts only. Nothing from the
// backend is shown unless it is one of these texts. Mirrors the Rust types of the 3 link IPC calls.

import { STRINGS } from "./strings";

export type LinkStateView = "Connected" | "NotPaired" | "NotFound" | "PortBusy" | "KeychainError";

export type LinkStatusView = {
  state: LinkStateView;
  pairing_secs_left: number | null;
  locked_secs: number | null;
};

/** The status line is polled this often (ms) while the pane is open. */
export const LINK_POLL_MS = 2000;

/** One fixed text per state; an unknown value reads as "not found". */
export function linkStatusText(state: LinkStateView): string {
  switch (state) {
    case "Connected":
      return STRINGS.linkConnected;
    case "NotPaired":
      return STRINGS.linkNotPaired;
    case "PortBusy":
      return STRINGS.linkPortBusy;
    case "KeychainError":
      return STRINGS.linkKeychain;
    default:
      return STRINGS.linkNotFound;
  }
}

/** "M:SS": 120 -> "2:00", 59 -> "0:59", 0 or negative -> "0:00". */
export function countdownText(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** "Type this code in the Smooth Operator extension popup. Expires in M:SS." */
export function pairHelpText(secs: number): string {
  return `${STRINGS.linkPairHelp} ${countdownText(secs)}.`;
}

/** Only the two texts Rust sends ("Too many wrong codes. Try again in N s." and the port-busy text) are
 *  shown; anything else (which could echo input) becomes the generic `linkPairFailed` text. */
export function pairErrorText(fromRust: string): string {
  if (fromRust === STRINGS.linkPortBusy || /^Too many wrong codes\. Try again in \d+ s\.$/.test(fromRust)) return fromRust;
  return STRINGS.linkPairFailed;
}
