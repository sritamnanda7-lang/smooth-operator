// SPDX-License-Identifier: FSL-1.1-MIT
// Quick retries after the app is gone: 1 s, 2 s, 4 s, then null (only the 30 s alarm brings it back).
const DELAYS = [1000, 2000, 4000];

/** @param {number} attempt 0 for the first retry */
export function delayFor(attempt) {
  return DELAYS[attempt] ?? null;
}
