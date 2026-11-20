// @vitest-environment happy-dom
// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 E4 (ytScan) and E5 (ytPlayer): chrome-extension/lib/page.js. Fixtures are synthetic HTML in the structure
// the spike (Z2) found: ytd-two-column-search-results-renderer > ytd-section-list-renderer > ytd-item-section-renderer
// > ytd-video-renderer (thumbnail anchor + title anchor, both /watch?v=ID); newer items are yt-lockup-view-model.
// Every call goes through a function REBUILT from its source text, because only the source text reaches the page
// (executeScript): a closure or import in page.js would throw a ReferenceError here.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SEL, ytPlay, ytPlayer, ytScan } from "../../../chrome-extension/lib/page.js";

// Only the source text of a function reaches the page (executeScript), so rebuild it from toString: a closure or import fails.
const rebuilt = (f: (...a: any[]) => unknown) => new Function(`return (${f.toString()})`)() as (...a: any[]) => any;
const scan = rebuilt(ytScan);
const player = rebuilt(ytPlayer);
const playFn = rebuilt(ytPlay);

const ID = (n: number) => `ID${String(n).padStart(9, "0")}`; // 11 chars
const video = (id: string, extra = "") =>
  `<ytd-video-renderer><div><ytd-thumbnail><a id="thumbnail" href="/watch?v=${id}"></a></ytd-thumbnail><h3><a id="video-title" href="/watch?v=${id}&pp=abc">Title ${id}</a></h3>${extra}</div></ytd-video-renderer>`;
const lockup = (id: string) => `<yt-lockup-view-model><div class="yt-lockup-view-model"><a class="yt-lockup-view-model__content-image" href="/watch?v=${id}"></a></div></yt-lockup-view-model>`;
const ad = (id: string) => `<ytd-ad-slot-renderer><div><a href="/watch?v=${id}">Sponsored</a></div></ytd-ad-slot-renderer>`;
const shorts = (id: string) => `<ytd-reel-shelf-renderer><div><a href="/shorts/${id}">short</a></div></ytd-reel-shelf-renderer>`;
// Real playlist (list=PL...): skipped. A radio mix (list=RD...) is a normal result; see the "live-shaped" tests below.
const mix = (id: string) => `<ytd-video-renderer><a href="/watch?v=${id}&list=PL${id}">Playlist</a></ytd-video-renderer>`;
const radio = (id: string, tail = "&start_radio=1") => `<ytd-video-renderer><div><ytd-thumbnail><a id="thumbnail" href="/watch?v=${id}&list=RD${id}${tail}"></a></ytd-thumbnail><h3><a id="video-title" href="/watch?v=${id}&list=RD${id}${tail}&pp=abc">T</a></h3></div></ytd-video-renderer>`;
const live = (id: string) =>
  `<ytd-video-renderer><a href="/watch?v=${id}"></a><ytd-badge-supported-renderer><div class="badge badge-style-type-live-now"><span>LIVE</span></div></ytd-badge-supported-renderer><ytd-thumbnail-overlay-time-status-renderer overlay-style="LIVE"></ytd-thumbnail-overlay-time-status-renderer></ytd-video-renderer>`;
const results = (items: string) =>
  `<ytd-masthead></ytd-masthead><ytd-two-column-search-results-renderer><ytd-section-list-renderer><ytd-item-section-renderer>${items}</ytd-item-section-renderer></ytd-section-list-renderer></ytd-two-column-search-results-renderer>`;
const setPage = (html: string, url = "https://www.youtube.com/results?search_query=blinding+lights") => {
  (window as any).happyDOM.setURL(url);
  document.body.innerHTML = html;
};
const Q = "blinding lights";

