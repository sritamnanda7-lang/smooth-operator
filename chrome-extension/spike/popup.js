// SPDX-License-Identifier: FSL-1.1-MIT
const $ = (id) => document.getElementById(id);

async function render() {
  const s = await chrome.storage.session.get(["connected", "lastError", "ts"]);
  $("status").textContent = s.connected === true ? "connected" : "not connected";
  $("error").textContent = s.lastError || "-";
  $("ts").textContent = s.ts || "-";
  $("chrome").textContent = navigator.userAgent;
}
render();
chrome.storage.onChanged.addListener(render);
