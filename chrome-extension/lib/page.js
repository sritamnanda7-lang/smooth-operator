// SPDX-License-Identifier: FSL-1.1-MIT
// Functions injected into a YouTube page with chrome.scripting.executeScript (isolated world). Only a
// function's source text is sent, so each one must stay self-contained: no imports, no closures, no
// browser extension API. They only read the page: no click, no typing, no events. Page text is untrusted,
// so they return fixed words and small numbers, never a title, URL or video id read from the page text
// (the one id returned by ytScan passed an 11-character check inside the page function).

/** Selectors from the Z2/Z3 spike. `max` = how many result links ytScan looks at. */
export const SEL = {
  max: 60,
  results: "ytd-two-column-search-results-renderer",
  anchor: "ytd-video-renderer a[href^='/watch?v='], yt-lockup-view-model a[href^='/watch?v=']",
  item: "ytd-video-renderer, yt-lockup-view-model",
  skip: "ytd-ad-slot-renderer, ytd-promoted-video-renderer, ytd-display-ad-renderer, ytd-search-pyv-renderer, ytd-promoted-sparkles-web-renderer",
  live: "ytd-thumbnail-overlay-time-status-renderer[overlay-style='LIVE'], .badge-style-type-live-now",
  consent: "ytd-consent-bump-v2-lightbox, tp-yt-paper-dialog form[action*='consent.youtube.com']",
  video: "#movie_player video",
  adShowing: "#movie_player.ad-showing",
};

/** @returns {{s: "found", id: string} | {s: "none"} | {s: "wait"} | {s: "consent"}} */
export function ytScan(sel, expectedQuery) {
  try {
    if (document.querySelector(sel.consent)) return { s: "consent" };
    if (location.pathname !== "/results" || new URLSearchParams(location.search).get("search_query") !== expectedQuery) return { s: "wait" };
    const box = document.querySelector(sel.results);
    if (!box) return { s: "wait" };
    const anchors = Array.from(box.querySelectorAll(sel.anchor)).slice(0, sel.max);
    for (const a of anchors) {
      const href = a.getAttribute("href") || "";
      const m = /^\/watch\?v=([A-Za-z0-9_-]{11})(&|$)/.exec(href);
      // Skip real playlists only; live results carry list=RD... (radio mix) on normal videos.
      if (!m || /[?&]list=(?!RD)/.test(href)) continue;
      const item = a.closest(sel.item);
      if (!item || item.closest(sel.skip) || item.querySelector(sel.live)) continue;
      return { s: "found", id: m[1] };
    }
    return { s: "none" };
  } catch {
    return { s: "wait" };
  }
}

/** @returns {{s: "playing" | "ad" | "paused" | "none", t: number}} t = currentTime in seconds (0 if unknown) */
export function ytPlayer(sel, expectedId) {
  try {
    if (location.pathname !== "/watch" || new URLSearchParams(location.search).get("v") !== expectedId) return { s: "none", t: 0 };
    const v = document.querySelector(sel.video);
    if (!v) return { s: "none", t: 0 };
    const t = Number.isFinite(v.currentTime) ? v.currentTime : 0;
    if (document.querySelector(sel.adShowing)) return { s: "ad", t };
    if (v.paused) return { s: "paused", t };
    return { s: v.readyState >= 3 ? "playing" : "none", t };
  } catch {
    return { s: "none", t: 0 };
  }
}

/**
 * Calls the page's own video.play() once (only injected when TRY_PLAY is true). No click, no event.
 * play() may never settle, so wait for it at most half a second.
 * @returns {Promise<"played" | "novideo" | "mismatch" | "error">}
 */
export async function ytPlay(sel, expectedId) {
  try {
    if (location.pathname !== "/watch" || new URLSearchParams(location.search).get("v") !== expectedId) return "mismatch";
    const v = document.querySelector(sel.video);
    if (!v) return "novideo";
    let timer;
    const giveUp = new Promise((res) => {
      timer = setTimeout(res, 500);
    });
    try {
      await Promise.race([v.play(), giveUp]);
    } finally {
      clearTimeout(timer);
    }
    return "played";
  } catch {
    return "error";
  }
}
