// SPDX-License-Identifier: FSL-1.1-MIT
// Test 12: no post-processing/history UI imports; no visible "Handy" in English strings.
import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dirname, "..", "..");
const SRC = join(ROOT, "desktop", "src");

function walk(dir: string, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  for (const n of readdirSync(dir)) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) {
      if (n !== "locales" && n !== "node_modules") walk(p, out);
    } else out.push(p);
  }
  return out;
}

describe("UI strip", () => {
  it("no import/dynamic import of post-processing or history modules", () => {
    const re = /(?:from\s+|import\s*\(\s*|import\s+)["']([^"']+)["']/g;
    const bad: string[] = [];
    for (const f of walk(SRC).filter((x) => /\.(ts|tsx)$/.test(x))) {
      const t = readFileSync(f, "utf8");
      for (const m of t.matchAll(re)) {
        if (/post-?processing|PostProcess|history|transcriptionJobs|update-checker|whats-new/i.test(m[1]))
          bad.push(`${relative(ROOT, f)}: ${m[1]}`);
      }
    }
    expect(bad, bad.slice(0, 15).join("\n")).toEqual([]);
  });

  it("post-processing / history UI directories are gone", () => {
    const dirs = ["components/settings/post-processing", "components/settings/history", "components/update-checker", "components/whats-new"];
    expect(dirs.filter((d) => existsSync(join(SRC, d)))).toEqual([]);
  });

  it("English locale has no visible 'Handy' string", () => {
    const f = join(SRC, "i18n", "locales", "en", "translation.json");
    const hits: string[] = [];
    const visit = (v: unknown, path: string) => {
      if (typeof v === "string") {
        if (/handy/i.test(v)) hits.push(`${path}: ${v.slice(0, 60)}`);
      } else if (v && typeof v === "object") {
        for (const [k, c] of Object.entries(v)) visit(c, `${path}.${k}`);
      }
    };
    visit(JSON.parse(readFileSync(f, "utf8")), "en");
    expect(hits, hits.slice(0, 15).join("\n")).toEqual([]);
  });
});
