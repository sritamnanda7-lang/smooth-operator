// SPDX-License-Identifier: FSL-1.1-MIT
// Turns the Rust Fn-check report into plain text lines. Pure: no DOM, no IPC. Shapes follow Rust's default serde output.
import { STRINGS } from "./strings";

export type Globe = "Ok" | "Unknown" | { Warn: string };
export type Dictation = "Ok" | "Warn" | "Unknown";
export interface FnCheckReport {
  applies: boolean;
  globe: Globe;
  dictation: Dictation;
  apps: string[];
}
export interface FnCheckView {
  /** Human-readable problem lines (English, from strings.ts). Empty when nothing to show. */
  messages: string[];
  /** Show the "Use Right Cmd" button. */
  offerRightCmd: boolean;
}

/** Pure. Nothing is shown unless `report.applies`. "Unknown" is not a problem. */
export function fnCheckView(report: FnCheckReport): FnCheckView {
  if (!report.applies) return { messages: [], offerRightCmd: false };
  const messages: string[] = [];
  if (
    typeof report.globe === "object" &&
    report.globe !== null &&
    "Warn" in report.globe
  ) {
    messages.push(`${report.globe.Warn}. ${STRINGS.globeAdvice}`);
  }
  if (report.dictation === "Warn") messages.push(STRINGS.dictationMessage);
  if (report.apps.length > 0)
    messages.push(`${STRINGS.appsMessage} ${report.apps.join(", ")}.`);
  return { messages, offerRightCmd: messages.length > 0 };
}
