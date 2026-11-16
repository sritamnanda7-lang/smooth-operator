// SPDX-License-Identifier: FSL-1.1-MIT
// Validates a command from the app before anything runs. The app is trusted only as far as this file:
// exact origin and path, exact query keys in order, lengths in code points, no control characters.
import { MAX_DAYS, MAX_ID } from "./config.js";

const YT_PREFIX = "https://www.youtube.com/results?";
const CAL_PREFIX = "https://calendar.google.com/calendar/render?";
const DATES = /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z\/(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/;
const MIN = 60_000;
const DAY = 24 * 60 * MIN;
const CONTROL = /[\u0000-\u001f\u007f]/;

/** Length in code points (an emoji is 1, like Rust chars().count(); JS .length would say 2). */
export function cpLen(s) {
  return [...s].length;
}

/** Entries of the query of `s` (already known to start with `prefix`), or null if it does not parse as plain https. */
function entries(s, prefix) {
  if (typeof s !== "string" || !s.startsWith(prefix) || s.includes("#")) return null;
  try {
    const u = new URL(s);
    if (u.href !== s) return null; // canonical form only: no raw space/tab/newline, odd case, :443, /./ or encoded keys
    if (u.username !== "" || u.password !== "" || u.port !== "") return null;
    const e = [...u.searchParams.entries()];
    // keys must be spelled plainly (not %73earch_query)
    return s.slice(prefix.length).split("&").every((kv, i) => kv.split("=")[0] === e[i]?.[0]) ? e : null;
  } catch {
    return null;
  }
}
const goodText = (v, max) => typeof v === "string" && cpLen(v) >= 1 && cpLen(v) <= max && !CONTROL.test(v);

export function checkYtUrl(s) {
  const e = entries(s, YT_PREFIX);
  return e !== null && e.length === 1 && e[0][0] === "search_query" && goodText(e[0][1], 120);
}

const ms = (m, o) => Date.UTC(+m[o], +m[o + 1] - 1, +m[o + 2], +m[o + 3], +m[o + 4], +m[o + 5]);
// A real calendar moment: Date.UTC rolls month 13 over, so read it back.
const real = (m, o) => new Date(ms(m, o)).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z") === `${m[o]}${m[o + 1]}${m[o + 2]}T${m[o + 3]}${m[o + 4]}${m[o + 5]}Z`;

export function checkCalUrl(s, nowMs) {
  const e = entries(s, CAL_PREFIX);
  if (e === null || e.length !== 3 || !s.startsWith(`${CAL_PREFIX}action=TEMPLATE&text=`)) return false; // the fixed word spelled plainly
  if (e[0][0] !== "action" || e[0][1] !== "TEMPLATE" || e[1][0] !== "text" || e[2][0] !== "dates") return false;
  if (!goodText(e[1][1], 100)) return false;
  const m = DATES.exec(e[2][1]);
  if (!m || !real(m, 1) || !real(m, 7)) return false;
  const start = ms(m, 1);
  const end = ms(m, 7);
  return end - start >= 5 * MIN && end - start <= 480 * MIN && start >= nowMs - DAY && start <= nowMs + MAX_DAYS * DAY;
}

const sameKeys = (o, keys) => {
  const have = Object.keys(o).sort();
  return have.length === keys.length && [...keys].sort().every((k, i) => k === have[i]);
};

/** @returns {{ok: true, cmd: {id: number, op: string, url?: string, query?: string}} | {ok: false, id?: number}} */
export function parseCmd(obj, nowMs) {
  if (obj === null || typeof obj !== "object" || Array.isArray(obj)) return { ok: false };
  const id = Number.isInteger(obj.id) && obj.id >= 1 && obj.id <= MAX_ID ? obj.id : undefined;
  const no = { ok: false, id };
  if (id === undefined || obj.t !== "cmd" || obj.v !== 1) return no;
  if (obj.op === "docs_open") return sameKeys(obj, ["t", "v", "id", "op"]) ? { ok: true, cmd: { id, op: "docs_open" } } : no;
  if (!sameKeys(obj, ["t", "v", "id", "op", "url"])) return no;
  if (obj.op === "yt_play" && checkYtUrl(obj.url)) return { ok: true, cmd: { id, op: "yt_play", url: obj.url, query: new URL(obj.url).searchParams.get("search_query") } };
  if (obj.op === "cal_open" && checkCalUrl(obj.url, nowMs)) return { ok: true, cmd: { id, op: "cal_open", url: obj.url } };
  return no;
}
