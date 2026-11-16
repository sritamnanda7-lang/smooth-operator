// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 E3: chrome-extension/sw.js. `chrome.storage.local.setAccessLevel({accessLevel:"TRUSTED_CONTEXTS"})` runs on every
// service-worker start (wrapped so a missing function or a synchronous throw is a rejection, never a dead worker), and every
// command waits for it. If it fails, every command answers `unsafe` and no tabs / scripting / windows call is made.
// The worker file is imported for real with the browser objects stubbed as globals.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appProof, extProof, hex64, makeBrowser, makeEnv, makeTime, openAndHello, settle, until, type Rig } from "../support/chromeFake";

const TOKEN = "ab".repeat(32);
const SW = "../../../chrome-extension/sw.js";
const status = (o: Record<string, unknown>) => JSON.stringify({ t: "status", v: 1, ...o });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "setInterval", "clearTimeout", "clearInterval"] });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.resetModules();
});

async function boot(access: "ok" | "reject" | "throw" | "missing", granted = false) {
  const r = makeEnv({ token: TOKEN, access });
  const b = makeBrowser(makeTime(), { granted });
  const seenAtFirstUse: number[] = [];
  const contains = b.chrome.permissions.contains;
  b.chrome.permissions.contains = async (q: unknown) => (seenAtFirstUse.push(r.accessCalls.length), contains(q));
  Object.assign(r.env.chrome, b.chrome);
  vi.stubGlobal("chrome", r.env.chrome);
  vi.stubGlobal("WebSocket", r.env.WebSocket);
  vi.resetModules();
  await import(/* @vite-ignore */ SW);
  await settle();
  return { r, b, seenAtFirstUse };
}
async function connectAndSend(r: Rig, cmd: Record<string, unknown>) {
  await until(() => r.sockets.length === 1, "the worker opens a socket");
  const s = r.sockets[0];
  await openAndHello(s, hex64(0x11));
  await until(() => s.sentOf("auth").length === 1, "auth");
  const a = s.sentOf("auth")[0] as { cnonce: string };
  s.serverSend(status({ state: "auth_ok", proof: appProof(TOKEN, a.cnonce) }));
  await settle();
  s.serverSend(JSON.stringify({ t: "cmd", v: 1, ...cmd }));
  await until(() => s.sentOf("res").length === 1, "the res message");
  return s.sentOf("res")[0] as { id: number; r: string };
}
const DOCS = { id: 4, op: "docs_open" };
const guarded = ["tabs.create", "tabs.update", "tabs.query", "tabs.get", "windows.update", "windows.create", "scripting.executeScript"];

describe("E3 setAccessLevel at every start", () => {
  it("is called once at start with exactly {accessLevel:'TRUSTED_CONTEXTS'} and the token is nowhere in it", async () => {
    const { r } = await boot("ok");
    expect(r.accessCalls).toEqual([{ accessLevel: "TRUSTED_CONTEXTS" }]);
    expect(JSON.stringify(r.accessCalls)).not.toContain(TOKEN);
  });
  it("is called again on each new service-worker start", async () => {
    const { r } = await boot("ok");
    vi.resetModules();
    await import(/* @vite-ignore */ SW);
    await settle();
    expect(r.accessCalls.length).toBe(2);
  });
  it("a command only reaches tabs / permissions after setAccessLevel was called", async () => {
    const { r, b, seenAtFirstUse } = await boot("ok", false);
    const res = await connectAndSend(r, DOCS);
    expect(res).toMatchObject({ id: 4, r: "no_access" });
    expect(seenAtFirstUse[0]).toBe(1);
    for (const api of guarded) expect(b.sim.count(api)).toBe(0);
  });
});

describe("E3 a failed lockdown makes every command unsafe", () => {
  for (const mode of ["reject", "throw", "missing"] as const) {
    it(`setAccessLevel ${mode}: the worker keeps running, the link connects, every command is unsafe, no browser calls`, async () => {
      const { r, b } = await boot(mode, true);
      const res = await connectAndSend(r, DOCS);
      expect(res).toMatchObject({ id: 4, r: "unsafe" });
      for (const api of [...guarded, "permissions.contains"]) expect(b.sim.count(api), api).toBe(0);
      // a second command (yt_play) is unsafe too
      const s = r.sockets[0];
      s.serverSend(JSON.stringify({ t: "cmd", v: 1, id: 5, op: "yt_play", url: "https://www.youtube.com/results?search_query=a" }));
      await until(() => s.sentOf("res").length === 2, "the second res");
      expect(s.sentOf("res")[1]).toMatchObject({ id: 5, r: "unsafe" });
      for (const api of guarded) expect(b.sim.count(api), api).toBe(0);
    });
  }
});
