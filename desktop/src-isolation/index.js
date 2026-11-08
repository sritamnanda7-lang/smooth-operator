// SPDX-License-Identifier: FSL-1.1-MIT
// Isolation hook: Tauri calls this for every IPC message from the settings page, before Rust sees it.
// It lets a message through only when it is a plain object (not a function, class instance, array,
// or anything with extra behaviour). Rust still checks the command name against the capability file.
window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (
    payload === null ||
    typeof payload !== "object" ||
    Array.isArray(payload)
  ) {
    throw new Error("IPC blocked: payload is not a plain object");
  }
  const proto = Object.getPrototypeOf(payload);
  if (proto !== Object.prototype && proto !== null) {
    throw new Error("IPC blocked: payload is not a plain object");
  }
  return payload;
};
