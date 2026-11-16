// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 E6 (yt_play), E7 (docs_open), E8 (cal_open), E9 (busy / cancel), E12 (no console), E15 (no_access) and the
// `ready` gate of E3: chrome-extension/lib/tasks.js `makeTasks({chrome, ready, now, sleep})`. A scripted browser
// (tests/so/support/chromeFake.ts `makeBrowser`) and a hand-advanced clock; the page functions are answered by name.
import { afterEach, describe, expect, it, vi } from "vitest";
import { makeTasks } from "../../../chrome-extension/lib/tasks.js";
import { SEL } from "../../../chrome-extension/lib/page.js";
import * as config from "../../../chrome-extension/lib/config.js";
import { makeBrowser, makeTime, settle, type Tab } from "../support/chromeFake";

afterEach(() => vi.restoreAllMocks());

const ID = "AAAAAAAAAAA";
const YT = "https://www.youtube.com/results?search_query=Blinding+Lights";
const WATCH = `https://www.youtube.com/watch?v=${ID}`;
const DOCS_EDIT = "https://docs.google.com/document/d/abc_123-x/edit";
const CAL = "https://calendar.google.com/calendar/render?action=TEMPLATE&text=Review&dates=20261116T093000Z/20261116T103000Z";
const CAL_EDIT = "https://calendar.google.com/calendar/u/0/r/eventedit?text=Review";
const ytCmd = (id = 1) => ({ id, op: "yt_play", url: YT, query: "Blinding Lights" });
const docsCmd = (id = 1) => ({ id, op: "docs_open" });
const calCmd = (id = 1) => ({ id, op: "cal_open", url: CAL });

type Out = { r: string; ms: number };
function rig(o: { granted?: boolean; tabs?: Tab[]; ready?: Promise<unknown> } = {}) {
  const time = makeTime();
  const b = makeBrowser(time, o);
  const tasks = makeTasks({ chrome: b.chrome, ready: o.ready ?? Promise.resolve(), now: time.now, sleep: time.sleep });
  const start = (cmd: any) => {
    const box: { out?: Out } = {};
    const p = tasks.run(cmd).then((v: Out) => (box.out = v));
    return { box, p };
  };
  const run = async (cmd: any, ms = 40_000) => {
    const j = start(cmd);
    await time.advance(ms);
    await j.p;
    return j.box.out as Out;
  };
  return { time, b, sim: b.sim, tasks, start, run };
}
/** A healthy YouTube: results show the video once loaded, the player plays with rising time. */
function healthy(r: ReturnType<typeof rig>) {
  r.sim.script.ytScan = ({ tab }) => (tab.url === YT ? { s: "found", id: ID } : { s: "wait" });
  r.sim.script.ytPlayer = ({ tab, now }) => (tab.url === WATCH ? { s: "playing", t: now / 1000 } : { s: "none", t: 0 });
}
const apis = (r: ReturnType<typeof rig>) => r.sim.calls.map((c) => c.api);
const guarded = ["tabs.create", "tabs.update", "tabs.query", "tabs.get", "windows.update", "scripting.executeScript"];

