// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E3, E4, E5, E7, E8, E9: chrome-extension/lib/link.js (`startLink(env)`), against a fake `chrome`, a fake
// WebSocket and a hand-advanced timer (tests/so/support/chromeFake.ts: it lists the exact API surface link.js may use).
// The server side is played by the test with node:crypto HMAC (key = raw token bytes), independent of proof.js.
import { afterEach, describe, expect, it, vi } from "vitest";
import { startLink } from "../../../chrome-extension/lib/link.js";
import { S } from "../../../chrome-extension/lib/state.js";
import { appProof, extProof, FakeWebSocket, hex64, makeEnv, openAndHello, settle, until, writtenKeys, type Rig } from "../support/chromeFake";

const TOKEN = "ab".repeat(32);
const NEW_TOKEN = "cd".repeat(32);
const URL = "ws://127.0.0.1:47815/";
const stops: (() => void)[] = [];
afterEach(() => {
  stops.splice(0).forEach((f) => f());
  vi.restoreAllMocks();
});

async function boot(opts: { token?: string } = {}) {
  const r = makeEnv(opts);
  const h = startLink(r.env);
  stops.push(() => h.stop());
  await settle();
  return r;
}
const linkOf = (r: Rig) => r.store.link as { state: string; retry_s?: number } | undefined;
const stateOf = (r: Rig) => linkOf(r)?.state;
const status = (o: Record<string, unknown>) => JSON.stringify({ t: "status", v: 1, ...o });

/** Drives one socket through hello -> auth -> auth_ok as the real app would. Returns the auth message the worker sent. */
async function authenticate(s: FakeWebSocket, token = TOKEN, nonce = hex64(0x11)) {
  await openAndHello(s, nonce);
  await until(() => s.sentOf("auth").length === 1, "the auth message");
  const a = s.sentOf("auth")[0] as { proof: string; cnonce: string };
  expect(a.proof).toBe(extProof(token, nonce));
  s.serverSend(status({ state: "auth_ok", proof: appProof(token, a.cnonce) }));
  await settle();
  return a;
}
/** Starts a pairing from the popup and answers `pair` with `reply`. */
async function pairWith(r: Rig, code: string, reply: Record<string, unknown> | null) {
  const before = r.sockets.length;
  r.emit.popup({ t: "pair", code });
  await until(() => r.sockets.length === before + 1, "a socket for pairing");
  const s = r.sockets[before];
  await openAndHello(s, hex64(0x22));
  await until(() => s.sentOf("pair").length === 1, "the pair message");
  if (reply) {
    s.serverSend(status(reply));
    await settle();
  }
  return s;
}

describe("E5 connecting", () => {
  it("with a token: exactly one socket, to exactly ws://127.0.0.1:47815/, state CONNECTING; the alarm is set", async () => {
    const r = await boot({ token: TOKEN });
    expect(r.sockets.length).toBe(1);
    expect(r.sockets[0].url).toBe(URL);
    expect(stateOf(r)).toBe(S.CONNECTING);
    const alarm = r.alarmsCreated.find((a) => a.name === "so-reconnect");
    expect(alarm, "chrome.alarms.create('so-reconnect', ...)").toBeDefined();
    expect(alarm!.info.periodInMinutes).toBe(0.5);
  });

  it("with no token: no socket, state UNPAIRED, the alarm is still set", async () => {
    const r = await boot();
    expect(r.sockets.length).toBe(0);
    expect(stateOf(r)).toBe(S.UNPAIRED);
    expect(r.alarmsCreated.some((a) => a.name === "so-reconnect")).toBe(true);
  });

  it("onStartup + onInstalled + the alarm in the SAME tick as the start give exactly one socket (CONNECTING counts)", async () => {
    const r = makeEnv({ token: TOKEN });
    const h = startLink(r.env);
    stops.push(() => h.stop());
    r.emit.startup();
    r.emit.installed();
    r.emit.alarm("so-reconnect");
    await settle();
    expect(r.sockets.length).toBe(1);
    r.emit.alarm("so-reconnect");
    await settle();
    expect(r.sockets.length, "an alarm while CONNECTING opens nothing").toBe(1);
  });

  it("an alarm while connected opens no second socket; an alarm with another name is ignored", async () => {
    const r = await boot({ token: TOKEN });
    await authenticate(r.sockets[0]);
    expect(stateOf(r)).toBe(S.CONNECTED);
    r.emit.alarm("so-reconnect");
    r.emit.startup();
    await settle();
    expect(r.sockets.length).toBe(1);
    r.sockets[0].serverClose();
    await settle();
    r.emit.alarm("something-else");
    await settle();
    expect(r.sockets.length, "a foreign alarm must not connect").toBe(1);
  });

  it("sends nothing before the app's hello and sends auth exactly once", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    s.open();
    await settle();
    expect(s.sent, "nothing is sent until hello arrives (the proof needs the nonce)").toEqual([]);
    s.serverSend(status({ state: "hello", nonce: hex64(0x11) }));
    await until(() => s.sentOf("auth").length === 1, "auth");
    await settle();
    expect(s.sentOf("auth").length).toBe(1);
    expect(s.sentOf("pair").length).toBe(0);
  });
});

