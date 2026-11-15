// SPDX-License-Identifier: FSL-1.1-MIT
// Fakes for the step-5 extension tests: a `chrome` object (storage.local, alarms, runtime), a WebSocket class the
// test drives by hand, and a timer scheduler the test advances. No real Chrome, no network, no clock.
//
// THE SURFACE THE EXTENSION MAY USE (anything else is undefined in the fake, so it throws):
//   chrome.runtime.id; chrome.runtime.onStartup/onInstalled/onMessage.addListener(fn)
//   chrome.alarms.create(name, info), .clear(name), .get(name), .getAll(), .onAlarm.addListener(fn({name}))
//   chrome.storage.local.get(keys) / .set(obj) / .remove(keys) / .clear()   (all return Promises)
//   chrome.storage.onChanged.addListener(fn(changes, areaName))
// WebSocket: `new WebSocket(url)`; .readyState (0..3), .send(text) (throws unless OPEN), .close(code?), and either
//   .onopen/.onmessage/.onclose/.onerror = fn  OR  .addEventListener(type, fn). Message events are `{data: text}`.
// startLink's env = { chrome, WebSocket, crypto, setTimeout, setInterval, clearTimeout, clearInterval }.
import { createHmac } from "node:crypto";

export const tick = () => new Promise<void>((r) => setImmediate(r));
export async function settle(n = 8) {
  for (let i = 0; i < n; i++) await tick();
}
export async function until(cond: () => boolean, what: string, ms = 1500) {
  const t = Date.now();
  while (!cond()) {
    if (Date.now() - t > ms) throw new Error(`timed out waiting for: ${what}`);
    await tick();
  }
}

export const hmacHex = (tokenHex: string, msg: string) => createHmac("sha256", Buffer.from(tokenHex, "hex")).update(msg).digest("hex");
export const extProof = (tokenHex: string, nonce: string) => hmacHex(tokenHex, `so-link-v1|ext|${nonce}`);
export const appProof = (tokenHex: string, cnonce: string) => hmacHex(tokenHex, `so-link-v1|app|${cnonce}`);
export const hex64 = (n: number) => n.toString(16).padStart(2, "0").repeat(32);

type Handler = (ev: unknown) => void;
export class FakeWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  readonly CONNECTING = 0;
  readonly OPEN = 1;
  readonly CLOSING = 2;
  readonly CLOSED = 3;
  readyState = 0;
  sent: string[] = [];
  closeCalls: { code?: number; reason?: string }[] = [];
  onopen: Handler | null = null;
  onmessage: Handler | null = null;
  onclose: Handler | null = null;
  onerror: Handler | null = null;
  private listeners: Record<string, Handler[]> = {};
  constructor(public url: string) {}
  addEventListener(type: string, fn: Handler) {
    (this.listeners[type] ||= []).push(fn);
  }
  removeEventListener(type: string, fn: Handler) {
    this.listeners[type] = (this.listeners[type] || []).filter((x) => x !== fn);
  }
  private fire(type: "open" | "message" | "close" | "error", ev: unknown) {
    const h = this[`on${type}` as "onopen"];
    if (h) h.call(this, ev);
    for (const l of this.listeners[type] || []) l.call(this, ev);
  }
  send(data: string) {
    if (this.readyState !== 1) throw new Error("InvalidStateError: socket is not open");
    this.sent.push(String(data));
  }
  close(code?: number, reason?: string) {
    if (this.readyState >= 2) return;
    this.closeCalls.push({ code, reason });
    this.readyState = 2;
    queueMicrotask(() => {
      this.readyState = 3;
      this.fire("close", { code: code ?? 1000, wasClean: true });
    });
  }
  // ---- test controls ----
  open() {
    this.readyState = 1;
    this.fire("open", {});
  }
  serverSend(text: string) {
    if (this.readyState !== 1) return;
    this.fire("message", { data: text });
  }
  serverClose(code = 1006) {
    this.readyState = 3;
    this.fire("close", { code, wasClean: false });
  }
  /** Connection refused: the app is not running. */
  refuse() {
    this.readyState = 3;
    this.fire("error", {});
    this.fire("close", { code: 1006, wasClean: false });
  }
  get closed() {
    return this.readyState >= 2;
  }
  /** JSON of every message the extension sent. */
  get sentJson(): Record<string, unknown>[] {
    return this.sent.map((s) => JSON.parse(s));
  }
  sentOf(t: string) {
    return this.sentJson.filter((m) => m.t === t);
  }
}

