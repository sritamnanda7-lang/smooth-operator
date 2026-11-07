// SPDX-License-Identifier: FSL-1.1-MIT
// Spike service worker: connect to local app, hello/ack, retry every 5 s.
const APP_URL = "ws://127.0.0.1:51717";
const RETRY_MS = 5000;
let ws = null;
let timer = null;

async function setStatus(connected, error) {
  await chrome.storage.session.set({
    connected,
    lastError: error || "",
    ts: new Date().toISOString(),
  });
}

function isAck(m) {
  return (
    m !== null &&
    typeof m === "object" &&
    !Array.isArray(m) &&
    Object.keys(m).length === 2 &&
    m.type === "ack" &&
    m.app === "so-spike"
  );
}

function scheduleRetry() {
  if (timer === null) {
    timer = setTimeout(() => {
      timer = null;
      connect();
    }, RETRY_MS);
  }
}

function connect() {
  if (ws && (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN)) return;
  let sock;
  try {
    sock = new WebSocket(APP_URL);
  } catch (e) {
    setStatus(false, "connect failed: " + String(e && e.message));
    scheduleRetry();
    return;
  }
  ws = sock;
  sock.onopen = () => {
    sock.send(JSON.stringify({ type: "hello", ext: "so-spike" }));
  };
  sock.onmessage = (ev) => {
    if (typeof ev.data !== "string" || ev.data.length > 1024) return;
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch (_) {
      return;
    }
    if (isAck(msg)) setStatus(true, "");
  };
  sock.onerror = () => {
    setStatus(false, "socket error (app not running, or blocked by firewall / local network prompt)");
  };
  sock.onclose = () => {
    if (ws === sock) ws = null;
    setStatus(false, null);
    scheduleRetry();
  };
}

chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
connect();
