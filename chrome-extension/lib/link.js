// SPDX-License-Identifier: FSL-1.1-MIT
// All side effects of the link: the socket, storage, alarms, timers. The service worker only calls
// startLink(). The state machine (state.js) decides; this file does what it says. Everything the app
// sends is untrusted until it parses and, for auth_ok, carries the right proof.
import { ALARM, ALARM_MIN, PING_MS, WS_URL } from "./config.js";
import { delayFor } from "./backoff.js";
import { authMsg, isCode, isHex64, pairMsg, parseApp, pingMsg, R, resMsg } from "./protocol.js";
import { clientProof, eqHex, newNonce, serverProof } from "./proof.js";
import { next, S } from "./state.js";

const OPENING = 0;
const OPEN = 1;

/** @returns {{stop: () => void}} */
export function startLink(env) {
  const { chrome, WebSocket, setTimeout, setInterval, clearTimeout, clearInterval, tasks } = env;
  const store = chrome.storage.local;
  let state = S.UNPAIRED;
  let retryS;
  let cur = null; // the one live socket and what we have sent on it
  let pingTimer = null;
  let retryTimer = null;
  let attempt = 0;
  let stopped = false;

  const persist = () => {
    const snapshot = state === S.LOCKED ? { state, retry_s: retryS } : { state };
    chrome.storage.local.set({ link: snapshot }).catch(() => {});
  };
  const readToken = async () => {
    const got = await store.get("token");
    return isHex64(got.token) ? got.token : null;
  };

  const stopPing = () => {
    if (pingTimer !== null) clearInterval(pingTimer);
    pingTimer = null;
  };
  const closeCur = () => {
    stopPing();
    const rec = cur;
    cur = null;
    if (rec) tasks?.cancelAll(); // a job belongs to the socket that asked for it
    try {
      rec?.ws.close();
    } catch {
      // already closed
    }
  };

  function connect(code) {
    if (cur && (cur.ws.readyState === OPENING || cur.ws.readyState === OPEN)) return;
    if (retryTimer !== null) clearTimeout(retryTimer);
    retryTimer = null;
    const ws = new WebSocket(WS_URL);
    const rec = { ws, pending: code, gotHello: false, sentAuth: false, sentPair: false, expectApp: "" };
    cur = rec;
    ws.onmessage = (e) => onMessage(rec, e.data);
    ws.onclose = () => onEnd(rec, { type: "closed" });
    ws.onerror = () => onEnd(rec, { type: "error" });
  }

  const onEnd = (rec, ev) => {
    if (stopped || rec !== cur) return;
    cur = null;
    tasks?.cancelAll();
    void dispatch(ev);
  };

  async function sendAuth(rec, nonce) {
    const token = await readToken();
    if (stopped || rec !== cur) return;
    if (token === null) return void dispatch({ type: "rejected" });
    const cnonce = newNonce();
    const proof = await clientProof(token, nonce);
    rec.expectApp = await serverProof(token, cnonce);
    if (stopped || rec !== cur) return;
    rec.sentAuth = true;
    try {
      rec.ws.send(authMsg(proof, cnonce));
    } catch {
      // the socket closed under us: its close event handles it
    }
  }

  async function run(effects, ev) {
    for (const fx of effects) {
      switch (fx) {
        case "connect":
          connect(ev.code);
          break;
        case "send_pair":
          if (cur) {
            cur.sentPair = true;
            try {
              cur.ws.send(pairMsg(cur.pending));
            } catch {
              // closed: handled by its close event
            }
          }
          break;
        case "send_auth":
          if (cur) void sendAuth(cur, ev.nonce);
          break;
        case "save_token":
          await chrome.storage.local.set({ token: ev.token });
          break;
        case "close":
          closeCur();
          break;
        case "retry_quick": {
          const wait = delayFor(attempt);
          if (wait !== null) {
            attempt += 1;
            if (retryTimer !== null) clearTimeout(retryTimer);
            retryTimer = setTimeout(() => void kick(), wait);
          }
          break;
        }
        case "start_ping":
          stopPing();
          pingTimer = setInterval(sendPing, PING_MS);
          break;
        case "stop_ping":
          stopPing();
          break;
      }
    }
  }

  function send(rec, text) {
    if (stopped || rec !== cur || rec.ws.readyState !== OPEN) return;
    try {
      rec.ws.send(text);
    } catch {
      // closed: handled by its close event
    }
  }

  /** The answer goes back only on the socket that asked, and only while it is still the current one. */
  async function runCmd(rec, cmd) {
    let out;
    try {
      out = await tasks.run(cmd);
    } catch {
      out = { r: R.error, ms: 0 };
    }
    send(rec, resMsg(cmd.id, out.r, out.ms));
  }

  function sendPing() {
    if (!cur || cur.ws.readyState !== OPEN || state !== S.CONNECTED) return stopPing();
    try {
      cur.ws.send(pingMsg());
    } catch {
      // closed: handled by its close event
    }
  }

  function dispatch(ev) {
    const r = next(state, ev);
    const changed = r.state !== state || ev.type === "locked";
    state = r.state;
    if (ev.type === "locked") retryS = ev.retry_s;
    if (changed) persist();
    return run(r.effects, ev);
  }

  function onMessage(rec, data) {
    if (stopped) return;
    if (rec !== cur) {
      // A late message from a socket we already dropped: ignore it and make sure it is closed.
      try {
        rec.ws.close();
      } catch {
        // already closed
      }
      return;
    }
    const parsed = parseApp(data);
    const reject = () => void dispatch({ type: "rejected" });
    const live = state === S.CONNECTED && rec.gotHello && tasks !== undefined; // commands only after auth
    if (!parsed.ok) {
      // A bad command with a usable id gets a bad_cmd answer; without one it is ignored. The link stays up.
      if (!parsed.cmd || !live) return reject();
      if (parsed.id !== undefined) send(rec, resMsg(parsed.id, R.bad_cmd, 0));
      return;
    }
    const m = parsed.msg;
    if (m.t === "cmd") return live ? void runCmd(rec, m) : reject();
    if (m.t === "cancel") return live ? tasks.cancel(m.id) : reject();
    if (m.t === "pong") return state === S.CONNECTED ? undefined : reject();
    if (m.state === "hello") {
      if (rec.gotHello) return reject();
      rec.gotHello = true;
      return void dispatch({ type: "hello", nonce: m.nonce });
    }
    if (!rec.gotHello) return reject();
    switch (m.state) {
      case "auth_ok":
        if (!rec.sentAuth || !eqHex(m.proof, rec.expectApp)) return reject();
        attempt = 0;
        return void dispatch({ type: "authOk", proof: m.proof });
      case "paired":
        return rec.sentPair ? void dispatch({ type: "paired", token: m.token }) : reject();
      case "wrong_code":
        return void dispatch({ type: "wrongCode" });
      case "locked":
        return void dispatch({ type: "locked", retry_s: m.retry_s });
      case "expired":
        return void dispatch({ type: "expired" });
      case "not_pairing":
        return void dispatch({ type: "notPairing" });
      default:
        return reject();
    }
  }

  /** Start, alarm tick, quick retry: connect if there is a token and no socket is opening or open. */
  async function kick() {
    if (stopped) return;
    const token = await readToken();
    if (!stopped) await dispatch({ type: "start", hasToken: token !== null });
  }

  /** Only our own popup, only these two exact shapes. */
  const fromPopup = (msg, sender) => {
    if (!sender || sender.id !== chrome.runtime.id || sender.tab) return null;
    // Later content scripts share our id: only the popup page itself may talk to us.
    if (sender.url !== chrome.runtime.getURL("popup.html")) return null;
    if (msg === null || typeof msg !== "object" || Array.isArray(msg)) return null;
    const keys = Object.keys(msg).sort().join(",");
    if (msg.t === "pair" && keys === "code,t" && isCode(msg.code)) return msg;
    if (msg.t === "retry" && keys === "t") return msg;
    return null;
  };

  chrome.runtime.onStartup.addListener(() => void kick());
  chrome.runtime.onInstalled.addListener(() => void kick());
  chrome.alarms.onAlarm.addListener((a) => {
    if (a && a.name === ALARM) void kick();
  });
  chrome.runtime.onMessage.addListener((msg, sender) => {
    const m = stopped ? null : fromPopup(msg, sender);
    if (m?.t === "pair") {
      void dispatch({ type: "submit", code: m.code });
    } else if (m?.t === "retry") {
      void readToken().then((token) => dispatch(token === null ? { type: "start", hasToken: false } : { type: "retry" }));
    }
    return false;
  });

  chrome.alarms.create(ALARM, { periodInMinutes: ALARM_MIN });
  persist();
  void kick();

  return {
    stop() {
      stopped = true;
      if (retryTimer !== null) clearTimeout(retryTimer);
      closeCur();
    },
  };
}
