// SPDX-License-Identifier: FSL-1.1-MIT
// Step 6 E1: chrome-extension/lib/cmd.js. Everything the app sends as a command is untrusted until `parseCmd` accepts it.
// Rules: exact origin and path, no credentials/hash/port, exact query keys in order, lengths in CODE POINTS, no control
// characters, calendar dates in a window. A bad command with a valid id still returns that id (the link answers bad_cmd).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { checkCalUrl, checkYtUrl, cpLen, parseCmd } from "../../../chrome-extension/lib/cmd.js";

const NOW = Date.UTC(2026, 10, 15, 10, 0, 0);
const MIN = 60_000;
const DAY = 24 * 60 * MIN;
const stamp = (ms: number) => new Date(ms).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
const dates = (start: number, mins = 60) => `${stamp(start)}/${stamp(start + mins * MIN)}`;
const YT = "https://www.youtube.com/results?search_query=Blinding+Lights";
const cal = (text = "Review", d = dates(NOW + DAY), action = "TEMPLATE") => `https://calendar.google.com/calendar/render?action=${action}&text=${encodeURIComponent(text)}&dates=${d}`;
const yt = (q: string) => `https://www.youtube.com/results?search_query=${encodeURIComponent(q)}`;
const msg = (o: Record<string, unknown>) => ({ t: "cmd", v: 1, id: 7, ...o });
// `want` is the id the link must get back; null means "no id" (an explicit undefined would trigger a default).
const bad = (o: unknown, want: number | null = 7) => {
  const id = want ?? undefined;
  const r = parseCmd(o as never, NOW) as { ok: boolean; id?: number };
  expect(r.ok, JSON.stringify(o)).toBe(false);
  expect(r.id, `id for ${JSON.stringify(o)}`).toBe(id);
};

describe("E1 cpLen counts code points", () => {
  it("an emoji or music symbol is 1, like Rust chars().count()", () => {
    expect(cpLen("")).toBe(0);
    expect(cpLen("abc")).toBe(3);
    expect(cpLen("𝄞")).toBe(1);
    expect("𝄞".length).toBe(2);
    expect(cpLen("𝄞".repeat(100))).toBe(100);
    expect(cpLen("é—a")).toBe(3);
  });
});

describe("E1 parseCmd accepts the three shapes", () => {
  it("yt_play: id, op, url and the decoded query", () => {
    const r = parseCmd(msg({ op: "yt_play", url: YT }), NOW) as any;
    expect(r.ok).toBe(true);
    expect(r.cmd.id).toBe(7);
    expect(r.cmd.op).toBe("yt_play");
    expect(r.cmd.url).toBe(YT);
    expect(r.cmd.query).toBe("Blinding Lights");
  });
  it("docs_open: no url", () => {
    const r = parseCmd(msg({ op: "docs_open" }), NOW) as any;
    expect(r.ok).toBe(true);
    expect(r.cmd.op).toBe("docs_open");
    expect(r.cmd.url).toBeUndefined();
  });
  it("cal_open: the url, with / raw or %2F in dates", () => {
    for (const u of [cal(), cal().replaceAll("/2026", "%2F2026")]) {
      const r = parseCmd(msg({ op: "cal_open", url: u }), NOW) as any;
      expect(r.ok, u).toBe(true);
      expect(r.cmd.url).toBe(u);
    }
  });
  it("the A8 title of 100 x 𝄞 (200 UTF-16 units, 100 code points) is accepted; 120 x 𝄞 as a search is accepted", () => {
    const t = "𝄞".repeat(100);
    expect(t.length).toBe(200);
    expect((parseCmd(msg({ op: "cal_open", url: cal(t) }), NOW) as any).ok).toBe(true);
    expect((parseCmd(msg({ op: "yt_play", url: yt("𝄞".repeat(120)) }), NOW) as any).ok).toBe(true);
  });
  it("boundaries: 1 and 120 code points of query, 1 and 100 of title, 480 minutes, start within the window", () => {
    for (const q of ["a", "a".repeat(120)]) expect(checkYtUrl(yt(q)), q.length + "").toBe(true);
    for (const t of ["a", "a".repeat(100)]) expect(checkCalUrl(cal(t), NOW), t.length + "").toBe(true);
    expect(checkCalUrl(cal("x", dates(NOW + DAY, 480)), NOW)).toBe(true);
    expect(checkCalUrl(cal("x", dates(NOW - DAY + MIN)), NOW)).toBe(true);
    expect(checkCalUrl(cal("x", dates(NOW + 400 * DAY - MIN)), NOW)).toBe(true);
  });
});

