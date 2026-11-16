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

// ---------------- step 6: E2 resMsg + R, and the cmd / cancel messages ----------------
import { R, resMsg } from "../../../chrome-extension/lib/protocol.js";

const CODES = ["playing", "ad", "paused", "no_video", "editor_open", "needs_login", "consent", "no_access", "timeout", "bad_cmd", "busy", "unsafe", "cancelled", "error"];

describe("E2 (step 6) R and resMsg", () => {
  it("R is a frozen enum of exactly the 14 result codes", () => {
    expect(Object.isFrozen(R)).toBe(true);
    expect([...Object.values(R)].sort()).toEqual([...CODES].sort());
  });
  it("resMsg has exactly the keys t, v, id, r, ms", () => {
    const m = JSON.parse(resMsg(7, "playing", 1234));
    expect(Object.keys(m).sort()).toEqual(["id", "ms", "r", "t", "v"]);
    expect(m).toEqual({ t: "res", v: 1, id: 7, r: "playing", ms: 1234 });
    for (const c of CODES) expect(JSON.parse(resMsg(1, c as never, 5)).r).toBe(c);
  });
  it("ms is an integer clamped to 0..120000", () => {
    const ms = (x: number) => JSON.parse(resMsg(1, "playing", x)).ms;
    expect(ms(-5)).toBe(0);
    expect(ms(999_999)).toBe(120_000);
    expect(ms(120_000)).toBe(120_000);
    expect(ms(0)).toBe(0);
    expect(Number.isInteger(ms(12.7)) && ms(12.7) >= 12 && ms(12.7) <= 13).toBe(true);
    for (const bad of [NaN, Infinity, -Infinity]) {
      const v = ms(bad);
      expect(Number.isInteger(v) && v >= 0 && v <= 120_000, String(bad)).toBe(true);
    }
  });
  it("r only comes from R: an unknown code throws or becomes error, never goes out as is", () => {
    expect(typeof resMsg).toBe("function");
    expect(JSON.parse(resMsg(1, "playing", 5)).r, "positive control").toBe("playing");
    let out: string | undefined;
    try {
      out = resMsg(1, "nope" as never, 5);
    } catch {
      return;
    }
    expect(JSON.parse(out).r).toBe("error");
  });
  it("never any page text: a title-like extra argument does not appear", () => {
    expect(resMsg(1, "playing", 5, "Secret Title" as never)).not.toContain("Secret");
  });
});

describe("parseApp (step 6): cmd and cancel", () => {
  const YT = "https://www.youtube.com/results?search_query=Blinding+Lights";
  it("accepts a valid cmd and cancel", () => {
    for (const o of [{ t: "cmd", v: 1, id: 7, op: "yt_play", url: YT }, { t: "cmd", v: 1, id: 8, op: "docs_open" }]) {
      const r = parseApp(JSON.stringify(o)) as any;
      expect(r.ok, JSON.stringify(o)).toBe(true);
      expect(r.msg.t).toBe("cmd");
      expect(r.msg.id).toBe(o.id);
    }
    const c = parseApp(JSON.stringify({ t: "cancel", v: 1, id: 7 })) as any;
    expect(c.ok).toBe(true);
    expect(c.msg).toMatchObject({ t: "cancel", id: 7 });
  });
  it("rejects a cancel with an extra key or a bad id", () => {
    for (const o of [{ t: "cancel", v: 1, id: 7, x: 1 }, { t: "cancel", v: 1, id: 0 }, { t: "cancel", v: 1, id: "7" }, { t: "cancel", v: 1 }, { t: "cancel", v: 2, id: 7 }, { t: "cancel", v: 1, id: 2147483648 }]) {
      expect(parseApp(JSON.stringify(o)).ok, JSON.stringify(o)).toBe(false);
    }
  });
});