describe("E5 pairing, then the normal auth path", () => {
  it("paired (after our own pair) saves the token under `token`, closes the socket, reconnects and authenticates; ping every 20 s", async () => {
    const r = await boot();
    const s0 = await pairWith(r, "123456", { state: "paired", token: NEW_TOKEN });
    expect(s0.sentOf("pair")[0]).toEqual({ t: "pair", v: 1, code: "123456" });
    expect(s0.sentOf("auth").length, "a pairing socket never sends auth").toBe(0);
    expect(r.store.token).toBe(NEW_TOKEN);
    expect(s0.closed).toBe(true);
    await until(() => r.sockets.length === 2, "the reconnect after pairing");
    const s1 = r.sockets[1];
    expect(s1.url).toBe(URL);
    await authenticate(s1, NEW_TOKEN, hex64(0x33));
    expect(stateOf(r)).toBe(S.CONNECTED);
    // ping every 20 s while authenticated
    expect(s1.sentOf("ping").length).toBe(0);
    await r.timers.advance(20_000);
    expect(s1.sentOf("ping")).toEqual([{ t: "ping", v: 1 }]);
    await r.timers.advance(20_000);
    expect(s1.sentOf("ping").length).toBe(2);
    await r.timers.advance(19_999);
    expect(s1.sentOf("ping").length, "not before the 20 s are up").toBe(2);
  });

  it("the ping stops when the socket closes", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await authenticate(s);
    s.serverClose();
    await settle();
    const n = s.sent.length;
    expect(stateOf(r), "right after the close (before the 1 s quick retry fires)").toBe(S.APP_NOT_FOUND);
    await r.timers.advance(60_000);
    expect(s.sent.length, "no ping on the closed socket").toBe(n);
    expect(s.sentOf("ping").length).toBe(0);
  });

  it("a pair answer of wrong_code / locked / expired / not_pairing ends in the matching state and keeps the pairing socket closed", async () => {
    for (const [reply, state, retry] of [
      [{ state: "wrong_code" }, S.WRONG_CODE, undefined],
      [{ state: "locked", retry_s: 60 }, S.LOCKED, 60],
      [{ state: "expired" }, S.EXPIRED, undefined],
      [{ state: "not_pairing" }, S.EXPIRED, undefined],
    ] as const) {
      const r = await boot();
      const s = await pairWith(r, "654321", reply);
      expect(stateOf(r), JSON.stringify(reply)).toBe(state);
      if (retry) expect(linkOf(r)!.retry_s).toBe(retry);
      expect(s.closed).toBe(true);
      expect(r.store.token, "no token appears from a failed pairing").toBeUndefined();
      await settle();
      expect(r.sockets.length, "no auto-reconnect without a token").toBe(1);
      r.emit.alarm("so-reconnect");
      await settle();
      expect(r.sockets.length, "the alarm does not connect without a token").toBe(1);
    }
  });

  it("after WRONG_CODE the user can try again (a new socket)", async () => {
    const r = await boot();
    await pairWith(r, "111111", { state: "wrong_code" });
    await pairWith(r, "222222", { state: "wrong_code" });
    expect(r.sockets.length).toBe(2);
    expect(r.sockets[1].sentOf("pair")[0]).toEqual({ t: "pair", v: 1, code: "222222" });
  });

  it("the app being down while pairing reads as APP_NOT_FOUND; Retry without a token goes back to UNPAIRED and opens nothing", async () => {
    const r = await boot();
    r.emit.popup({ t: "pair", code: "123456" });
    await until(() => r.sockets.length === 1, "pairing socket");
    r.sockets[0].refuse();
    await settle();
    expect(stateOf(r)).toBe(S.APP_NOT_FOUND);
    r.emit.popup({ t: "retry" });
    await settle();
    expect(r.sockets.length).toBe(1);
    expect(stateOf(r)).toBe(S.UNPAIRED);
  });

  it("an invalid code (not 6 digits) from the popup sends nothing and opens no socket", async () => {
    const r = await boot();
    for (const code of ["12345", "1234567", "abcdef", "", " 123456", "١٢٣٤٥٦"]) r.emit.popup({ t: "pair", code });
    await settle();
    expect(r.sockets.length).toBe(0);
    expect(stateOf(r)).toBe(S.UNPAIRED);
  });
});