describe("E6 yt_play happy path", () => {
  it("new tab, foreground; scans only after the tab shows the sent URL; watch URL exactly; playing", async () => {
    const r = rig();
    healthy(r);
    r.sim.nav = (url) => ({ delay: url === YT ? 1500 : 800, url });
    const out = await r.run(ytCmd());
    expect(out.r).toBe("playing");
    expect(Number.isInteger(out.ms) && out.ms >= 0 && out.ms <= 120_000).toBe(true);
    expect(r.sim.count("tabs.create")).toBe(1);
    expect(r.sim.of("tabs.create")[0]).toMatchObject({ url: YT, active: true });
    expect(r.sim.count("windows.update")).toBeGreaterThanOrEqual(1);
    expect(r.sim.of("windows.update")[0]).toMatchObject({ p: { focused: true } });
    expect(r.sim.count("windows.create"), "never windows.create: with no window the service worker does not run").toBe(0);
    const upd = r.sim.of("tabs.update").map((u) => u.url);
    expect(upd).toEqual([WATCH]);
    // stale-document guards: no scan before the results URL was reached, no player poll before the watch URL
    const done = (url: string) => r.sim.navDone.find((n) => n.url === url)!.at;
    const scans = r.sim.scripts("ytScan");
    const plays = r.sim.scripts("ytPlayer");
    expect(scans.length).toBeGreaterThanOrEqual(1);
    expect(plays.length).toBeGreaterThanOrEqual(2);
    expect(Math.min(...scans.map((c) => c.at))).toBeGreaterThanOrEqual(done(YT));
    expect(Math.min(...plays.map((c) => c.at))).toBeGreaterThanOrEqual(done(WATCH));
    // arguments: SEL and what the page must be showing
    expect((scans[0].args as any).args).toEqual([SEL, "Blinding Lights"]);
    expect((plays[0].args as any).args).toEqual([SEL, ID]);
    for (const c of [...scans, ...plays]) expect((c.args as any).keys).toEqual(["args", "func", "target"]);
  });
  it("first a permission check for the youtube host, before any tab call", async () => {
    const r = rig();
    healthy(r);
    await r.run(ytCmd());
    expect(apis(r)[0]).toBe("permissions.contains");
    expect(JSON.stringify(r.sim.of("permissions.contains")[0])).toContain("youtube.com");
  });
  it("reuses the YouTube tab used last: tabs.update with the sent URL, no tabs.create", async () => {
    const r = rig({
      tabs: [
        { id: 5, windowId: 1, url: "https://www.youtube.com/watch?v=OLDOLDOLD11", status: "complete", lastAccessed: 100 },
        { id: 6, windowId: 1, url: "https://www.youtube.com/", status: "complete", lastAccessed: 500 },
      ],
    });
    healthy(r);
    const out = await r.run(ytCmd());
    expect(out.r).toBe("playing");
    expect(r.sim.count("tabs.create")).toBe(0);
    expect(r.sim.count("tabs.query")).toBeGreaterThanOrEqual(1);
    expect(JSON.stringify(r.sim.of("tabs.query")[0])).toContain("youtube.com");
    const first = r.sim.of("tabs.update")[0];
    expect(first).toMatchObject({ id: 6, url: YT });
  });
  it("a reused tab still showing the OLD watch page never reports playing from it", async () => {
    const old = "https://www.youtube.com/watch?v=OLDOLDOLD11";
    const r = rig({ tabs: [{ id: 5, windowId: 1, url: old, status: "complete", lastAccessed: 100 }] });
    // The old page keeps playing; the navigation to the results never happens (tab.url stays old).
    r.sim.nav = () => ({ delay: 100, url: old });
    r.sim.script.ytScan = () => ({ s: "wait" });
    r.sim.script.ytPlayer = ({ now }) => ({ s: "playing", t: now / 1000 });
    const out = await r.run(ytCmd());
    expect(out.r).not.toBe("playing");
    expect(r.sim.scripts("ytPlayer").length).toBe(0);
    expect(r.sim.scripts("ytScan").length, "no scan of a page that is not the sent URL").toBe(0);
  });
});

