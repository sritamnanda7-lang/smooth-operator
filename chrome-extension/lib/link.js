// SPDX-License-Identifier: FSL-1.1-MIT
// All side effects of the link: the socket, storage, alarms, timers. The service worker only calls
// this. Stub until the tests exist.

/** @returns {{stop: () => void}} */
export function startLink(_env) {
  throw new Error("not implemented");
}