beforeEach(() => setPage(""));
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("E4 ytScan", () => {
  it("SEL exists and the function survives being rebuilt from its source", () => {
    expect(SEL && typeof SEL === "object").toBe(true);
    expect(typeof scan).toBe("function");
  });
  it("an ad first, then a real video: the video's id", () => {
    setPage(results(ad(ID(1)) + video(ID(2)) + video(ID(3))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(2) });
  });
  it("the newer lockup structure works too", () => {
    setPage(results(lockup(ID(4))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(4) });
  });
  it("Shorts are skipped; a Shorts shelf first does not win", () => {
    setPage(results(shorts("SHORTSID123") + video(ID(5))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(5) });
  });
  it("a real playlist (list=PL) is skipped", () => {
    setPage(results(mix(ID(6)) + video(ID(7))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(7) });
  });
  it("live-shaped results: ad first, then list=RD radio-mix items (&start_radio=1, &pp=): first non-ad id", () => {
    setPage(results(ad(ID(1)) + radio(ID(2)) + radio(ID(3), "&pp=xyz")));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(2) });
  });
  it("list=RD is ok, list=PL is skipped, next RD item chosen", () => {
    setPage(results(mix(ID(4)) + radio(ID(5)) + radio(ID(6))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(5) });
  });
  it("list=UU and other non-RD playlists are skipped too", () => {
    const uu = `<ytd-video-renderer><a href="/watch?v=${ID(7)}&list=UU123">x</a></ytd-video-renderer>`;
    setPage(results(uu + radio(ID(8))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(8) });
  });
  it("only real-playlist links: none", () => {
    setPage(results(mix(ID(4)) + mix(ID(5))));
    expect(scan(SEL, Q)).toEqual({ s: "none" });
  });
  it("a live stream is skipped", () => {
    setPage(results(live(ID(8)) + video(ID(9))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(9) });
  });
  it("only skippable items: none", () => {
    setPage(results(ad(ID(1)) + shorts("SHORTSID123") + mix(ID(6)) + live(ID(8))));
    expect(scan(SEL, Q)).toEqual({ s: "none" });
  });
  it("results container present but empty: none", () => {
    setPage(results(""));
    expect(scan(SEL, Q)).toEqual({ s: "none" });
  });
  it("results container not there yet (page still loading): wait", () => {
    setPage("<ytd-masthead></ytd-masthead>");
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
    setPage("");
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
  });
  it("a consent dialog: consent", () => {
    setPage(`<ytd-consent-bump-v2-lightbox><tp-yt-paper-dialog><form action="https://consent.youtube.com/save"><button>Accept all</button></form></tp-yt-paper-dialog></ytd-consent-bump-v2-lightbox>`);
    expect(scan(SEL, Q)).toEqual({ s: "consent" });
  });
  it("returns only the keys s (and id for found)", () => {
    setPage(results(video(ID(2))));
    expect(Object.keys(scan(SEL, Q)).sort()).toEqual(["id", "s"]);
    setPage(results(""));
    expect(Object.keys(scan(SEL, Q))).toEqual(["s"]);
    setPage("");
    expect(Object.keys(scan(SEL, Q))).toEqual(["s"]);
  });
  it("malformed or hostile hrefs are never found", () => {
    const hrefs = [
      "/watch?v=short",
      "/watch?v=",
      `/watch?v=${ID(1)}X`, // 12 chars
      "/watch?v=bad!char!!!",
      "/watch?v=bad char abc",
      `/watch?x=${ID(1)}`,
      `/watchfake?v=${ID(1)}`,
      `/shorts/${ID(1)}`,
      `javascript:void(0)//watch?v=${ID(1)}`,
      `https://evil.example/watch?v=${ID(1)}`,
      `//evil.example/watch?v=${ID(1)}`,
    ];
    for (const h of hrefs) {
      setPage(results(`<ytd-video-renderer><a href="${h}">x</a></ytd-video-renderer>`));
      const r = scan(SEL, Q);
      expect(r.s, h).not.toBe("found");
    }
  });
  it("old results for another query, or a page that is not /results, give wait (never found)", () => {
    setPage(results(video(ID(2))), "https://www.youtube.com/results?search_query=old+query");
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
    setPage(results(video(ID(2))), "https://www.youtube.com/results");
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
    setPage(results(video(ID(2))), "https://www.youtube.com/watch?v=" + ID(9));
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
    setPage(results(video(ID(2))), "https://www.youtube.com/");
    expect(scan(SEL, Q)).toEqual({ s: "wait" });
    // positive control: the same DOM with the matching query is found
    setPage(results(video(ID(2))));
    expect(scan(SEL, Q).s).toBe("found");
  });
  it("only the first 60 result anchors count: a real video at anchor 61 is none", () => {
    const junk = Array.from({ length: 60 }, (_, i) => mix(ID(100 + i))).join("");
    setPage(results(junk + video(ID(2))));
    expect(scan(SEL, Q)).toEqual({ s: "none" });
    // positive control: the same video earlier is found
    setPage(results(junk.split("</ytd-video-renderer>").slice(0, 5).join("</ytd-video-renderer>") + "</ytd-video-renderer>" + video(ID(2))));
    expect(scan(SEL, Q)).toEqual({ s: "found", id: ID(2) });
  });
});

// ---------- E5 ----------
const watchUrl = (id: string) => `https://www.youtube.com/watch?v=${id}`;
function watchPage(o: { paused?: boolean; ready?: number; t?: number; ad?: boolean; video?: boolean }) {
  setPage(
    `<div id="movie_player" class="html5-video-player${o.ad ? " ad-showing ad-interrupting" : ""}">${o.video === false ? "" : '<video class="html5-main-video"></video>'}</div>`,
    watchUrl(ID(1)),
  );
  const v = document.querySelector("video");
  if (v) {
    Object.defineProperty(v, "paused", { value: o.paused ?? false, configurable: true });
    Object.defineProperty(v, "readyState", { value: o.ready ?? 4, configurable: true });
    Object.defineProperty(v, "currentTime", { value: o.t ?? 5.5, configurable: true });
  }
}

