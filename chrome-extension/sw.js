// SPDX-License-Identifier: FSL-1.1-MIT
// The extension's service worker: it owns the socket to the app. All logic is in lib/link.js; this file
// only hands it the browser's objects. Listeners must be set up at the top level, so it runs at once.
import { startLink } from "./lib/link.js";

startLink({
  chrome,
  WebSocket,
  // Wrapped: the browser's timer functions throw if they are called as methods of another object.
  setTimeout: (f, ms) => setTimeout(f, ms),
  setInterval: (f, ms) => setInterval(f, ms),
  clearTimeout: (id) => clearTimeout(id),
  clearInterval: (id) => clearInterval(id),
});
