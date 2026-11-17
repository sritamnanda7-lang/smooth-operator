// SPDX-License-Identifier: FSL-1.1-MIT
// Runs one page job at a time (yt_play, docs_open, cal_open) and answers with a result code from R.
// Only fixed words and numbers leave this file: never a URL, title or page text. Every browser call is
// raced against the cancel signal, so cancel() ends a job at once even if the browser call never settles.
import { ACTIVE_TAB, AD_WAIT, EMPTY_WAIT, CAL_EDIT_RE, DOCS_EDIT_RE, DOCS_URL, LOGIN_CHECKS, PAGE_POLL, PAGE_WAIT, PLAYER_POLL, PLAY_WAIT, RESULTS_WAIT, JOB_MAX, JOB_TICK, SCAN_POLL, TRY_PLAY, VIDEO_ID_RE, WATCH_WAIT } from "./config.js";
import { SEL, ytPlay, ytPlayer, ytScan } from "./page.js";
import { R } from "./protocol.js";

const HOST = { yt_play: "https://www.youtube.com/*", docs_open: "https://docs.google.com/*", cal_open: "https://calendar.google.com/*" };

/** Same origin + path and the one param `key` equal to the expected URL's; other params, %20 vs +, :443, host case are fine. */
function sameUrl(got, want, key) {
  try {
    const g = new URL(got);
    const w = new URL(want);
    return g.origin === w.origin && g.pathname === w.pathname && w.searchParams.get(key) !== null && g.searchParams.get(key) === w.searchParams.get(key);
  } catch {
    return false;
  }
}

/** Thrown to end a job with a result code. */
class Done {
  constructor(r) {
    this.r = r;
  }
}

export function makeTasks({ chrome, ready, now, sleep }) {
  let job = null; // {id, cancel} of the one running job

  async function exec(cmd, call) {
    // Nothing touches the browser until the storage lockdown succeeded.
    try {
      // Waited for in slices: a standing timer keeps the job's clock moving while the lockdown is pending.
      let settled = false;
      const flag = ready.then(() => (settled = true), () => (settled = true));
      while (!settled) await call(Promise.race([flag, sleep(JOB_TICK)]));
      await call(ready);
    } catch (e) {
      throw e instanceof Done ? e : new Done(R.unsafe);
    }
    if (!(await call(chrome.permissions.contains({ origins: [HOST[cmd.op]] })))) return R.no_access;

    const nap = (ms) => call(sleep(ms));
    const inject = (tabId, func, args) => chrome.scripting.executeScript({ target: { tabId }, func, args });
    const page = async (tabId, func, args) => (await call(inject(tabId, func, args)))?.[0]?.result;

    /** Opens `url` in the foreground (yt_play reuses the YouTube tab used last). Returns the tab id. */
    async function open(url, reuse) {
      const old = reuse ? (await call(chrome.tabs.query({ url: HOST[cmd.op] }))).sort((a, b) => (b.lastAccessed ?? 0) - (a.lastAccessed ?? 0))[0] : undefined;
      const tab = old ? await call(chrome.tabs.update(old.id, { url, active: true })) : await call(chrome.tabs.create({ url, active: ACTIVE_TAB }));
      await call(chrome.windows.update(tab.windowId, { focused: true }).catch(() => {})); // best effort
      return tab.id;
    }

    /** Polls the tab until its URL passes `ok`. Google asking the user to sign in hides the URL: needs_login. */
    async function waitUrl(tabId, ok, poll, limit) {
      const end = now() + limit;
      let blind = 0;
      for (;;) {
        const tab = await call(chrome.tabs.get(tabId));
        if (tab.url && ok(tab.url)) return;
        blind = tab.status === "complete" && !tab.url ? blind + 1 : 0;
        if (blind >= LOGIN_CHECKS) throw new Done(R.needs_login);
        if (now() >= end) throw new Done(R.timeout);
        await nap(poll);
      }
    }

    if (cmd.op === "docs_open" || cmd.op === "cal_open") {
      const tabId = await open(cmd.op === "docs_open" ? DOCS_URL : cmd.url, false);
      const re = cmd.op === "docs_open" ? DOCS_EDIT_RE : CAL_EDIT_RE;
      await waitUrl(tabId, (u) => re.test(u), PAGE_POLL, PAGE_WAIT);
      return R.editor_open;
    }

    // yt_play: results page -> first real video -> watch page -> is it playing
    const tabId = await open(cmd.url, true);
    await waitUrl(tabId, (u) => sameUrl(u, cmd.url, "search_query"), SCAN_POLL, PAGE_WAIT); // never scan the old document
    const scanEnd = now() + RESULTS_WAIT;
    let id = null;
    let emptySince = null; // when the results list was first seen empty, in a row
    while (id === null) {
      const r = await page(tabId, ytScan, [SEL, cmd.query]);
      if (r?.s === "consent") return R.consent;
      emptySince = r?.s === "none" ? emptySince ?? now() : null; // a wait or a hit resets it
      if (emptySince !== null && now() - emptySince >= EMPTY_WAIT) return R.no_video;
      if (r?.s === "found") {
        if (typeof r.id !== "string" || !VIDEO_ID_RE.test(r.id)) return R.error; // the page misbehaves: never navigate on its word
        id = r.id;
      } else if (now() >= scanEnd) return R.timeout;
      else await nap(SCAN_POLL);
    }
    const watch = `https://www.youtube.com/watch?v=${id}`;
    await call(chrome.tabs.update(tabId, { url: watch }));
    await waitUrl(tabId, (u) => sameUrl(u, watch, "v"), SCAN_POLL, PAGE_WAIT);

    const start = now();
    let adSince = null;
    let last = "none";
    let prevT = null;
    let tried = false;
    for (;;) {
      const r = await page(tabId, ytPlayer, [SEL, id]);
      last = ["playing", "ad", "paused"].includes(r?.s) ? r.s : "none";
      const t = Number.isFinite(r?.t) ? r.t : 0;
      if (last === "ad" && adSince === null) adSince = now();
      if (last === "playing" && prevT !== null && t > prevT) return R.playing; // the clock must be moving
      prevT = last === "playing" ? t : null;
      if (TRY_PLAY && last === "paused" && !tried && now() - start >= PLAY_WAIT) {
        tried = true;
        inject(tabId, ytPlay, [SEL, id]).catch(() => {}); // fire and forget: play() may never settle
      }
      const end = Math.max(start + WATCH_WAIT, adSince === null ? 0 : adSince + AD_WAIT);
      if (now() >= end) return last === "ad" ? R.ad : last === "paused" ? R.paused : R.timeout;
      await nap(PLAYER_POLL);
    }
  }

  return {
    async run(cmd) {
      if (job) return { r: R.busy, ms: 0 };
      const t0 = now();
      let cancel;
      const cancelled = new Promise((_, rej) => {
        cancel = (r) => rej(new Done(r));
      });
      cancelled.catch(() => {});
      const me = { id: cmd.id, cancel: () => cancel(R.cancelled) };
      job = me;
      // Watchdog: whatever a browser call does, a job never outlives JOB_MAX (the app's own deadline is shorter).
      void sleep(JOB_MAX).then(() => {
        if (job === me) cancel(R.timeout);
      });
      let r;
      try {
        r = await exec(cmd, (p) => Promise.race([p, cancelled]));
      } catch (e) {
        r = e instanceof Done ? e.r : R.error; // any browser error: a fixed word, never its message
      } finally {
        job = null;
      }
      return { r, ms: now() - t0 };
    },
    cancel(id) {
      if (job && job.id === id) job.cancel();
    },
    cancelAll() {
      job?.cancel();
    },
  };
}
