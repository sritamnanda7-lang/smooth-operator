// SPDX-License-Identifier: FSL-1.1-MIT
// V4: Fn check messages and the "Use Right Cmd" offer appear only when the check applies.
import { describe, expect, it } from "vitest";
import { fnCheckView, type FnCheckReport } from "../../desktop/src/so/settings/fnCheckView";

const base: FnCheckReport = { applies: true, globe: "Ok", dictation: "Ok", apps: [] };

describe("fnCheckView", () => {
  it("shows nothing when the check does not apply, whatever the report says", () => {
    const v = fnCheckView({ applies: false, globe: { Warn: "Show Emoji" }, dictation: "Warn", apps: ["Perplexity"] });
    expect(v.messages).toEqual([]);
    expect(v.offerRightCmd).toBe(false);
  });

  it("shows nothing and no offer when everything is fine", () => {
    const v = fnCheckView(base);
    expect(v.messages).toEqual([]);
    expect(v.offerRightCmd).toBe(false);
  });

  it("globe warning gives a message and the Right Cmd offer", () => {
    const v = fnCheckView({ ...base, globe: { Warn: "Show Emoji" } });
    expect(v.messages.length).toBeGreaterThan(0);
    expect(v.messages.join(" ")).toContain("Show Emoji");
    expect(v.offerRightCmd).toBe(true);
  });

  it("dictation warning gives a message and the offer", () => {
    const v = fnCheckView({ ...base, dictation: "Warn" });
    expect(v.messages.length).toBeGreaterThan(0);
    expect(v.messages.join(" ").toLowerCase()).toContain("dictation");
    expect(v.offerRightCmd).toBe(true);
  });

  it("named apps appear in a message and trigger the offer", () => {
    const v = fnCheckView({ ...base, apps: ["Perplexity", "Wispr Flow"] });
    const text = v.messages.join(" ");
    expect(text).toContain("Perplexity");
    expect(text).toContain("Wispr Flow");
    expect(v.offerRightCmd).toBe(true);
  });

  it("unknown results alone are not a warning and give no offer", () => {
    const v = fnCheckView({ ...base, globe: "Unknown", dictation: "Unknown" });
    expect(v.offerRightCmd).toBe(false);
  });

  it("messages are plain strings (rendered as text, never as HTML)", () => {
    const v = fnCheckView({ ...base, apps: ["<b>x</b>"] });
    for (const m of v.messages) expect(typeof m).toBe("string");
  });
});