describe("E6 stale-document guard compares origin + pathname + the one expected param", () => {
  const OLD = "https://www.youtube.com/watch?v=OLDOLDOLD11";
  /** `nav` ends the results navigation (and the watch navigation) at the given URLs; returns what happened. */
  async function runWith(resultsUrl: string, watchUrl: string = WATCH) {
    const r = rig();
    r.sim.nav = (u) => ({ delay: 300, url: u === YT ? resultsUrl : u === WATCH ? watchUrl : u });
    r.sim.script.ytScan = () => ({ s: "found", id: ID });
    r.sim.script.ytPlayer = ({ now }) => ({ s: "playing", t: now / 1000 });
    const out = await r.run(ytCmd(), 60_000);
    return { r, out };
  }
  it("extra params, other encodings of the same value and the parser's own normalisation still match the results page", async () => {
    for (const u of [`${YT}&sp=EgIQAQ%3D%3D`, "https://www.youtube.com/results?search_query=Blinding%20Lights", "https://www.youtube.com/results?pp=x&search_query=Blinding+Lights", "https://www.youtube.com:443/results?search_query=Blinding+Lights", "https://WWW.YOUTUBE.COM/results?search_query=Blinding+Lights"]) {
      const { r } = await runWith(u);
      expect(r.sim.scripts("ytScan").length, u).toBeGreaterThanOrEqual(1);
    }
  });
  it("a different path, host, scheme, query value or key, case of the path, or the old document never match", async () => {
    for (const u of [
      "https://www.youtube.com/results/?search_query=Blinding+Lights",
      "https://www.youtube.com/Results?search_query=Blinding+Lights",
      "https://www.youtube.com/watch?search_query=Blinding+Lights",
      "https://m.youtube.com/results?search_query=Blinding+Lights",
      "http://www.youtube.com/results?search_query=Blinding+Lights",
      "https://www.youtube.com/results?search_query=Other+Song",
      "https://www.youtube.com/results?q=Blinding+Lights",
      "https://www.youtube.com/results",
      OLD,
    ]) {
      const { r, out } = await runWith(u);
      expect(r.sim.scripts("ytScan").length, `${u} must not be scanned`).toBe(0);
      expect(out.r, u).not.toBe("playing");
    }
  });
  it("the watch page: extra params match; another id, path, host or the results page do not", async () => {
    for (const w of [`${WATCH}&t=5s`, `${WATCH}&pp=abc`, `https://www.youtube.com/watch?pp=abc&v=${ID}`]) {
      const { r, out } = await runWith(YT, w);
      expect(r.sim.scripts("ytPlayer").length, w).toBeGreaterThanOrEqual(1);
      expect(out.r, w).toBe("playing");
    }
    for (const w of ["https://www.youtube.com/watch?v=BBBBBBBBBBB", `https://www.youtube.com/shorts/${ID}`, `https://m.youtube.com/watch?v=${ID}`, `https://www.youtube.com/watch?x=${ID}`, `https://www.youtube.com/watch/?v=${ID}`, YT, OLD]) {
      const { r, out } = await runWith(YT, w);
      expect(r.sim.scripts("ytPlayer").length, `${w} must not be polled`).toBe(0);
      expect(out.r, w).not.toBe("playing");
    }
  });
});

describe("E6 empty results are no_video only after about 2 s of continuous emptiness", () => {
  it("none from the start: no_video, not before 2 s (time-based, not poll-count based)", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytScan = ({ tab }) => (tab.url === YT ? { s: "none" } : { s: "wait" });
    const out = await r.run(ytCmd(), 60_000);
    expect(out.r).toBe("no_video");
    expect(out.ms).toBeGreaterThanOrEqual(2000);
    expect(out.ms).toBeLessThanOrEqual(5000);
    const scans = r.sim.scripts("ytScan");
    const first = Math.min(...scans.map((c) => c.at));
    expect(Math.max(...scans.map((c) => c.at)) - first, "continuous emptiness lasted about 2 s").toBeGreaterThanOrEqual(1800);
  });
  it("a result that appears at 1.5 s is found and plays", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytScan = ({ tab, now }) => (tab.url !== YT ? { s: "wait" } : now < 1500 ? { s: "none" } : { s: "found", id: ID });
    expect((await r.run(ytCmd(), 60_000)).r).toBe("playing");
  });
  it("a wait in between resets the emptiness (the page re-rendered): still found later", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytScan = ({ tab, now }) => (tab.url !== YT ? { s: "wait" } : now < 1000 ? { s: "none" } : now < 1400 ? { s: "wait" } : now < 3200 ? { s: "none" } : { s: "found", id: ID });
    expect((await r.run(ytCmd(), 60_000)).r).toBe("playing");
  });
});

