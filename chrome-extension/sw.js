// SPDX-License-Identifier: FSL-1.1-MIT
// The extension's service worker: it owns the socket to the app. All logic is in lib/link.js and lib/tasks.js;
// this file only hands them the browser's objects. Listeners must be set up at the top level, so it runs at once.
import { startLink } from "./lib/link.js";
import { makeTasks } from "./lib/tasks.js";

// Only the extension's own pages may read its storage (the token lives there). Wrapped so a missing function or
// a synchronous throw becomes a rejection, never a dead worker; every command waits for it and answers `unsafe` if it failed.
const ready = Promise.resolve().then(() => chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" }));
ready.catch(() => {}); // handled here so a failure is not an "unhandled rejection"; tasks.js reads the same promise

// Wrapped: the browser's timer functions throw if they are called as methods of another object.
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

startLink({
  chrome,
  WebSocket,
  setTimeout: (f, ms) => setTimeout(f, ms),
  setInterval: (f, ms) => setInterval(f, ms),
  clearTimeout: (id) => clearTimeout(id),
  clearInterval: (id) => clearInterval(id),
  tasks: makeTasks({ chrome, ready, now: () => Date.now(), sleep }),
});