describe("E5 REJECTED never forgets the pairing", () => {
  it("rejected: state REJECTED, socket closed, token unchanged, the alarm is still set and reconnects on the next alarm", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await openAndHello(s, hex64(0x11));
    await until(() => s.sentOf("auth").length === 1, "auth");
    s.serverSend(status({ state: "rejected" }));
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s.closed).toBe(true);
    expect(r.store.token).toBe(TOKEN);
    expect(r.removes).toEqual([]);
    expect(r.clears()).toBe(0);
    expect(r.alarmsCleared).not.toContain("so-reconnect");
    expect(writtenKeys(r)).not.toContain("token");
    r.emit.alarm("so-reconnect");
    await until(() => r.sockets.length === 2, "reconnect on the alarm after REJECTED");
    expect(r.sockets[1].url).toBe(URL);
  });

  it("Retry from the popup in REJECTED or APP_NOT_FOUND reconnects when a token exists", async () => {
    const r = await boot({ token: TOKEN });
    r.sockets[0].refuse();
    await settle();
    expect(stateOf(r)).toBe(S.APP_NOT_FOUND);
    r.emit.popup({ t: "retry" });
    await until(() => r.sockets.length === 2, "retry opens a socket");
  });
});

describe("E4 backoff in the worker", () => {
  const retryTimers = (r: Rig) => r.timers.pending().filter((t) => !t.every).map((t) => t.ms);
  it("1 s, 2 s, 4 s, then only the alarm; reset after a verified auth_ok", async () => {
    const r = await boot({ token: TOKEN });
    r.sockets[0].refuse();
    await settle();
    expect(stateOf(r)).toBe(S.APP_NOT_FOUND);
    expect(retryTimers(r)).toEqual([1000]);
    await r.timers.advance(1000);
    expect(r.sockets.length).toBe(2);
    r.sockets[1].refuse();
    await settle();
    expect(retryTimers(r)).toEqual([2000]);
    await r.timers.advance(2000);
    expect(r.sockets.length).toBe(3);
    r.sockets[2].refuse();
    await settle();
    expect(retryTimers(r)).toEqual([4000]);
    await r.timers.advance(4000);
    expect(r.sockets.length).toBe(4);
    r.sockets[3].refuse();
    await settle();
    expect(retryTimers(r), "after the third quick retry only the alarm is left").toEqual([]);
    await r.timers.advance(100_000);
    expect(r.sockets.length).toBe(4);
    // The alarm brings it back; a verified auth_ok resets the counter.
    r.emit.alarm("so-reconnect");
    await until(() => r.sockets.length === 5, "alarm reconnect");
    await authenticate(r.sockets[4]);
    expect(stateOf(r)).toBe(S.CONNECTED);
    r.sockets[4].serverClose();
    await settle();
    expect(retryTimers(r), "back to the first delay").toEqual([1000]);
  });
});

