// SPDX-License-Identifier: FSL-1.1-MIT
// The link state machine: pure. next() returns the new state plus effects from a fixed list. No effect
// ever deletes the token or stops the alarm, so a squatter on the port cannot make the extension forget
// its pairing. Payloads (code, nonce, token) travel in the event and are used by link.js.
import { isCode } from "./protocol.js";

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

const LIVE = [S.PAIRING, S.CONNECTING, S.CONNECTED];
const CODE_OK = [S.UNPAIRED, S.REJECTED, S.WRONG_CODE, S.EXPIRED];

/** Events: start{hasToken}, submit{code}, retry, open, hello{nonce}, authOk{proof}, paired{token},
 *  wrongCode, locked{retry_s}, expired, notPairing, rejected, closed, error.
 *  Effects: connect, send_pair, send_auth, save_token, close, retry_quick, start_ping, stop_ping. */
export function next(state, ev) {
  const to = (s, ...effects) => ({ state: s, effects });
  const reject = () => to(S.REJECTED, "close");
  const inPairing = (s) => (state === S.PAIRING ? s : reject());
  switch (ev.type) {
    case "start":
      if (LIVE.includes(state)) return to(state);
      return ev.hasToken ? to(S.CONNECTING, "connect") : to(S.UNPAIRED);
    case "submit":
      return isCode(ev.code) && CODE_OK.includes(state) ? to(S.PAIRING, "connect") : to(state);
    case "retry":
      return state === S.APP_NOT_FOUND || state === S.REJECTED ? to(S.CONNECTING, "connect") : to(state);
    case "open":
      return to(state);
    case "hello":
      if (state === S.CONNECTING) return to(state, "send_auth");
      if (state === S.PAIRING) return to(state, "send_pair");
      return reject();
    case "authOk":
      return state === S.CONNECTING ? to(S.CONNECTED, "start_ping") : reject();
    case "paired":
      return state === S.PAIRING ? to(S.CONNECTING, "save_token", "close", "connect") : reject();
    case "wrongCode":
      return inPairing(to(S.WRONG_CODE, "close"));
    case "locked":
      return inPairing(to(S.LOCKED, "close"));
    case "expired":
    case "notPairing":
      return inPairing(to(S.EXPIRED, "close"));
    case "rejected":
      return reject();
    case "closed":
    case "error":
      if (state === S.CONNECTED) return to(S.APP_NOT_FOUND, "stop_ping", "retry_quick");
      if (state === S.CONNECTING) return to(S.APP_NOT_FOUND, "retry_quick");
      if (state === S.PAIRING) return to(S.APP_NOT_FOUND);
      return to(state);
    default:
      return to(state);
  }
}