export function makeEnv(opts: { token?: string; id?: string } = {}) {
  const id = opts.id ?? "extid_abcdefghijklmnop";
  const store: Record<string, unknown> = {};
  if (opts.token) store.token = opts.token;
  const writes: Record<string, unknown>[] = [];
  const removes: unknown[] = [];
  let clears = 0;
  const alarmsCreated: { name: string; info: Record<string, unknown> }[] = [];
  const alarmsCleared: string[] = [];
  const onStartup: Handler[] = [];
  const onInstalled: Handler[] = [];
  const onMessage: ((m: unknown, s: unknown, r: (x?: unknown) => void) => unknown)[] = [];
  const onAlarm: ((a: { name: string }) => unknown)[] = [];
  const onChanged: ((c: Record<string, { oldValue?: unknown; newValue?: unknown }>, area: string) => void)[] = [];

  const pick = (keys: unknown) => {
    if (keys == null) return { ...store };
    if (typeof keys === "string") return keys in store ? { [keys]: store[keys] } : {};
    if (Array.isArray(keys)) return Object.fromEntries(keys.filter((k) => k in store).map((k) => [k, store[k]]));
    return Object.fromEntries(Object.entries(keys as object).map(([k, d]) => [k, k in store ? store[k] : d]));
  };
  const chrome = {
    runtime: {
      id,
      onStartup: { addListener: (f: Handler) => void onStartup.push(f) },
      onInstalled: { addListener: (f: Handler) => void onInstalled.push(f) },
      onMessage: { addListener: (f: (typeof onMessage)[number]) => void onMessage.push(f) },
    },
    alarms: {
      create: (name: string, info: Record<string, unknown>) => void alarmsCreated.push({ name, info }),
      clear: async (name: string) => void alarmsCleared.push(name),
      get: async (name: string) => alarmsCreated.filter((a) => a.name === name && !alarmsCleared.includes(name)).at(-1) ?? undefined,
      getAll: async () => alarmsCreated.filter((a) => !alarmsCleared.includes(a.name)),
      onAlarm: { addListener: (f: (typeof onAlarm)[number]) => void onAlarm.push(f) },
    },
    storage: {
      onChanged: { addListener: (f: (typeof onChanged)[number]) => void onChanged.push(f) },
      local: {
        get: async (keys?: unknown) => pick(keys),
        set: async (obj: Record<string, unknown>) => {
          writes.push(JSON.parse(JSON.stringify(obj)));
          const ch: Record<string, { oldValue?: unknown; newValue?: unknown }> = {};
          for (const [k, v] of Object.entries(obj)) {
            ch[k] = { oldValue: store[k], newValue: v };
            store[k] = JSON.parse(JSON.stringify(v));
          }
          onChanged.forEach((f) => f(ch, "local"));
        },
        remove: async (keys: unknown) => {
          removes.push(keys);
          for (const k of Array.isArray(keys) ? keys : [keys]) delete store[k as string];
        },
        clear: async () => {
          clears++;
          for (const k of Object.keys(store)) delete store[k];
        },
      },
    },
  };

  // ---- timers: advanced by hand ----
  let nowMs = 0;
  let nextId = 1;
  const list: { id: number; at: number; ms: number; fn: () => unknown; every: boolean }[] = [];
  const timers = {
    setTimeout: (fn: () => unknown, ms = 0) => {
      list.push({ id: nextId, at: nowMs + ms, ms, fn, every: false });
      return nextId++;
    },
    setInterval: (fn: () => unknown, ms = 0) => {
      list.push({ id: nextId, at: nowMs + ms, ms, fn, every: true });
      return nextId++;
    },
    clear: (id: number) => {
      const i = list.findIndex((t) => t.id === id);
      if (i >= 0) list.splice(i, 1);
    },
    /** Pending timers: delay in ms and whether it repeats. */
    pending: () => list.map((t) => ({ ms: t.ms, every: t.every })),
    async advance(ms: number) {
      const end = nowMs + ms;
      for (;;) {
        const due = list.filter((t) => t.at <= end).sort((a, b) => a.at - b.at || a.id - b.id)[0];
        if (!due) break;
        nowMs = due.at;
        if (due.every) due.at += due.ms;
        else list.splice(list.indexOf(due), 1);
        await due.fn();
        await settle(4);
      }
      nowMs = end;
      await settle(4);
    },
  };

  const sockets: FakeWebSocket[] = [];
  class TestSocket extends FakeWebSocket {
    constructor(url: string) {
      super(url);
      sockets.push(this);
    }
  }
  const env = {
    chrome,
    WebSocket: TestSocket as unknown as typeof WebSocket,
    crypto: globalThis.crypto,
    setTimeout: timers.setTimeout,
    setInterval: timers.setInterval,
    clearTimeout: timers.clear,
    clearInterval: timers.clear,
  };
  const emit = {
    startup: () => onStartup.forEach((f) => f({})),
    installed: () => onInstalled.forEach((f) => f({ reason: "install" })),
    alarm: (name: string) => onAlarm.forEach((f) => f({ name })),
    message: (msg: unknown, sender: unknown) => onMessage.map((f) => f(msg, sender, () => {})),
    /** A message from the extension's own popup. */
    popup: (msg: unknown) => onMessage.map((f) => f(msg, { id }, () => {})),
  };
  return { env, id, store, writes, removes, clears: () => clears, alarmsCreated, alarmsCleared, sockets, timers, emit };
}
export type Rig = ReturnType<typeof makeEnv>;

/** The keys written to storage, over all writes. */
export const writtenKeys = (r: Rig) => [...new Set(r.writes.flatMap((w) => Object.keys(w)))];

/** Opens `s`, sends the app's hello and returns the nonce the app "chose". */
export async function openAndHello(s: FakeWebSocket, nonce: string) {
  s.open();
  await settle();
  s.serverSend(JSON.stringify({ t: "status", v: 1, state: "hello", nonce }));
  await settle();
}