describe("E6 yt_play outcomes", () => {
  it("an ad first, then the video: playing", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytPlayer = ({ tab, now }) => (tab.url !== WATCH ? { s: "none", t: 0 } : now < 5000 ? { s: "ad", t: 0 } : { s: "playing", t: now / 1000 });
    expect((await r.run(ytCmd())).r).toBe("playing");
  });
  it("an ad that never ends: ad, after the ad wait", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytPlayer = ({ tab }) => (tab.url === WATCH ? { s: "ad", t: 0 } : { s: "none", t: 0 });
    const out = await r.run(ytCmd(), 60_000);
    expect(out.r).toBe("ad");
    expect(out.ms).toBeGreaterThanOrEqual(10_000);
  });
  it("paused for the whole wait: paused; play() is tried only if TRY_PLAY", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytPlayer = ({ tab }) => (tab.url === WATCH ? { s: "paused", t: 0 } : { s: "none", t: 0 });
    r.sim.script.ytPlay = () => "played";
    const out = await r.run(ytCmd(), 60_000);
    expect(out.r).toBe("paused");
    const others = r.sim.calls.filter((c) => c.api === "scripting.executeScript" && !["ytScan", "ytPlayer"].includes((c.args as any).fn));
    expect((config as any).TRY_PLAY, "default TRY_PLAY is false (spike Z1)").toBe(false);
    expect(others.length).toBe(0);
    expect(r.sim.scripts("ytPlay").length).toBe(0);
  });
  it("TRY_PLAY true: ytPlay is injected once, after PLAY_WAIT of paused, with args exactly [SEL, id]; the result follows the player", async () => {
    vi.resetModules();
    vi.doMock("../../../chrome-extension/lib/config.js", async (orig) => ({ ...(await orig<object>()), TRY_PLAY: true }));
    try {
      const { makeTasks: mk } = await import("../../../chrome-extension/lib/tasks.js");
      const { SEL: sel } = await import("../../../chrome-extension/lib/page.js");
      for (const [mode, want] of [["stays_paused", "paused"], ["plays_after", "playing"], ["playing", "playing"]] as const) {
        const time = makeTime();
        const b = makeBrowser(time);
        const tasks = mk({ chrome: b.chrome, ready: Promise.resolve(), now: time.now, sleep: time.sleep });
        let played = false;
        b.sim.script.ytScan = ({ tab }: any) => (tab.url === YT ? { s: "found", id: ID } : { s: "wait" });
        b.sim.script.ytPlayer = ({ tab, now }: any) =>
          tab.url !== WATCH ? { s: "none", t: 0 } : mode === "playing" || (mode === "plays_after" && played) ? { s: "playing", t: now / 1000 } : { s: "paused", t: 0 };
        b.sim.script.ytPlay = () => ((played = true), "played");
        const p = tasks.run(ytCmd());
        let out: Out | undefined;
        void p.then((v: Out) => (out = v));
        await time.advance(60_000);
        await p;
        const plays = b.sim.scripts("ytPlay");
        expect(out?.r, mode).toBe(want);
        if (mode === "playing") {
          expect(plays.length, "playing needs no play()").toBe(0);
          continue;
        }
        expect(plays.length, mode).toBe(1);
        expect((plays[0].args as any).args).toEqual([sel, ID]);
        expect((plays[0].args as any).keys).toEqual(["args", "func", "target"]);
        const firstPoll = Math.min(...b.sim.scripts("ytPlayer").map((c) => c.at));
        expect(plays[0].at - firstPoll, "play() only after PLAY_WAIT").toBeGreaterThanOrEqual(6000);
      }
    } finally {
      vi.doUnmock("../../../chrome-extension/lib/config.js");
      vi.resetModules();
    }
  });
  it("TRY_PLAY true and the ytPlay injection hangs: the job still ends paused, and cancel gives cancelled within one poll", async () => {
    vi.resetModules();
    vi.doMock("../../../chrome-extension/lib/config.js", async (orig) => ({ ...(await orig<object>()), TRY_PLAY: true }));
    try {
      const { makeTasks: mk } = await import("../../../chrome-extension/lib/tasks.js");
      const setup = () => {
        const time = makeTime();
        const b = makeBrowser(time);
        const tasks = mk({ chrome: b.chrome, ready: Promise.resolve(), now: time.now, sleep: time.sleep });
        b.sim.script.ytScan = ({ tab }: any) => (tab.url === YT ? { s: "found", id: ID } : { s: "wait" });
        b.sim.script.ytPlayer = ({ tab }: any) => (tab.url === WATCH ? { s: "paused", t: 0 } : { s: "none", t: 0 });
        b.sim.hang.add("ytPlay");
        return { time, b, tasks };
      };
      const a = setup();
      const p = a.tasks.run(ytCmd());
      let out: Out | undefined;
      void p.then((v: Out) => (out = v));
      await a.time.advance(60_000);
      await p;
      expect(out?.r).toBe("paused");
      const c = setup();
      const p2 = c.tasks.run(ytCmd(9));
      let out2: Out | undefined;
      void p2.then((v: Out) => (out2 = v));
      for (let i = 0; i < 80 && c.b.sim.scripts("ytPlay").length === 0; i++) await c.time.advance(250);
      expect(c.b.sim.scripts("ytPlay").length, "the hanging ytPlay was injected").toBe(1);
      c.tasks.cancel(9);
      await c.time.advance(500);
      await p2;
      expect(out2?.r).toBe("cancelled");
    } finally {
      vi.doUnmock("../../../chrome-extension/lib/config.js");
      vi.resetModules();
    }
  });
  it("a playing flag with a frozen clock (stalled) is not playing", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytPlayer = ({ tab }) => (tab.url === WATCH ? { s: "playing", t: 3 } : { s: "none", t: 0 });
    const out = await r.run(ytCmd(), 60_000);
    expect(out.r).not.toBe("playing");
  });
  it("the page never shows a video: timeout", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytPlayer = () => ({ s: "none", t: 0 });
    expect((await r.run(ytCmd(), 60_000)).r).toBe("timeout");
  });
  it("none found: no_video; consent: consent; never ready: timeout", async () => {
    for (const [scan, want] of [[{ s: "none" }, "no_video"], [{ s: "consent" }, "consent"], [{ s: "wait" }, "timeout"]] as const) {
      const r = rig();
      healthy(r);
      r.sim.script.ytScan = ({ tab }) => (tab.url === YT ? scan : { s: "wait" });
      const out = await r.run(ytCmd(), 60_000);
      expect(out.r, JSON.stringify(scan)).toBe(want);
      expect(r.sim.count("tabs.update"), "no watch navigation").toBe(0);
    }
  });
  it("a scan result with a bad id is never navigated to", async () => {
    for (const id of ["short", "AAAAAAAAAAAA", "AAAA AAAAAAA", "../../etc/pw", "AAAAAAAAAA!"]) {
      const r = rig();
      healthy(r);
      r.sim.script.ytScan = ({ tab }) => (tab.url === YT ? { s: "found", id } : { s: "wait" });
      const out = await r.run(ytCmd(), 60_000);
      expect(out.r, id).not.toBe("playing");
      expect(r.sim.of("tabs.update").filter((u) => String(u.url).includes("watch")), id).toEqual([]);
    }
  });
  it("Google needs the user: tab complete, URL unreadable twice: needs_login", async () => {
    const r = rig();
    healthy(r);
    r.sim.nav = () => ({ delay: 400, url: undefined });
    const out = await r.run(ytCmd(), 60_000);
    expect(out.r).toBe("needs_login");
    expect(r.sim.scripts("ytScan").length).toBe(0);
  });
  it("tabs.get rejecting is error", async () => {
    const r = rig();
    healthy(r);
    r.sim.getFails = true;
    expect((await r.run(ytCmd(), 60_000)).r).toBe("error");
  });
});

