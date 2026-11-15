// SPDX-License-Identifier: FSL-1.1-MIT
// Message builders and the parser for what the app sends. Everything the app sends is untrusted until
// it parses here: exact fields, exact formats, version 1, at most 4,096 characters.
import { MAX_MSG } from "./config.js";

const HEX64 = /^[0-9a-f]{64}$/;
const isHex64 = (s) => typeof s === "string" && HEX64.test(s);

/** Extra field each `status` state carries, with its validator. */
const FIELD = {
  hello: ["nonce", isHex64],
  auth_ok: ["proof", isHex64],
  paired: ["token", isHex64],
  locked: ["retry_s", (n) => Number.isInteger(n) && n >= 0],
  wrong_code: null,
  not_pairing: null,
  expired: null,
  rejected: null,
};

const sameKeys = (obj, keys) => {
  const have = Object.keys(obj).sort();
  return have.length === keys.length && [...keys].sort().every((k, i) => k === have[i]);
};

/** @returns {{ok: true, msg: object} | {ok: false}} */
export function parseApp(text) {
  const no = { ok: false };
  if (typeof text !== "string" || text.length > MAX_MSG) return no;
  let m;
  try {
    m = JSON.parse(text);
  } catch {
    return no;
  }
  if (m === null || typeof m !== "object" || Array.isArray(m) || m.v !== 1) return no;
  if (m.t === "pong") return sameKeys(m, ["t", "v"]) ? { ok: true, msg: { t: "pong", v: 1 } } : no;
  if (m.t !== "status" || typeof m.state !== "string" || !Object.hasOwn(FIELD, m.state)) return no;
  const f = FIELD[m.state];
  if (f === null) return sameKeys(m, ["t", "v", "state"]) ? { ok: true, msg: { t: "status", v: 1, state: m.state } } : no;
  const [name, valid] = f;
  if (!sameKeys(m, ["t", "v", "state", name]) || !valid(m[name])) return no;
  return { ok: true, msg: { t: "status", v: 1, state: m.state, [name]: m[name] } };
}

export function pairMsg(code) {
  return JSON.stringify({ t: "pair", v: 1, code });
}
export function authMsg(proof, cnonce) {
  return JSON.stringify({ t: "auth", v: 1, proof, cnonce });
}
export function pingMsg() {
  return JSON.stringify({ t: "ping", v: 1 });
}
/** Exactly 6 ASCII digits. */
export function isCode(s) {
  return typeof s === "string" && /^[0-9]{6}$/.test(s);
}
export { isHex64 };