describe("E5 ytPlayer", () => {
  it("playing: not paused, ready, with the current time", () => {
    watchPage({ t: 5.5 });
    const r = player(SEL, ID(1));
    expect(r.s).toBe("playing");
    expect(r.t).toBe(5.5);
  });
  it("paused", () => {
    watchPage({ paused: true });
    expect(player(SEL, ID(1)).s).toBe("paused");
  });
  it("an ad is showing: ad (even though the video element plays)", () => {
    watchPage({ ad: true });
    expect(player(SEL, ID(1)).s).toBe("ad");
  });
  it("not enough data yet is not playing", () => {
    watchPage({ ready: 1 });
    expect(player(SEL, ID(1)).s).not.toBe("playing");
  });
  it("no video element: none", () => {
    watchPage({ video: false });
    expect(player(SEL, ID(1)).s).toBe("none");
  });
  it("the page shows another video (the old one playing in a reused tab): none, never playing", () => {
    watchPage({ t: 50 });
    expect(player(SEL, ID(2))).toMatchObject({ s: "none" });
    setPage(`<div id="movie_player"><video class="html5-main-video"></video></div>`, "https://www.youtube.com/results?search_query=x");
    expect(player(SEL, ID(1)).s).toBe("none");
    watchPage({ t: 50 });
    expect(player(SEL, ID(1)).s, "positive control").toBe("playing");
  });
  it("returns only s and t", () => {
    watchPage({});
    expect(Object.keys(player(SEL, ID(1))).sort()).toEqual(["s", "t"]);
  });
});

// ---------- ytPlay: the one video.play() call (only injected when TRY_PLAY is true) ----------
describe("ytPlay", () => {
  const ENUM = ["played", "novideo", "mismatch", "error"];
  const withPlay = (play: unknown) => {
    watchPage({ paused: true });
    const v = document.querySelector("video")!;
    Object.defineProperty(v, "play", { value: play, configurable: true, writable: true });
    return v;
  };
  it("calls video.play() exactly once and returns only a short enum string", async () => {
    const play = vi.fn(() => Promise.resolve());
    withPlay(play);
    const r = await playFn(SEL, ID(1));
    expect(play).toHaveBeenCalledTimes(1);
    expect(r).toBe("played");
    expect(ENUM).toContain(r);
    expect(typeof r).toBe("string");
  });
  it("never clicks, focuses or dispatches events", async () => {
    const play = vi.fn(() => Promise.resolve());
    withPlay(play);
    const spies = [vi.spyOn(EventTarget.prototype, "dispatchEvent"), vi.spyOn(HTMLElement.prototype, "click"), vi.spyOn(HTMLElement.prototype, "focus")];
    await playFn(SEL, ID(1));
    for (const s of spies) expect(s).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });
  it("the page shows another video: mismatch, and play() is not called", async () => {
    const play = vi.fn(() => Promise.resolve());
    withPlay(play);
    expect(await playFn(SEL, ID(2))).toBe("mismatch");
    expect(play).not.toHaveBeenCalled();
    setPage(`<div id="movie_player"><video class="html5-main-video"></video></div>`, "https://www.youtube.com/results?search_query=x");
    const play2 = vi.fn(() => Promise.resolve());
    Object.defineProperty(document.querySelector("video")!, "play", { value: play2, configurable: true });
    expect(await playFn(SEL, ID(1))).toBe("mismatch");
    expect(play2).not.toHaveBeenCalled();
  });
  it("no video element: novideo", async () => {
    watchPage({ video: false });
    expect(await playFn(SEL, ID(1))).toBe("novideo");
  });
  it("a hostile page whose play() throws, rejects or is not a function gives error and never throws", async () => {
    for (const play of [
      () => {
        throw new Error("Secret Title https://evil.example");
      },
      () => Promise.reject(new Error("Secret Title")),
      "not a function",
      undefined,
    ]) {
      withPlay(play);
      const r = await playFn(SEL, ID(1));
      expect(r).toBe("error");
    }
  });
  it("a getter that throws when play is read gives error", async () => {
    watchPage({ paused: true });
    Object.defineProperty(document.querySelector("video")!, "play", {
      get() {
        throw new Error("Secret Title");
      },
      configurable: true,
    });
    expect(await playFn(SEL, ID(1))).toBe("error");
  });
  it("a play() that never settles does not hang the injection: played or error within 1 s", async () => {
    vi.useFakeTimers();
    withPlay(() => new Promise(() => {}));
    let out: unknown;
    void Promise.resolve(playFn(SEL, ID(1))).then((v) => (out = v));
    await vi.advanceTimersByTimeAsync(1000);
    expect(["played", "error"]).toContain(out);
  });
});