describe("E7 docs_open", () => {
  it("one tabs.create of docs.new in the foreground, editor_open when the doc URL shows, no injection", async () => {
    const r = rig();
    r.sim.nav = () => ({ delay: 1500, url: DOCS_EDIT });
    const out = await r.run(docsCmd());
    expect(out.r).toBe("editor_open");
    expect(r.sim.of("tabs.create")).toEqual([{ url: "https://docs.new/", active: true }]);
    expect(r.sim.count("scripting.executeScript")).toBe(0);
    expect(r.sim.count("windows.update")).toBeGreaterThanOrEqual(1);
    expect(JSON.stringify(r.sim.of("permissions.contains")[0])).toContain("docs.google.com");
    for (const u of ["https://docs.google.com/document/u/0/d/abc/edit", "https://docs.google.com/document/d/abc/edit?usp=x"]) {
      const r2 = rig();
      r2.sim.nav = () => ({ delay: 500, url: u });
      expect((await r2.run(docsCmd())).r, u).toBe("editor_open");
    }
  });
  it("needs_login when the URL is unreadable; timeout when it never reaches an editor; never created twice", async () => {
    const r = rig();
    r.sim.nav = () => ({ delay: 400, url: undefined });
    expect((await r.run(docsCmd(), 30_000)).r).toBe("needs_login");
    expect(r.sim.count("tabs.create")).toBe(1);
    const r2 = rig();
    r2.sim.nav = () => ({ delay: 400, url: "https://docs.google.com/document/u/0/create" });
    const out = await r2.run(docsCmd(), 40_000);
    expect(out.r).toBe("timeout");
    expect(out.ms).toBeGreaterThanOrEqual(14_000);
    expect(r2.sim.count("tabs.create"), "never re-created inside the extension").toBe(1);
    expect(r2.sim.count("scripting.executeScript")).toBe(0);
  });
  it("tabs.get rejecting is error", async () => {
    const r = rig();
    r.sim.getFails = true;
    expect((await r.run(docsCmd(), 30_000)).r).toBe("error");
  });
});

