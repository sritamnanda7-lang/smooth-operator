// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E4 (pure half): chrome-extension/lib/backoff.js. Attempts are counted from 0 (the first retry is attempt 0).
import { describe, expect, it } from "vitest";
import { delayFor } from "../../../chrome-extension/lib/backoff.js";

describe("E4 delayFor", () => {
  it("1 s, 2 s, 4 s, then null (wait for the alarm)", () => {
    expect(delayFor(0)).toBe(1000);
    expect(delayFor(1)).toBe(2000);
    expect(delayFor(2)).toBe(4000);
    expect(delayFor(3)).toBeNull();
    expect(delayFor(4)).toBeNull();
    expect(delayFor(100)).toBeNull();
  });
});