describe("E3 the app must prove itself too", () => {
  it("a wrong server proof is REJECTED and the socket is closed; the token stays", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await openAndHello(s, hex64(0x11));
    await until(() => s.sentOf("auth").length === 1, "auth");
    const a = s.sentOf("auth")[0] as { cnonce: string };
    s.serverSend(status({ state: "auth_ok", proof: appProof(NEW_TOKEN, a.cnonce) })); // proof made with another token
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s.closed).toBe(true);
    expect(r.store.token).toBe(TOKEN);
  });

  it("a proof for the client's OWN proof direction (ext) is not accepted as the app's proof", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await openAndHello(s, hex64(0x11));
    await until(() => s.sentOf("auth").length === 1, "auth");
    const a = s.sentOf("auth")[0] as { cnonce: string };
    s.serverSend(status({ state: "auth_ok", proof: extProof(TOKEN, a.cnonce) }));
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
  });

  it("two sockets send different cnonces; an auth_ok carrying the proof for the OLDER socket's cnonce is REJECTED", async () => {
    const r = await boot({ token: TOKEN });
    const s0 = r.sockets[0];
    await openAndHello(s0, hex64(0x11));
    await until(() => s0.sentOf("auth").length === 1, "auth 0");
    const c0 = (s0.sentOf("auth")[0] as { cnonce: string }).cnonce;
    s0.serverClose();
    await settle();
    await r.timers.advance(1000);
    expect(r.sockets.length).toBe(2);
    const s1 = r.sockets[1];
    await openAndHello(s1, hex64(0x12));
    await until(() => s1.sentOf("auth").length === 1, "auth 1");
    const c1 = (s1.sentOf("auth")[0] as { cnonce: string }).cnonce;
    expect(c0).toMatch(/^[0-9a-f]{64}$/);
    expect(c1).toMatch(/^[0-9a-f]{64}$/);
    expect(c1, "a fresh cnonce per socket").not.toBe(c0);
    s1.serverSend(status({ state: "auth_ok", proof: appProof(TOKEN, c0) }));
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s1.closed).toBe(true);
  });

  it("the token never appears in any message the worker sends; proofs are not the token", async () => {
    const r = await boot({ token: TOKEN });
    await authenticate(r.sockets[0]);
    await r.timers.advance(40_000);
    for (const s of r.sockets) for (const m of s.sent) expect(m).not.toContain(TOKEN);
    const r2 = await boot();
    const s = await pairWith(r2, "123456", { state: "paired", token: NEW_TOKEN });
    await until(() => r2.sockets.length === 2, "reconnect");
    await authenticate(r2.sockets[1], NEW_TOKEN, hex64(0x44));
    for (const sock of [s, r2.sockets[1]]) for (const m of sock.sent) expect(m).not.toContain(NEW_TOKEN);
  });
});

describe("E7 runtime messages", () => {
  it("from another sender id, no sender, or with extra fields or other shapes: ignored", async () => {
    const r = await boot({ token: TOKEN });
    r.sockets[0].refuse();
    await settle();
    const before = r.sockets.length;
    const stateBefore = stateOf(r);
    const writesBefore = r.writes.length;
    const bad: [unknown, unknown][] = [
      [{ t: "pair", code: "123456" }, { id: "some-other-extension" }],
      [{ t: "pair", code: "123456" }, {}],
      [{ t: "pair", code: "123456" }, undefined],
      [{ t: "retry" }, { id: "some-other-extension" }],
      [{ t: "retry" }, { id: r.id + "x" }],
      [{ t: "retry", extra: 1 }, { id: r.id }],
      [{ t: "pair", code: "123456", extra: 1 }, { id: r.id }],
      [{ t: "pair" }, { id: r.id }],
      [{ t: "pair", code: 123456 }, { id: r.id }],
      [{ t: "pair", code: "12345" }, { id: r.id }],
      [{ t: "nope" }, { id: r.id }],
      ["pair", { id: r.id }],
      [null, { id: r.id }],
      [undefined, { id: r.id }],
      [["retry"], { id: r.id }],
      [{}, { id: r.id }],
    ];
    for (const [m, sender] of bad) r.emit.message(m, sender);
    await settle();
    expect(r.sockets.length, "no socket from an ignored message").toBe(before);
    expect(stateOf(r)).toBe(stateBefore);
    expect(r.writes.length).toBe(writesBefore);
  });

  it("the same messages from the extension's own popup work", async () => {
    const r = await boot();
    r.emit.message({ t: "pair", code: "123456" }, { id: r.id });
    await until(() => r.sockets.length === 1, "pair from the own popup");
  });
});

describe("E8 full flow: nothing printed, the code never stored, the token only under `token`", () => {
  it("spies on console.* and storage", async () => {
    const spies = (["log", "info", "warn", "error", "debug", "trace"] as const).map((m) => vi.spyOn(console, m).mockImplementation(() => {}));
    const r = await boot();
    await pairWith(r, "999999", { state: "wrong_code" });
    const s0 = await pairWith(r, "123456", { state: "paired", token: NEW_TOKEN });
    expect(s0.closed).toBe(true);
    await until(() => r.sockets.length === 3, "reconnect");
    await authenticate(r.sockets[2], NEW_TOKEN, hex64(0x55));
    await r.timers.advance(20_000);
    r.sockets[2].serverClose();
    await settle();
    for (const sp of spies) expect(sp.mock.calls, "the extension prints nothing").toEqual([]);
    expect(writtenKeys(r).sort()).toEqual(["link", "token"]);
    const all = JSON.stringify(r.writes);
    expect(all).not.toContain("123456");
    expect(all).not.toContain("999999");
    for (const w of r.writes) {
      for (const [k, v] of Object.entries(w)) {
        if (JSON.stringify(v).includes(NEW_TOKEN)) expect(k, "the token may only be written under `token`").toBe("token");
        expect(JSON.stringify(v)).not.toMatch(/proof|nonce|code/);
      }
    }
    expect(r.writes.filter((w) => "token" in w).length, "the token is written once").toBe(1);
    expect(r.removes).toEqual([]);
    expect(r.clears()).toBe(0);
  });
});