describe("E8 cal_open", () => {
  it("tabs.create with the exact URL, foreground; editor_open on the event editor; no injection", async () => {
    const r = rig();
    r.sim.nav = () => ({ delay: 1200, url: CAL_EDIT });
    const out = await r.run(calCmd());
    expect(out.r).toBe("editor_open");
    expect(r.sim.of("tabs.create")).toEqual([{ url: CAL, active: true }]);
    expect(r.sim.count("scripting.executeScript")).toBe(0);
    expect(JSON.stringify(r.sim.of("permissions.contains")[0])).toContain("calendar.google.com");
  });
  it("needs_login and timeout", async () => {
    const r = rig();
    r.sim.nav = () => ({ delay: 400, url: undefined });
    expect((await r.run(calCmd(), 30_000)).r).toBe("needs_login");
    const r2 = rig();
    r2.sim.nav = () => ({ delay: 400, url: "https://calendar.google.com/calendar/u/0/r" });
    expect((await r2.run(calCmd(), 40_000)).r).toBe("timeout");
  });
});

describe("E9 one job at a time, cancel", () => {
  it("a second command while one runs answers busy at once and touches nothing", async () => {
    const r = rig();
    healthy(r);
    const a = r.start(ytCmd(1));
    await settle();
    const before = r.sim.calls.length;
    const b = r.start(docsCmd(2));
    await settle();
    expect(b.box.out?.r).toBe("busy");
    expect(r.sim.calls.length).toBe(before);
    await r.time.advance(40_000);
    await a.p;
    expect(a.box.out?.r).toBe("playing");
  });
  it("after a job ends the next one runs", async () => {
    const r = rig();
    r.sim.nav = () => ({ delay: 100, url: DOCS_EDIT });
    expect((await r.run(docsCmd(1))).r).toBe("editor_open");
    expect((await r.run(docsCmd(2))).r).toBe("editor_open");
  });
  it("cancel(id) ends the running job as cancelled within one poll; an unknown id is ignored", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytScan = () => ({ s: "wait" });
    const a = r.start(ytCmd(5));
    await r.time.advance(1000);
    r.tasks.cancel(99);
    await r.time.advance(1000);
    expect(a.box.out, "an unknown id must not cancel the job").toBeUndefined();
    const scansBefore = r.sim.scripts("ytScan").length;
    r.tasks.cancel(5);
    await r.time.advance(500);
    await a.p;
    expect(a.box.out?.r).toBe("cancelled");
    expect(r.sim.scripts("ytScan").length - scansBefore, "at most one more poll after cancel (within one poll)").toBeLessThanOrEqual(1);
  });
  it("cancelAll ends it as cancelled", async () => {
    const r = rig();
    healthy(r);
    r.sim.script.ytScan = () => ({ s: "wait" });
    const a = r.start(ytCmd(5));
    await r.time.advance(600);
    r.tasks.cancelAll();
    await r.time.advance(600);
    await a.p;
    expect(a.box.out?.r).toBe("cancelled");
  });
});

