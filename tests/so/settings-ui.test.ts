// SPDX-License-Identifier: FSL-1.1-MIT
// V5: Settings sidebar has no Advanced pane; GeneralPane imports only our items + MicrophoneSelector + ModelSettingsCard.
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(import.meta.dirname, "..", "..", "desktop", "src");
const read = (rel: string) => readFileSync(join(SRC, rel), "utf8");

describe("V5 sidebar", () => {
  it("has no advanced entry, import or label", () => {
    const t = read("components/Sidebar.tsx");
    expect(t).not.toMatch(/advanced/i);
  });
  it("still has general, models, about, debug", () => {
    const t = read("components/Sidebar.tsx");
    for (const k of ["general", "models", "about", "debug"]) expect(t, k).toMatch(new RegExp(`\\b${k}\\b`));
    expect(t).toContain("so/settings/GeneralPane");
  });
});

describe("V5 GeneralPane imports", () => {
  const t = read("so/settings/GeneralPane.tsx");
  const specs = [...t.matchAll(/(?:from\s+|import\s+)["']([^"']+)["']/g)].map((m) => m[1]);
  const ALLOWED_BASENAMES = new Set([
    "react",
    "HotkeySetting",
    "FnCheckNotice",
    "fnCheckView",
    "strings",
    "MicrophoneSelector",
    "ModelSettingsCard",
    "SettingsGroup",
  ]);
  it("imports only allowed modules", () => {
    expect(specs.length).toBeGreaterThan(0);
    for (const s of specs) {
      const base = s.split("/").pop()!;
      expect(ALLOWED_BASENAMES.has(base), `unexpected import: ${s}`).toBe(true);
    }
  });
  it("no other Handy settings item is imported", () => {
    for (const s of specs) {
      if (/components\/settings\//.test(s) || /^\.\.\/(\.\.\/)?components\/settings/.test(s)) {
        expect(/MicrophoneSelector$|ModelSettingsCard$/.test(s), s).toBe(true);
      }
    }
  });
  it("includes MicrophoneSelector, ModelSettingsCard and our HotkeySetting", () => {
    for (const n of ["MicrophoneSelector", "ModelSettingsCard", "HotkeySetting"]) expect(specs.some((s) => s.endsWith(n)), n).toBe(true);
  });
});