describe("E9 untrusted server messages", () => {
  it("paired on a socket that never sent pair (CONNECTING) is ignored: socket closed, stored token unchanged, never saved", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await openAndHello(s, hex64(0x11));
    await until(() => s.sentOf("auth").length === 1, "auth");
    s.serverSend(status({ state: "paired", token: NEW_TOKEN }));
    await settle();
    expect(s.closed).toBe(true);
    expect(r.store.token).toBe(TOKEN);
    expect(writtenKeys(r)).not.toContain("token");
    expect(JSON.stringify(r.writes)).not.toContain(NEW_TOKEN);
    expect(stateOf(r)).toBe(S.REJECTED);
  });

  it("paired while CONNECTED is ignored too", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await authenticate(s);
    s.serverSend(status({ state: "paired", token: NEW_TOKEN }));
    await settle();
    expect(s.closed).toBe(true);
    expect(r.store.token).toBe(TOKEN);
    expect(stateOf(r)).toBe(S.REJECTED);
  });

  it("paired on the pairing socket of an OLDER attempt does not save (only the live pairing socket counts)", async () => {
    const r = await boot();
    const s0 = await pairWith(r, "123456", null);
    s0.serverClose(); // the first attempt dies without an answer
    await settle();
    const s1 = await pairWith(r, "654321", null);
    s0.serverSend(status({ state: "paired", token: NEW_TOKEN })); // late message from the dead socket: it is closed, nothing happens
    await settle();
    expect(r.store.token).toBeUndefined();
    s1.serverSend(status({ state: "paired", token: NEW_TOKEN }));
    await settle();
    expect(r.store.token, "the live pairing socket is honoured").toBe(NEW_TOKEN);
  });

  it("auth_ok on a socket that never sent auth is REJECTED", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    s.open();
    await settle();
    s.serverSend(status({ state: "auth_ok", proof: appProof(TOKEN, hex64(0x77)) })); // before hello, before our auth
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s.closed).toBe(true);
    expect(r.store.token).toBe(TOKEN);
  });

  it("auth_ok on a pairing socket (which sent pair, not auth) is REJECTED and never CONNECTED", async () => {
    const r = await boot();
    const s = await pairWith(r, "123456", { state: "auth_ok", proof: appProof(TOKEN, hex64(0x77)) });
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s.closed).toBe(true);
    expect(r.store.token).toBeUndefined();
  });

  it("rejected leaves the token unchanged", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await openAndHello(s, hex64(0x11));
    await until(() => s.sentOf("auth").length === 1, "auth");
    s.serverSend(status({ state: "rejected" }));
    await settle();
    expect(r.store.token).toBe(TOKEN);
    expect(stateOf(r)).toBe(S.REJECTED);
  });

  it("a second hello, a pong before auth_ok, or text that does not parse: REJECTED and closed", async () => {
    for (const second of [status({ state: "hello", nonce: hex64(0x99) }), JSON.stringify({ t: "pong", v: 1 }), "not json", status({ state: "nope" }), JSON.stringify({ t: "status", v: 1, state: "expired", extra: 1 })]) {
      const r = await boot({ token: TOKEN });
      const s = r.sockets[0];
      await openAndHello(s, hex64(0x11));
      await until(() => s.sentOf("auth").length === 1, "auth");
      s.serverSend(second);
      await settle();
      expect(stateOf(r), second).toBe(S.REJECTED);
      expect(s.closed, second).toBe(true);
      expect(r.store.token).toBe(TOKEN);
    }
  });

  it("a message longer than 4,096 characters is not accepted", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    s.open();
    await settle();
    s.serverSend(status({ state: "hello", nonce: hex64(0x11) }) + " ".repeat(5000));
    await settle();
    expect(stateOf(r)).toBe(S.REJECTED);
    expect(s.sentOf("auth").length).toBe(0);
  });

  it("a pong while CONNECTED is fine", async () => {
    const r = await boot({ token: TOKEN });
    const s = r.sockets[0];
    await authenticate(s);
    s.serverSend(JSON.stringify({ t: "pong", v: 1 }));
    await settle();
    expect(stateOf(r)).toBe(S.CONNECTED);
    expect(s.closed).toBe(false);
  });
});
