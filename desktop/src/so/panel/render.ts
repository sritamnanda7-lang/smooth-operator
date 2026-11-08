// SPDX-License-Identifier: FSL-1.1-MIT
// Panel page logic. Untrusted text goes in through `textContent` only: never innerHTML, never a
// class name taken from the payload. (`textContent` sets plain text; `innerHTML` would parse it as
// HTML, so a line like `<img onerror=...>` could run code.)
// Wire format (from Rust): { v: 1, lines: [{ text: string, kind: "info" | "listening" | "warn" }] }.

const MAX_LINES = 6;
const MAX_CHARS = 200;

/** Fixed map: payload kind -> CSS class. Nothing else may ever become a class name. */
export const KIND_CLASS: Readonly<Record<string, string>> = Object.freeze({
  info: "so-info",
  listening: "so-listening",
  warn: "so-warn",
});

function isPlainObject(x: unknown): x is Record<string, unknown> {
  return typeof x === "object" && x !== null && !Array.isArray(x);
}

/** Class for a kind. Only an own key of the fixed map counts ("__proto__", "toString" etc. do not). */
function classFor(kind: unknown): string {
  if (typeof kind === "string" && Object.prototype.hasOwnProperty.call(KIND_CLASS, kind)) {
    return KIND_CLASS[kind];
  }
  return KIND_CLASS.info;
}

/** Render a payload into `root` using textContent only. Returns true if applied, false if ignored (bad version, not an array, ...). At most 6 lines. */
export function renderPayload(root: HTMLElement, payload: unknown): boolean {
  if (!isPlainObject(payload) || payload.v !== 1 || !Array.isArray(payload.lines)) {
    return false;
  }
  const doc = root.ownerDocument;
  const els: HTMLElement[] = [];
  for (const item of payload.lines) {
    if (els.length >= MAX_LINES) break;
    if (!isPlainObject(item) || typeof item.text !== "string") continue;
    const el = doc.createElement("div");
    el.className = classFor(item.kind);
    el.textContent = item.text.slice(0, MAX_CHARS);
    els.push(el);
  }
  root.replaceChildren(...els);
  return true;
}

/** The root the page writes into. Set by `installSoPanel`. */
let activeRoot: HTMLElement | null = null;

/** The one frozen object the page exposes. Created once, so calling `installSoPanel` again cannot swap it. */
const API = Object.freeze({
  set(payload: unknown): void {
    if (activeRoot === null) return;
    try {
      renderPayload(activeRoot, payload);
    } catch {
      // A bad payload must never break the page.
    }
  },
});

/** Define `win.soPanel = { set(payload) }` as non-writable, non-configurable and frozen. */
export function installSoPanel(win: Window, root: HTMLElement): void {
  activeRoot = root;
  // Already defined by us (same frozen object): nothing to redefine, only the root changed.
  if (Object.getOwnPropertyDescriptor(win, "soPanel")?.value === API) return;
  Object.defineProperty(win, "soPanel", {
    value: API,
    writable: false,
    configurable: false,
    enumerable: true,
  });
}
