// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E6: chrome-extension/lib/view.js. Popup text per state; the texts are the plan's, character for character.
import { describe, expect, it } from "vitest";
import { popupView } from "../../../chrome-extension/lib/view.js";
import { S } from "../../../chrome-extension/lib/state.js";

const v = (state: string, retry_s?: number) => popupView({ state, retry_s } as never);

describe("E6 popupView text", () => {
  it("one fixed text per state", () => {
    expect(v(S.CONNECTED).text).toBe("Connected to Smooth Operator");
    expect(v(S.UNPAIRED).text).toBe("Not paired. In Smooth Operator: Settings → General → Chrome extension → Pair, then type the code here.");
    expect(v(S.APP_NOT_FOUND).text).toBe("Fix connection: Smooth Operator isn't running. Open it, then click Retry.");
    expect(v(S.REJECTED).text).toBe("Fix connection: pair again");
    expect(v(S.WRONG_CODE).text).toBe("Wrong code: check Smooth Operator");
    expect(v(S.EXPIRED).text).toBe("Code expired: click Pair in Smooth Operator for a new code");
    expect(v(S.LOCKED, 42).text).toBe("Too many tries: wait 42 s, then click Pair in the app again");
  });
  it("Locked never prints undefined or NaN", () => {
    for (const r of [undefined, NaN as number]) expect(v(S.LOCKED, r).text).not.toMatch(/undefined|NaN/);
  });
  it("PAIRING and CONNECTING have a non-empty text of their own (the plan does not word them)", () => {
    for (const s of [S.PAIRING, S.CONNECTING]) {
      expect(typeof v(s).text).toBe("string");
      expect(v(s).text.length).toBeGreaterThan(0);
    }
    expect(v(S.PAIRING).text).not.toBe(v(S.CONNECTED).text);
  });
});

describe("E6 flags", () => {
  it("fix is true for APP_NOT_FOUND and REJECTED only", () => {
    for (const s of Object.values(S) as string[]) expect(v(s, 5).fix, s).toBe(s === S.APP_NOT_FOUND || s === S.REJECTED);
  });
  it("the code field shows only in UNPAIRED, REJECTED, WRONG_CODE and EXPIRED", () => {
    const want = new Set<string>([S.UNPAIRED, S.REJECTED, S.WRONG_CODE, S.EXPIRED]);
    for (const s of Object.values(S) as string[]) expect(v(s, 5).showCode, s).toBe(want.has(s));
  });
  it("Retry shows only in APP_NOT_FOUND", () => {
    for (const s of Object.values(S) as string[]) expect(v(s, 5).showRetry, s).toBe(s === S.APP_NOT_FOUND);
  });
  it("a missing or unknown link (storage not written yet) does not throw and reads as a string", () => {
    for (const l of [undefined, null, {}, { state: "NOPE" }]) {
      const r = popupView(l as never);
      expect(typeof r.text).toBe("string");
      expect(r.text.length).toBeGreaterThan(0);
    }
  });
});