describe("N1 only canonical URLs (new URL(s).href === s)", () => {
  const dates1 = dates(NOW + DAY);
  const calBase = `https://calendar.google.com/calendar/render?action=TEMPLATE&text=Review&dates=${dates1}`;
  it("the app's own URL shapes still pass", () => {
    for (const u of [YT, yt("Beyoncé — Halo"), yt("𝄞".repeat(5)), yt("a&b=c#d"), cal(), cal("He said \"hi\" \\ back"), cal("𝄞".repeat(100)), cal().replaceAll("/2026", "%2F2026"), new URL(`https://www.youtube.com/results?${new URLSearchParams({ search_query: "Beyoncé — Halo ~!*" })}`).href]) {
      expect(new URL(u).href, u).toBe(u);
      expect(new URL(u).hostname === "www.youtube.com" ? checkYtUrl(u) : checkCalUrl(u, NOW), u).toBe(true);
    }
  });
  it("non-canonical spellings are rejected", () => {
    const yts = [
      "https://www.youtube.com/results?search_query=a b",
      "https://www.youtube.com/results?search_query=a\tb",
      "https://www.youtube.com/results?search_query=a\nb",
      "https://www.youtube.com/results?search_query=a'b",
      "https://WWW.YOUTUBE.COM/results?search_query=a",
      "https://www.youtube.com:443/results?search_query=a",
      "https://www.youtube.com/results?%73earch_query=a",
      " https://www.youtube.com/results?search_query=a",
      "https://www.youtube.com/results?search_query=a ",
      "https://www.youtube.com/./results?search_query=a",
    ];
    for (const u of yts) {
      expect(checkYtUrl(u), JSON.stringify(u)).toBe(false);
      bad(msg({ op: "yt_play", url: u }));
    }
    const cals = [
      calBase.replaceAll("&text=", "&%74ext="),
      calBase.replaceAll("Review", "Re view"),
      calBase.replaceAll("Review", "Re\tview"),
      calBase.replaceAll("https://calendar", "https://CALENDAR"),
      calBase.replaceAll(".com/", ".com:443/"),
      calBase.replaceAll("TEMPLATE", "TEM%50LATE"),
      " " + calBase,
    ];
    for (const u of cals) {
      expect(checkCalUrl(u, NOW), JSON.stringify(u)).toBe(false);
      bad(msg({ op: "cal_open", url: u }));
    }
  });
});

describe("N2 calendar event length", () => {
  it("shorter than 5 minutes is rejected, 5 minutes is fine, a start up to 1 day in the past is still allowed", () => {
    for (const m of [1, 4]) expect(checkCalUrl(cal("x", dates(NOW + DAY, m)), NOW), `${m} min`).toBe(false);
    expect(checkCalUrl(cal("x", dates(NOW + DAY, 5)), NOW)).toBe(true);
    expect(checkCalUrl(cal("x", dates(NOW - DAY + MIN, 5)), NOW)).toBe(true);
    expect(checkCalUrl(cal("x", dates(NOW - DAY + MIN, 4)), NOW)).toBe(false);
  });
});

