// SPDX-License-Identifier: FSL-1.1-MIT
// The popup: shows the state kept by the service worker and sends two messages to it: pair (the 6-digit
// code, never stored here) and retry. Text only via textContent.
import { isCode } from "./lib/protocol.js";
import { popupView } from "./lib/view.js";

const statusEl = document.getElementById("status");
const pairRow = document.getElementById("pair-row");
const digits = document.getElementById("digits");
const retryBtn = document.getElementById("retry");

function render(link) {
  const v = popupView(link);
  statusEl.textContent = v.text;
  statusEl.classList.toggle("fix", v.fix);
  pairRow.hidden = !v.showCode;
  retryBtn.hidden = !v.showRetry;
}

document.getElementById("pair").addEventListener("click", () => {
  const typed = digits.value;
  digits.value = "";
  if (isCode(typed)) void chrome.runtime.sendMessage({ t: "pair", code: typed });
});
retryBtn.addEventListener("click", () => void chrome.runtime.sendMessage({ t: "retry" }));

chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.link) render(changes.link.newValue);
});
chrome.storage.local.get("link").then((got) => render(got.link));
