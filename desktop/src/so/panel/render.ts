// SPDX-License-Identifier: FSL-1.1-MIT
// STUB (step 1 sub-step 1): the builder fills these. Tests: tests/so/panel.test.ts (V1-V3).
// Wire format (from Rust): { v: 1, lines: [{ text: string, kind: "info" | "listening" | "warn" }] }.

/** Fixed map: payload kind -> CSS class. Nothing else may ever become a class name. */
export const KIND_CLASS: Readonly<Record<string, string>> = Object.freeze({});

/** Render a payload into `root` using textContent only. Returns true if applied, false if ignored (bad version, not an array, ...). At most 6 lines. */
export function renderPayload(_root: HTMLElement, _payload: unknown): boolean {
  throw new Error("not implemented: renderPayload");
}

/** Define `win.soPanel = { set(payload) }` as non-writable, non-configurable and frozen. */
export function installSoPanel(_win: Window, _root: HTMLElement): void {
  throw new Error("not implemented: installSoPanel");
}