describe("E1 parseCmd rejects, and keeps a valid id", () => {
  it("unknown op, extra key, wrong t or v, missing keys", () => {
    bad(msg({ op: "nope" }));
    bad(msg({ op: "docs_open", extra: 1 }));
    bad(msg({ op: "docs_open", url: "https://docs.new/" }));
    bad(msg({ op: "yt_play", url: YT, extra: 1 }));
    bad(msg({ op: "yt_play" }));
    bad(msg({ op: "cal_open" }));
    bad(msg({ op: "yt_play", url: 5 }));
    bad({ t: "cmd", v: 2, id: 7, op: "docs_open" });
    bad({ t: "cmd", id: 7, op: "docs_open" });
    bad({ t: "nope", v: 1, id: 7, op: "docs_open" });
    bad({ t: "cmd", v: 1, id: 7 });
  });
  it("bad ids give no id back", () => {
    for (const id of [0, -1, 1.5, "1", null, 2147483648, NaN, undefined, [], {}]) bad({ t: "cmd", v: 1, id, op: "docs_open" }, null);
    expect((parseCmd({ t: "cmd", v: 1, id: 2147483647, op: "docs_open" } as never, NOW) as any).ok).toBe(true);
    expect((parseCmd({ t: "cmd", v: 1, id: 1, op: "docs_open" } as never, NOW) as any).ok).toBe(true);
  });
  it("not an object at all", () => {
    for (const o of [null, undefined, 5, "x", []]) bad(o, null);
  });
  it("YouTube: scheme, host, port, credentials, hash, path, keys, lengths, control characters", () => {
    const cases: [string, string][] = [
      ["http://www.youtube.com/results?search_query=a", "http"],
      ["https://youtube.com/results?search_query=a", "no www"],
      ["https://m.youtube.com/results?search_query=a", "m."],
      ["https://www.youtube.com.evil.com/results?search_query=a", "suffix host"],
      ["https://evil.com/www.youtube.com/results?search_query=a", "path trick"],
      ["https://a@www.youtube.com/results?search_query=a", "username"],
      ["https://a:b@www.youtube.com/results?search_query=a", "password"],
      ["https://www.youtube.com:8443/results?search_query=a", "port"],
      ["https://www.youtube.com/results?search_query=a#x", "hash"],
      ["https://www.youtube.com/results?search_query=a#", "empty hash"],
      ["https://www.youtube.com/watch?search_query=a", "wrong path"],
      ["https://www.youtube.com/results/?search_query=a", "trailing slash path"],
      ["https://www.youtube.com/results", "no query"],
      ["https://www.youtube.com/results?search_query=", "empty query"],
      ["https://www.youtube.com/results?q=a", "wrong key"],
      ["https://www.youtube.com/results?search_query=a&x=1", "extra key"],
      ["https://www.youtube.com/results?x=1&search_query=a", "extra key first"],
      ["https://www.youtube.com/results?search_query=a&search_query=b", "repeated key"],
      [yt("a".repeat(121)), "121 code points"],
      [yt("𝄞".repeat(121)), "121 x 4-byte"],
      ["https://www.youtube.com/results?search_query=a%00b", "NUL"],
      ["https://www.youtube.com/results?search_query=a%0Ab", "newline"],
      ["https://www.youtube.com/results?search_query=a%1Fb", "unit separator"],
      ["https://www.youtube.com/results?search_query=a%7Fb", "DEL"],
      ["javascript:alert(1)", "javascript"],
      ["not a url", "garbage"],
      ["", "empty"],
    ];
    for (const [u, why] of cases) {
      expect(checkYtUrl(u), why).toBe(false);
      bad(msg({ op: "yt_play", url: u }));
    }
    expect(checkYtUrl(5 as never)).toBe(false);
    expect(checkYtUrl(null as never)).toBe(false);
  });
  it("Calendar: origin, path, keys and their ORDER, action, dates, lengths", () => {
    const d = dates(NOW + DAY);
    const base = "https://calendar.google.com/calendar/render";
    const cases: [string, string][] = [
      [cal().replaceAll("https:", "http:"), "http"],
      [cal().replaceAll("calendar.google.com", "calendar.google.com.evil.com"), "suffix host"],
      [cal().replaceAll("https://", "https://a@"), "username"],
      [cal().replaceAll(".com/", ".com:8443/"), "port"],
      [cal() + "#x", "hash"],
      [cal().replaceAll("/calendar/render", "/calendar/u/0/render"), "other path"],
      [cal().replaceAll("/calendar/render", "/calendar/r/eventedit"), "eventedit path"],
      [`${base}?text=Review&action=TEMPLATE&dates=${d}`, "keys out of order"],
      [`${base}?action=TEMPLATE&dates=${d}&text=Review`, "keys out of order 2"],
      [cal() + "&add=a@b.co", "add"],
      [cal() + "&ctz=Asia/Kolkata", "ctz"],
      [cal() + "&authuser=1", "authuser"],
      [cal() + "&details=hi", "details"],
      [`${base}?action=TEMPLATE&text=Review`, "no dates"],
      [`${base}?action=TEMPLATE&dates=${d}`, "no text"],
      [`${base}?text=Review&dates=${d}`, "no action"],
      [cal("Review", d, "EDIT"), "action not TEMPLATE"],
      [cal("Review", d, "template"), "action lower case"],
      [cal("a".repeat(101)), "101 code points of text"],
      [cal("𝄞".repeat(101)), "101 x 4-byte"],
      [cal(""), "empty text"],
      [`${base}?action=TEMPLATE&text=a%00b&dates=${d}`, "NUL in text"],
      [`${base}?action=TEMPLATE&text=a%0Ab&dates=${d}`, "newline in text"],
      [cal("x", stamp(NOW + DAY).replaceAll("Z", "") + "/" + stamp(NOW + DAY + 60 * MIN).replaceAll("Z", "")), "dates without Z"],
      [cal("x", `${stamp(NOW + DAY)}/${stamp(NOW + DAY)}`), "end == start"],
      [cal("x", `${stamp(NOW + DAY + 60 * MIN)}/${stamp(NOW + DAY)}`), "end before start"],
      [cal("x", dates(NOW + DAY, 481)), "481 minutes"],
      [cal("x", dates(NOW - DAY - MIN)), "start before now - 1 day"],
      [cal("x", dates(NOW + 400 * DAY + MIN)), "start after now + 400 days"],
      [cal("x", "20261116T093000Z"), "one date only"],
      [cal("x", "2026-11-16/2026-11-17"), "ISO dates"],
    ];
    for (const [u, why] of cases) {
      expect(checkCalUrl(u, NOW), why).toBe(false);
      bad(msg({ op: "cal_open", url: u }));
    }
    expect(checkCalUrl(5 as never, NOW)).toBe(false);
  });
});

