// SPDX-License-Identifier: FSL-1.1-MIT
// Test 11: every file added/modified vs handy-v0.10.0-base carries an SPDX header,
// unless covered by root REUSE.toml or a `<file>.license` sidecar.
// Modified Handy files: MIT. New files: FSL-1.1-MIT.
import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { existsSync, openSync, readSync, closeSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { parse } from "smol-toml";

const ROOT = join(import.meta.dirname, "..", "..");
const BASE = "handy-v0.10.0-base";
const git = (...a: string[]) =>
  execFileSync("git", a, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });

function globToRe(g: string): RegExp {
  let re = "";
  for (let i = 0; i < g.length; i++) {
    const c = g[i];
    if (c === "*" && g[i + 1] === "*") {
      re += ".*";
      i++;
      if (g[i + 1] === "/") i++;
    } else if (c === "*") re += "[^/]*";
    else re += c.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${re}$`);
}

function reuseGlobs(): RegExp[] {
  const f = join(ROOT, "REUSE.toml");
  if (!existsSync(f)) return [];
  const t = parse(readFileSync(f, "utf8")) as { annotations?: { path: string | string[] }[] };
  return (t.annotations ?? []).flatMap((a) => (Array.isArray(a.path) ? a.path : [a.path])).map(globToRe);
}

function head(path: string): string {
  const fd = openSync(join(ROOT, path), "r");
  try {
    const buf = Buffer.alloc(2048);
    const n = readSync(fd, buf, 0, 2048, 0);
    return buf.subarray(0, n).toString("latin1").split("\n").slice(0, 12).join("\n");
  } finally {
    closeSync(fd);
  }
}

function changed(): { path: string; kind: "A" | "M" }[] {
  const out: { path: string; kind: "A" | "M" }[] = [];
  const baseFiles = new Set(git("ls-tree", "-r", "--name-only", BASE).split("\n").filter(Boolean));
  for (const line of git("diff", "--name-status", "-M", "--diff-filter=AMR", BASE).split("\n").filter(Boolean)) {
    const parts = line.split("\t");
    const st = parts[0][0];
    const path = parts[parts.length - 1];
    out.push({ path, kind: st === "A" ? "A" : "M" });
  }
  for (const p of git("ls-files", "--others", "--exclude-standard").split("\n").filter(Boolean)) {
    out.push({ path: p, kind: baseFiles.has(p) ? "M" : "A" });
  }
  return out;
}

describe("SPDX headers vs base tag", () => {
  it("base tag exists", () => {
    expect(git("rev-parse", "--verify", `${BASE}^{commit}`).trim()).toMatch(/^[0-9a-f]{40}$/);
  });
  it("REUSE.toml exists (declares unchanged Handy files MIT)", () => {
    expect(existsSync(join(ROOT, "REUSE.toml"))).toBe(true);
  });
  it("every added/modified file has a header, REUSE entry or .license sidecar", () => {
    const globs = reuseGlobs();
    const missing: string[] = [];
    for (const { path, kind } of changed()) {
      if (!existsSync(join(ROOT, path))) continue;
      if (globs.some((g) => g.test(path))) continue;
      if (existsSync(join(ROOT, `${path}.license`))) continue;
      const h = head(path);
      const m = /SPDX-License-Identifier:\s*([A-Za-z0-9.\-+ ]+)/.exec(h);
      if (!m) {
        missing.push(`${path} (${kind}): no SPDX header`);
        continue;
      }
      const id = m[1].trim();
      if (kind === "A" && id !== "FSL-1.1-MIT") missing.push(`${path}: new file must be FSL-1.1-MIT, found ${id}`);
      if (kind === "M" && id !== "MIT") missing.push(`${path}: modified Handy file must be MIT, found ${id}`);
    }
    expect(missing, missing.slice(0, 20).join("\n")).toEqual([]);
  });
});
