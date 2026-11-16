// SPDX-License-Identifier: FSL-1.1-MIT
// Fakes for the step-5 extension tests: a `chrome` object (storage.local, alarms, runtime), a WebSocket class the
// test drives by hand, and a timer scheduler the test advances. No real Chrome, no network, no clock.
//
// THE SURFACE THE EXTENSION MAY USE (anything else is undefined in the fake, so it throws):
//   chrome.runtime.id; chrome.runtime.onStartup/onInstalled/onMessage.addListener(fn)
//   chrome.alarms.create(name, info), .clear(name), .get(name), .getAll(), .onAlarm.addListener(fn({name}))
//   chrome.storage.local.get(keys) / .set(obj) / .remove(keys) / .clear()   (all return Promises)
//   chrome.storage.onChanged.addListener(fn(changes, areaName))
//   chrome.runtime.getURL(path)
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

export function makeEnv(opts: { token?: string; id?: string; access?: "ok" | "reject" | "throw" | "missing" } = {}) {
  const accessCalls: unknown[] = [];
  const id = opts.id ?? "extid_abcdefghijklmnop";
  const store: Record<string, unknown> = {};
  if (opts.token) store.token = opts.token;
  const writes: Record<string, unknown>[] = [];
  const removes: unknown[] = [];
  let clears = 0;
  let setDelay = 0; // storage.local.set applies after this many event-loop ticks (a slow storage)
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
      getURL: (path: string) => `chrome-extension://${id}/${path}`,
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
        ...(opts.access === "missing"
          ? {}
          : {
              // step 6: only the extension's own pages may read storage.local (call shape: one options object).
              setAccessLevel: (o: unknown) => {
                accessCalls.push(o);
                if (opts.access === "throw") throw new Error("setAccessLevel threw synchronously");
                return opts.access === "reject" ? Promise.reject(new Error("setAccessLevel rejected")) : Promise.resolve();
              },
            }),
        get: async (keys?: unknown) => pick(keys),
        set: async (obj: Record<string, unknown>) => {
          for (let i = 0; i < setDelay; i++) await tick();
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
    popup: (msg: unknown) => onMessage.map((f) => f(msg, { id, url: `chrome-extension://${id}/popup.html` }, () => {})),
  };
  return { env, id, accessCalls, setSetDelay: (n: number) => void (setDelay = n), store, writes, removes, clears: () => clears, alarmsCreated, alarmsCleared, sockets, timers, emit };
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

// ======================= step 6: time, tabs, windows, scripting, permissions =======================
/** A clock the test advances by hand; `sleep` resolves when advanced past it. */
export function makeTime() {
  let t = 0;
  const sleepers: { at: number; res: () => void }[] = [];
  return {
    now: () => t,
    /** Sleepers still waiting (0 when the code under test left no timer behind). */
    pending: () => sleepers.length,
    sleep: (ms: number) => new Promise<void>((res) => void sleepers.push({ at: t + ms, res })),
    async advance(ms: number) {
      const end = t + ms;
      for (;;) {
        const due = sleepers.filter((s) => s.at <= end).sort((a, b) => a.at - b.at)[0];
        if (!due) break;
        sleepers.splice(sleepers.indexOf(due), 1);
        t = due.at;
        due.res();
        await settle(6);
      }
      t = end;
      await settle(6);
    },
  };
}
export type Time = ReturnType<typeof makeTime>;

export type Tab = { id: number; windowId: number; url?: string; status: "loading" | "complete"; lastAccessed?: number };
export type Call = { api: string; args: unknown; at: number };
export type Nav = { delay: number; url: string | undefined };

/**
 * A scriptable `chrome` for the page jobs. Every call is logged in `calls` (api name, args, fake time).
 * `nav(url)` says what a navigation ends as (default: the same URL after 300 ms). `script[fn]` answers
 * `executeScript` by the injected function's name ("ytScan", "ytPlayer", ...). Anything not listed here is
 * undefined (so a call to it throws): no `tabs.remove`, no `tabs.executeScript`, no `windows.create` unless listed.
 */
export function makeBrowser(time: Time, o: { granted?: boolean; tabs?: Tab[] } = {}) {
  const calls: Call[] = [];
  const log = (api: string, args: unknown) => void calls.push({ api, args: JSON.parse(JSON.stringify(args ?? null)), at: time.now() });
  const tabs = new Map<number, Tab>((o.tabs ?? []).map((t) => [t.id, { ...t }]));
  let nextId = 100;
  const sim = {
    calls,
    tabs,
    granted: o.granted ?? true,
    getFails: false,
    nav: (url: string): Nav => ({ delay: 300, url }),
    script: {} as Record<string, (c: { tab: Tab; args: unknown[]; now: number }) => unknown>,
    hangApis: new Set<string>(), // chrome APIs (e.g. "tabs.get") whose promise never settles
    hang: new Set<string>(), // injected function names whose executeScript never settles
    navDone: [] as { tabId: number; url: string | undefined; at: number }[],
    count: (api: string) => calls.filter((c) => c.api === api).length,
    of: (api: string) => calls.filter((c) => c.api === api).map((c) => c.args as Record<string, any>),
    scripts: (fn: string) => calls.filter((c) => c.api === "scripting.executeScript" && (c.args as any).fn === fn),
  };
  const navigate = (tab: Tab, url: string) => {
    const n = sim.nav(url);
    tab.status = "loading";
    void time.sleep(n.delay).then(() => {
      tab.url = n.url;
      tab.status = "complete";
      sim.navDone.push({ tabId: tab.id, url: n.url, at: time.now() });
    });
  };
  const chrome = {
    permissions: { contains: async (q: unknown) => (log("permissions.contains", q), sim.granted) },
    tabs: {
      query: async (q: { url?: string }) => {
        log("tabs.query", q);
        const prefix = (q.url ?? "").replace(/\*$/, "");
        return [...tabs.values()].filter((t) => t.url !== undefined && t.url.startsWith(prefix)).map((t) => ({ ...t }));
      },
      create: async (p: { url: string; active?: boolean }) => {
        log("tabs.create", p);
        const tab: Tab = { id: nextId++, windowId: 1, url: "", status: "loading" };
        tabs.set(tab.id, tab);
        navigate(tab, p.url);
        return { ...tab };
      },
      update: async (id: number, p: { url?: string; active?: boolean }) => {
        log("tabs.update", { id, ...p });
        const tab = tabs.get(id);
        if (!tab) throw new Error("No tab with id");
        if (p.url) navigate(tab, p.url);
        return { ...tab };
      },
      get: async (id: number) => {
        log("tabs.get", { id });
        if (sim.hangApis.has("tabs.get")) return new Promise<never>(() => {});
        const tab = tabs.get(id);
        if (!tab || sim.getFails) throw new Error("No tab with id: " + id);
        return { ...tab };
      },
    },
    windows: {
      update: async (id: number, p: unknown) => (log("windows.update", { id, p }), {}),
    },
    scripting: {
      executeScript: async (inj: { target: { tabId: number }; func: (...a: any[]) => unknown; args?: unknown[] }) => {
        const tab = tabs.get(inj.target.tabId);
        log("scripting.executeScript", { tabId: inj.target.tabId, fn: inj.func.name, args: inj.args ?? [], keys: Object.keys(inj).sort() });
        if (!tab) throw new Error("No tab with id");
        if (sim.hang.has(inj.func.name)) return new Promise<never>(() => {});
        const impl = sim.script[inj.func.name];
        return [{ result: impl ? impl({ tab, args: inj.args ?? [], now: time.now() }) : undefined }];
      },
    },
  };
  return { chrome, sim };
}
export type Browser = ReturnType<typeof makeBrowser>;