// Round trip with the Rust builders: tests/so/fixtures/url-roundtrip.json holds the exact URLs `exec::ops` produced
// (exec_ops_tests.rs a10 fails if Rust drifts from it; this block fails if the extension rejects or alters them).
describe("round trip with the Rust-built URLs", () => {
  const fx = JSON.parse(readFileSync(join(import.meta.dirname, "..", "fixtures", "url-roundtrip.json"), "utf8")) as { now_utc_ms: number; items: { input: string; yt: string; cal: string }[] };
  it("has the hostile and international inputs", () => {
    expect(fx.items.length).toBeGreaterThanOrEqual(10);
    expect(fx.items.some((i) => i.input.includes("\u2028")) && fx.items.some((i) => i.input.startsWith(" ")) && fx.items.some((i) => [...i.input].length === 100)).toBe(true);
  });
  it("every yt URL passes, and the decoded query is exactly the input", () => {
    for (const it of fx.items) {
      expect(new URL(it.yt).href, it.input).toBe(it.yt);
      expect(checkYtUrl(it.yt), `check ${JSON.stringify(it.input)}`).toBe(true);
      const r = parseCmd(msg({ op: "yt_play", url: it.yt }), fx.now_utc_ms) as any;
      expect(r.ok, `parse ${JSON.stringify(it.input)}`).toBe(true);
      expect(r.cmd.query, "the query round-trips unchanged").toBe(it.input);
    }
  });
  it("every cal URL passes with the Rust clock, and the title round-trips", () => {
    for (const it of fx.items) {
      expect(new URL(it.cal).href, it.input).toBe(it.cal);
      expect(checkCalUrl(it.cal, fx.now_utc_ms), `check ${JSON.stringify(it.input)}`).toBe(true);
      const r = parseCmd(msg({ op: "cal_open", url: it.cal }), fx.now_utc_ms) as any;
      expect(r.ok, `parse ${JSON.stringify(it.input)}`).toBe(true);
      expect(new URL(it.cal).searchParams.get("text")).toBe(it.input);
    }
  });
});
