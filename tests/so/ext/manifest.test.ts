// SPDX-License-Identifier: FSL-1.1-MIT
// Step 5 E10 (extra): the manifest `key` really gives the allowed Origin's extension ID. Chrome derives the ID as the
// first 16 bytes of SHA-256(DER public key), each hex digit mapped 0-9a-f -> a-p. If this fails, L3 (outcome 20) fails.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = join(import.meta.dirname, "..", "..", "..");
const ID = "annldiclndpcpopgmdmfcdjdeehfdlik";

describe("E10 manifest key -> extension ID", () => {
  it("equals the ID in ALLOWED_ORIGIN and in the spike result", () => {
    const m = JSON.parse(readFileSync(join(ROOT, "chrome-extension/manifest.json"), "utf8"));
    const der = Buffer.from(m.key, "base64");
    const id = [...createHash("sha256").update(der).digest().subarray(0, 16)].map((b) => (b >> 4).toString(16) + (b & 15).toString(16)).join("").replace(/[0-9a-f]/g, (c) => String.fromCharCode("a".charCodeAt(0) + parseInt(c, 16)));
    expect(id).toBe(ID);
    const rust = readFileSync(join(ROOT, "desktop/src-tauri/src/so/link/mod.rs"), "utf8");
    expect(rust).toContain(`pub const ALLOWED_ORIGIN: &str = "chrome-extension://${id}";`);
  });
});

// ---------------- step 6 E13: the manifest is exactly the approved permission set ----------------
describe("E13 manifest permissions (step 6)", () => {
  const m = JSON.parse(readFileSync(join(ROOT, "chrome-extension/manifest.json"), "utf8"));
  it("storage, alarms, scripting and exactly three hosts", () => {
    expect(m.permissions).toEqual(["storage", "alarms", "scripting"]);
    expect([...m.host_permissions].sort()).toEqual(["https://calendar.google.com/*", "https://docs.google.com/*", "https://www.youtube.com/*"]);
  });
  it("no content scripts, tabs, activeTab, all-urls, web-accessible resources or externally_connectable", () => {
    for (const k of ["content_scripts", "web_accessible_resources", "externally_connectable"]) expect(m[k]).toBeUndefined();
    const all = JSON.stringify(m);
    for (const bad of ['"tabs"', "activeTab", "<all_urls>"]) expect(all).not.toContain(bad);
  });
  it("minimum_chrome_version is a number >= 116 (the founder sets the final value after Z5)", () => {
    const n = Number(m.minimum_chrome_version);
    expect(Number.isInteger(n) && n >= 116).toBe(true);
  });
});
