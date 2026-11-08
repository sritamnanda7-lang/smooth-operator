// SPDX-License-Identifier: FSL-1.1-MIT
// STUB. Tests: tests/so/fnCheckView.test.ts (V4). Shapes follow Rust's default serde output.

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

/** Pure. Nothing is shown unless `report.applies`. */
export function fnCheckView(_report: FnCheckReport): FnCheckView {
  throw new Error("not implemented: fnCheckView");
}
