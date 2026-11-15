// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E1: chrome-extension/lib/state.js. `next(state, event) -> {state, effects}` is pure. The plan says
// "every state x event -> expected state + effects" but does not list them: THIS FILE IS THE TABLE (tester-specified,
// see the report). Effects are strings from a fixed vocabulary; payloads (code, nonce, token) travel in the event and
// are handled by link.js:
//   connect      open a socket (link.js refuses if one is CONNECTING or OPEN)
//   send_pair    send {"t":"pair","v":1,"code":<the submitted code>} on the current socket
//   send_auth    compute the proof for the hello nonce and send auth on the current socket
//   save_token   write the paired token to storage.local under `token`
//   close        close the current socket
//   retry_quick  schedule a reconnect with backoff.delayFor(attempt) (null = wait for the alarm)
//   start_ping   start the 20 s ping
//   stop_ping    stop the ping
// There is NO effect that deletes the token or stops the alarm, in any state, for any event.
import { describe, expect, it } from "vitest";
import { next, S } from "../../../chrome-extension/lib/state.js";

type Ev = { type: string; [k: string]: unknown };
const EVENTS: Ev[] = [
  { type: "start", hasToken: true },
  { type: "start", hasToken: false },
  { type: "submit", code: "123456" },
  { type: "submit", code: "12345" },
  { type: "submit", code: "abcdef" },
  { type: "submit", code: "١٢٣٤٥٦" },
  { type: "retry" },
  { type: "open" },
  { type: "hello", nonce: "ab".repeat(32) },
  { type: "authOk", proof: "cd".repeat(32) },
  { type: "paired", token: "ef".repeat(32) },
  { type: "wrongCode" },
  { type: "locked", retry_s: 60 },
  { type: "expired" },
  { type: "notPairing" },
  { type: "rejected" },
  { type: "closed" },
  { type: "error" },
];
const STATES = Object.values(S) as string[];
const LIVE = [S.PAIRING, S.CONNECTING, S.CONNECTED] as string[];
const CODE_OK = [S.UNPAIRED, S.REJECTED, S.WRONG_CODE, S.EXPIRED] as string[];
const REJ = { state: S.REJECTED, effects: ["close"] };

/** The oracle: written from the plan's behaviour text, independent of the implementation. */
function expected(state: string, ev: Ev): { state: string; effects: string[] } {
  const same = { state, effects: [] as string[] };
  switch (ev.type) {
    case "start":
      if (LIVE.includes(state)) return same;
      return ev.hasToken ? { state: S.CONNECTING, effects: ["connect"] } : { state: S.UNPAIRED, effects: [] };
    case "submit":
      if (!/^[0-9]{6}$/.test(String(ev.code))) return same;
      return CODE_OK.includes(state) ? { state: S.PAIRING, effects: ["connect"] } : same;
    case "retry":
      return state === S.APP_NOT_FOUND || state === S.REJECTED ? { state: S.CONNECTING, effects: ["connect"] } : same;
    case "open":
      return same;
    case "hello":
      if (state === S.CONNECTING) return { state, effects: ["send_auth"] };
      if (state === S.PAIRING) return { state, effects: ["send_pair"] };
      return REJ;
    case "authOk":
      return state === S.CONNECTING ? { state: S.CONNECTED, effects: ["start_ping"] } : REJ;
    case "paired":
      return state === S.PAIRING ? { state: S.CONNECTING, effects: ["save_token", "close", "connect"] } : REJ;
    case "wrongCode":
      return state === S.PAIRING ? { state: S.WRONG_CODE, effects: ["close"] } : REJ;
    case "locked":
      return state === S.PAIRING ? { state: S.LOCKED, effects: ["close"] } : REJ;
    case "expired":
    case "notPairing":
      return state === S.PAIRING ? { state: S.EXPIRED, effects: ["close"] } : REJ;
    case "rejected":
      return REJ;
    case "closed":
    case "error":
      if (state === S.CONNECTED) return { state: S.APP_NOT_FOUND, effects: ["stop_ping", "retry_quick"] };
      if (state === S.CONNECTING) return { state: S.APP_NOT_FOUND, effects: ["retry_quick"] };
      if (state === S.PAIRING) return { state: S.APP_NOT_FOUND, effects: [] };
      return same;
  }
  throw new Error("oracle: unknown event " + ev.type);
}

describe("E1 S has the nine states", () => {
  it("names", () => {
    expect([...STATES].sort()).toEqual(["APP_NOT_FOUND", "CONNECTED", "CONNECTING", "EXPIRED", "LOCKED", "PAIRING", "REJECTED", "UNPAIRED", "WRONG_CODE"]);
  });
});

describe("E1 next() table: every state x event", () => {
  for (const state of STATES) {
    for (const ev of EVENTS) {
      it(`${state} + ${ev.type}${"code" in ev ? `(${ev.code})` : ""}${"hasToken" in ev ? `(${ev.hasToken})` : ""}`, () => {
        const want = expected(state, ev);
        const r = next(state, structuredClone(ev));
        expect(r.state).toBe(want.state);
        expect(r.effects).toEqual(want.effects);
      });
    }
  }
});

describe("E1 invariants", () => {
  const VOCAB = ["connect", "send_pair", "send_auth", "save_token", "close", "retry_quick", "start_ping", "stop_ping"];
  it("effects come from the fixed vocabulary and never delete the token or stop the alarm", () => {
    for (const state of STATES) {
      for (const ev of EVENTS) {
        for (const fx of next(state, structuredClone(ev)).effects) {
          expect(VOCAB, `${state} + ${ev.type}`).toContain(fx);
          expect(String(fx)).not.toMatch(/delete|remove|clear|alarm|forget/i);
        }
      }
    }
  });
  it("submit with a non-6-digit code never sends and never changes state", () => {
    for (const state of STATES) {
      for (const code of ["12345", "1234567", "12a456", "", " 123456", "١٢٣٤٥٦", "12345\n"]) {
        const r = next(state, { type: "submit", code });
        expect(r.state, `${state} ${JSON.stringify(code)}`).toBe(state);
        expect(r.effects).toEqual([]);
      }
    }
  });
  it("is pure: same input, same output; the input event is not changed", () => {
    for (const state of STATES) {
      for (const ev of EVENTS) {
        const copy = structuredClone(ev);
        const a = next(state, ev);
        const b = next(state, ev);
        expect(b).toEqual(a);
        expect(ev).toEqual(copy);
      }
    }
  });
  it("an unexpected server message in any state ends in REJECTED, never CONNECTED", () => {
    for (const state of STATES.filter((s) => s !== S.CONNECTING)) expect(next(state, { type: "authOk", proof: "cd".repeat(32) }).state).toBe(S.REJECTED);
    for (const state of STATES.filter((s) => s !== S.PAIRING)) expect(next(state, { type: "paired", token: "ef".repeat(32) }).state).toBe(S.REJECTED);
  });
});