describe("E9 JOB_MAX watchdog (75 s)", () => {
  it("a browser call that never settles ends as timeout at about 75 s, leaves no timer, and the next command runs", async () => {
    const r = rig();
    r.sim.hangApis.add("tabs.get");
    const a = r.start(docsCmd(1));
    await r.time.advance(74_000);
    expect(a.box.out, "not before the watchdog").toBeUndefined();
    await r.time.advance(2000);
    await a.p;
    expect(a.box.out?.r).toBe("timeout");
    expect(a.box.out!.ms).toBeGreaterThanOrEqual(74_000);
    expect(a.box.out!.ms).toBeLessThanOrEqual(76_000);
    expect(r.time.pending(), "no sleeper left scheduled after the job ended").toBe(0);
    r.sim.hangApis.clear();
    r.sim.nav = () => ({ delay: 100, url: DOCS_EDIT });
    expect((await r.run(docsCmd(2))).r, "the next command runs (not busy)").toBe("editor_open");
  });
  it("a cancel before the watchdog stays cancelled", async () => {
    const r = rig();
    r.sim.hangApis.add("tabs.get");
    const a = r.start(docsCmd(1));
    await r.time.advance(10_000);
    r.tasks.cancel(1);
    await r.time.advance(500);
    await a.p;
    expect(a.box.out?.r).toBe("cancelled");
    await r.time.advance(100_000);
    expect(a.box.out?.r, "the watchdog must not overwrite it").toBe("cancelled");
    expect(r.time.pending()).toBe(0);
  });
});

describe("E3 (tasks side) every command awaits ready", () => {
  it("nothing touches tabs, scripting, windows or permissions before ready resolves", async () => {
    let release!: () => void;
    const ready = new Promise<void>((res) => (release = res));
    const r = rig({ ready });
    healthy(r);
    const a = r.start(ytCmd());
    await r.time.advance(3000);
    expect(r.sim.calls).toEqual([]);
    release();
    await r.time.advance(40_000);
    await a.p;
    expect(a.box.out?.r).toBe("playing");
  });
  it("a rejected ready: every op answers unsafe and makes no tabs/scripting/windows call", async () => {
    for (const cmd of [ytCmd(1), docsCmd(2), calCmd(3)]) {
      const ready = Promise.reject(new Error("no storage lockdown"));
      ready.catch(() => {});
      const r = rig({ ready });
      healthy(r);
      expect((await r.run(cmd, 5000)).r, cmd.op).toBe("unsafe");
      for (const api of [...guarded, "permissions.contains", "windows.create"]) expect(r.sim.count(api), `${cmd.op} ${api}`).toBe(0);
    }
  });
});

describe("E15 site access not granted", () => {
  it("permissions.contains false: no_access, and no tabs / scripting / windows call", async () => {
    for (const cmd of [ytCmd(1), docsCmd(2), calCmd(3)]) {
      const r = rig({ granted: false });
      healthy(r);
      expect((await r.run(cmd, 5000)).r, cmd.op).toBe("no_access");
      expect(r.sim.count("permissions.contains")).toBeGreaterThanOrEqual(1);
      for (const api of [...guarded, "windows.create"]) expect(r.sim.count(api), `${cmd.op} ${api}`).toBe(0);
    }
  });
});

describe("E12 no console output", () => {
  it("a full job and the failure paths never call console.*", async () => {
    const spies = (["log", "info", "warn", "error", "debug", "trace"] as const).map((m) => vi.spyOn(console, m).mockImplementation(() => {}));
    const r = rig();
    healthy(r);
    await r.run(ytCmd(1));
    const r2 = rig({ granted: false });
    await r2.run(docsCmd(2), 3000);
    const r3 = rig();
    r3.sim.getFails = true;
    await r3.run(calCmd(3), 30_000);
    for (const s of spies) expect(s).not.toHaveBeenCalled();
  });
});
