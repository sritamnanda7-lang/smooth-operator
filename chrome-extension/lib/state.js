// SPDX-License-Identifier: FSL-1.1-MIT
// The link state machine: pure. next() returns the new state plus effects (never "delete token" or
// "stop alarm"). Stub until the tests exist.
export const S = Object.freeze({
  UNPAIRED: "UNPAIRED",
  PAIRING: "PAIRING",
  CONNECTING: "CONNECTING",
  CONNECTED: "CONNECTED",
  APP_NOT_FOUND: "APP_NOT_FOUND",
  REJECTED: "REJECTED",
  WRONG_CODE: "WRONG_CODE",
  LOCKED: "LOCKED",
  EXPIRED: "EXPIRED",
});

/** Events: start{hasToken}, submit{code}, retry, open, hello{nonce}, authOk{proof}, paired{token},
 *  wrongCode, locked{retry_s}, expired, notPairing, rejected, closed, error. */
export function next(_state, _event) {
  throw new Error("not implemented");
}
