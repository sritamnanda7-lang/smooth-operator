// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E2 + isCode: chrome-extension/lib/protocol.js. Message builders and the parser for what the app sends.
import { describe, expect, it } from "vitest";
import { authMsg, isCode, pairMsg, parseApp, pingMsg } from "../../../chrome-extension/lib/protocol.js";

const H = "ab".repeat(32);
const status = (o: Record<string, unknown>) => JSON.stringify({ t: "status", v: 1, ...o });

describe("E2 parseApp accepts exactly what the app sends", () => {
  it("hello, auth_ok, paired, the simple statuses, locked and pong", () => {
    for (const [o, check] of [
      [{ state: "hello", nonce: H }, (m: any) => m.nonce === H],
      [{ state: "auth_ok", proof: H }, (m: any) => m.proof === H],
      [{ state: "paired", token: H }, (m: any) => m.token === H],
      [{ state: "wrong_code" }, () => true],
      [{ state: "not_pairing" }, () => true],
      [{ state: "expired" }, () => true],
      [{ state: "rejected" }, () => true],
      [{ state: "locked", retry_s: 60 }, (m: any) => m.retry_s === 60],
    ] as const) {
      const r = parseApp(status(o as Record<string, unknown>));
      expect(r.ok, JSON.stringify(o)).toBe(true);
      expect((r as any).msg.state).toBe((o as any).state);
      expect(check((r as any).msg)).toBe(true);
    }
    const pong = parseApp(JSON.stringify({ t: "pong", v: 1 }));
    expect(pong.ok).toBe(true);
    expect((pong as any).msg.t).toBe("pong");
  });
});

describe("E2 parseApp rejects everything else", () => {
  const bad = (text: string, why: string) => expect(parseApp(text).ok, why).toBe(false);
  it("not JSON, not an object, unknown t, unknown state, wrong version", () => {
    bad("nope", "not json");
    bad("", "empty");
    bad("[]", "array");
    bad("null", "null");
    bad("5", "number");
    bad(JSON.stringify({ t: "nope", v: 1 }), "unknown t");
    bad(JSON.stringify({ v: 1 }), "no t");
    bad(status({ state: "nope" }), "unknown state");
    bad(status({}), "no state");
    bad(JSON.stringify({ t: "status", v: 2, state: "expired" }), "v 2");
    bad(JSON.stringify({ t: "status", state: "expired" }), "no v");
    bad(JSON.stringify({ t: "status", v: "1", state: "expired" }), "v as string");
    bad(JSON.stringify({ t: "pong", v: 2 }), "pong v2");
  });
  it("an extra field", () => {
    bad(status({ state: "expired", extra: 1 }), "extra on expired");
    bad(status({ state: "hello", nonce: H, extra: 1 }), "extra on hello");
    bad(JSON.stringify({ t: "pong", v: 1, extra: 1 }), "extra on pong");
    bad(status({ state: "hello", nonce: H, token: H }), "a field of another state");
  });
  it("more than 4,096 characters", () => {
    const pad = " ".repeat(4097 - status({ state: "expired" }).length);
    bad(status({ state: "expired" }) + pad, "4097 chars");
    expect(parseApp(status({ state: "expired" }) + " ".repeat(4096 - status({ state: "expired" }).length)).ok, "exactly 4096 is fine").toBe(true);
  });
  it("nonce, proof and token must be exactly 64 lowercase hex", () => {
    for (const [state, field] of [["hello", "nonce"], ["auth_ok", "proof"], ["paired", "token"]]) {
      for (const v of [H.slice(1), H + "0", H.toUpperCase(), "zz".repeat(32), "", 5, null, H + "\n", " " + H.slice(1)]) {
        bad(status({ state, [field]: v }), `${state}.${field}=${JSON.stringify(v)}`);
      }
      bad(status({ state }), `${state} without ${field}`);
    }
  });
  it("retry_s must be a non-negative integer", () => {
    for (const v of [-1, 1.5, "60", null, NaN, undefined]) bad(status({ state: "locked", retry_s: v }), `retry_s=${v}`);
    bad(status({ state: "locked" }), "locked without retry_s");
  });
});

describe("E2 builders produce the exact JSON", () => {
  it("pair, auth, ping", () => {
    expect(JSON.parse(pairMsg("123456"))).toEqual({ t: "pair", v: 1, code: "123456" });
    expect(JSON.parse(authMsg(H, "cd".repeat(32)))).toEqual({ t: "auth", v: 1, proof: H, cnonce: "cd".repeat(32) });
    expect(JSON.parse(pingMsg())).toEqual({ t: "ping", v: 1 });
    expect(typeof pairMsg("1")).toBe("string");
  });
});

describe("isCode: exactly 6 ASCII digits", () => {
  it("accepts and rejects", () => {
    for (const ok of ["123456", "000000", "000042", "999999"]) expect(isCode(ok), ok).toBe(true);
    for (const bad of ["12345", "1234567", "12a456", " 123456", "123456 ", "", "١٢٣٤٥٦", "+23456", "12.456", "123456\n"]) expect(isCode(bad), JSON.stringify(bad)).toBe(false);
    for (const bad of [123456, null, undefined, {}, ["123456"]]) expect(isCode(bad as never), String(bad)).toBe(false);
  });
});
