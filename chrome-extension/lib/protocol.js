// SPDX-License-Identifier: FSL-1.1-MIT
// Message builders and the parser for what the app sends. Stubs until the tests exist.

/** @returns {{ok: true, msg: object} | {ok: false}} */
export function parseApp(_text) {
  throw new Error("not implemented");
}
export function pairMsg(_code) {
  throw new Error("not implemented");
}
export function authMsg(_proof, _cnonce) {
  throw new Error("not implemented");
}
export function pingMsg() {
  throw new Error("not implemented");
}
/** Exactly 6 ASCII digits. */
export function isCode(_s) {
  throw new Error("not implemented");
}
