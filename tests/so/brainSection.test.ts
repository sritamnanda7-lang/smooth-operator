// SPDX-License-Identifier: FSL-1.1-MIT
// @vitest-environment happy-dom
// Step 4 U1-U9: the AI settings group (desktop/src/so/settings/BrainSection.tsx). The IPC commands are mocked
// (tests/so/support/bindingsMock.ts stands in for "@/bindings"); no network, no Keychain, no Tauri.
//
// HOW IT RUNS: the repo's vitest.config.ts has no "@" alias, so this file starts its own small Vite server
// (inline config, alias only) and loads the real component through it, then renders it with react-dom in
// happy-dom. Assumptions about the component (see the report): it renders ONE control set for the selected
// provider (status, one password input, Save, Remove key, Check key, Get a key, Set a spend limit), it reads
// its data through `commands.soBrain*` from "@/bindings", and it needs nothing but that module mocked.
// Sentinels are real words (a key that looks like a key, and a word in error text).
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { createServer, type ViteDevServer } from "vite";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..", "..");
const SRC = join(ROOT, "desktop", "src");
const KEY = "AIzaSyPumpernickelWalrus0123456789";
const KEY_WORD = "pumpernickel";

type Cmd = ReturnType<typeof vi.fn>;
type Provider = "claude" | "gemini";
type Row = { id: Provider; label: string; recommended: boolean; key_saved: boolean; model: string };
type World = {
  provider: Provider;
  rows: Row[];
  check: string;
  saveResult: () => Promise<unknown>;
  saveDelayMs: number;
  checkDelayMs: number;
  extra: boolean;
};
const ok = { status: "ok", data: null };

function newWorld(over: Partial<World> = {}): World {
  return {
    provider: "gemini",
    rows: [
      { id: "claude", label: "Claude", recommended: true, key_saved: false, model: "claude-haiku-5-5" },
      { id: "gemini", label: "Gemini", recommended: false, key_saved: false, model: "gemini-3.5-flash-lite" },
    ],
    check: "Ok",
    saveResult: async () => ok,
    saveDelayMs: 0,
    checkDelayMs: 0,
    extra: false,
    ...over,
  };
}

let server: ViteDevServer;
let React: typeof import("react");
let createRoot: typeof import("react-dom/client").createRoot;
let Section: () => unknown;
let GeneralModuleText = "";
let root: ReturnType<typeof createRoot> | undefined;
let host: HTMLElement | undefined;
let world: World;
let cmds: Record<string, Cmd>;
let consoleSpies: Cmd[] = [];

beforeAll(async () => {
  server = await createServer({
    configFile: false,
    root: ROOT,
    logLevel: "silent",
    appType: "custom",
    server: { middlewareMode: true, hmr: false, watch: null },
    esbuild: { jsx: "automatic" },
    resolve: {
      alias: [
        { find: "@/bindings", replacement: join(ROOT, "tests/so/support/bindingsMock.ts") },
        { find: /^@\//, replacement: SRC + "/" },
      ],
    },
  });
  React = (await import("../../desktop/node_modules/react/index.js")).default as unknown as typeof import("react");
  createRoot = (await import("../../desktop/node_modules/react-dom/client.js")).createRoot;
  (globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  GeneralModuleText = readFileSync(join(SRC, "so/settings/GeneralPane.tsx"), "utf8");
});
afterAll(async () => {
  await server?.close();
});
afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  root = undefined;
  host?.remove();
  host = undefined;
  consoleSpies.forEach((s) => s.mockRestore());
  consoleSpies = [];
  localStorage.clear();
  sessionStorage.clear();
});

const act = (f: () => Promise<void> | void) => (React as unknown as { act: (f: () => unknown) => Promise<void> }).act(f);
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
const flush = async (ms = 0) => {
  for (let i = 0; i < 4; i++) await act(async () => void (await tick(ms)));
};

/** Mount the real component with fresh mocks. */
async function mount(over: Partial<World> = {}) {
  world = newWorld(over);
  const status = () => ({
    provider: world.provider,
    providers: world.rows.map((r) => (world.extra ? { ...r, key: KEY, secret: KEY, apiKey: KEY } : { ...r })),
    ...(world.extra ? { key: KEY, secret: KEY } : {}),
  });
  cmds = {
    soBrainStatus: vi.fn(async () => status()),
    soBrainSetProvider: vi.fn(async (p: unknown) => {
      world.provider = p as Provider;
      return ok;
    }),
    soBrainSaveKey: vi.fn(async (p: unknown) => {
      if (world.saveDelayMs) await tick(world.saveDelayMs);
      const r = (await world.saveResult()) as { status?: string };
      if (r?.status === "ok") world.rows.find((x) => x.id === p)!.key_saved = true;
      return r;
    }),
    soBrainRemoveKey: vi.fn(async (p: unknown) => {
      world.rows.find((x) => x.id === p)!.key_saved = false;
      return ok;
    }),
    soBrainCheckKey: vi.fn(async () => {
      if (world.checkDelayMs) await tick(world.checkDelayMs);
      return world.check;
    }),
    soBrainOpenConsole: vi.fn(async () => ok),
  };
  (globalThis as unknown as { __so: unknown }).__so = { commands: cmds };
  for (const m of ["log", "info", "warn", "error", "debug", "trace"] as const) {
    consoleSpies.push(vi.spyOn(console, m).mockImplementation(() => {}) as unknown as Cmd);
  }
  server.moduleGraph.invalidateAll();
  const mod = (await server.ssrLoadModule(join(SRC, "so/settings/BrainSection.tsx"))) as { BrainSection: () => unknown };
  Section = mod.BrainSection;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  await act(async () => root!.render(React.createElement(Section as never)));
  await flush();
}

const text = () => host!.textContent ?? "";
const buttons = () => Array.from(host!.querySelectorAll("button"));
function button(label: string): HTMLButtonElement {
  const want = label.toLowerCase();
  const b = buttons().find((x) => (x.textContent ?? "").trim().toLowerCase() === want);
  if (!b) throw new Error(`no button "${label}"; buttons: ${JSON.stringify(buttons().map((x) => x.textContent))}`);
  return b;
}
function input(): HTMLInputElement {
  const all = host!.querySelectorAll('input[type="password"]');
  expect(all.length, "exactly one password field").toBe(1);
  return all[0] as HTMLInputElement;
}
async function type(v: string) {
  const el = input();
  await act(async () => {
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    set.call(el, v);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}
async function click(b: HTMLElement, ms = 0) {
  await act(async () => void b.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  await flush(ms);
}
async function chooseProvider(id: Provider) {
  const sel = Array.from(host!.querySelectorAll("select")).find((s) => Array.from(s.options).some((o) => o.value === id));
  if (sel) {
    await act(async () => {
      sel.value = id;
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    });
  } else {
    const radio = host!.querySelector(`input[type="radio"][value="${id}"]`) as HTMLElement | null;
    const label = world.rows.find((r) => r.id === id)!.label.toLowerCase();
    const alt = Array.from(host!.querySelectorAll('[role="radio"], [role="tab"], button')).find((e) => (e.textContent ?? "").trim().toLowerCase().startsWith(label)) as HTMLElement | undefined;
    const target = radio ?? alt;
    if (!target) throw new Error("no provider switch (select, radio or button) found");
    await act(async () => void target.dispatchEvent(new MouseEvent("click", { bubbles: true })));
  }
  await flush();
}

/** Everything the page could leak into: DOM text, markup, every input value and attribute, storage, console. */
function pageDump(): string {
  const parts = [host!.innerHTML, text(), localStorage.length ? JSON.stringify({ ...localStorage }) : "", JSON.stringify({ ...sessionStorage }), document.title];
  host!.querySelectorAll("input,textarea").forEach((i) => parts.push((i as HTMLInputElement).value, i.getAttribute("value") ?? ""));
  return parts.join("\n");
}
function consoleDump(): string {
  return consoleSpies.map((s) => JSON.stringify(s.mock.calls)).join("\n");
}
const hasKey = (s: string) => s.toLowerCase().includes(KEY_WORD);

// ---------- U1 ----------
describe("U1 status", () => {
  it("shows 'Key saved ✓' when a key is saved and 'No key' when not", async () => {
    await mount();
    expect(text()).toContain("No key");
    expect(text()).not.toContain("Key saved");
    world.rows[1].key_saved = true;
    await act(async () => root!.unmount());
    root = undefined;
    host!.remove();
    await mount({ rows: world.rows });
    expect(text()).toContain("Key saved ✓");
    expect(text()).not.toContain("No key");
  });

  it("shows the label, the model and 'recommended' for Claude, and nothing else from the status", async () => {
    await mount({ extra: true });
    await chooseProvider("claude");
    expect(text()).toContain("Claude");
    expect(text()).toContain("claude-haiku-5-5");
    expect(text().toLowerCase()).toContain("recommended");
    expect(hasKey(pageDump()), "extra status fields (a decoy key) must never be rendered").toBe(false);
    expect(cmds.soBrainStatus).toHaveBeenCalled();
  });

  it("the model is read-only: no text input, no editable model field", async () => {
    await mount();
    const editable = Array.from(host!.querySelectorAll("input, textarea")).filter((i) => (i as HTMLInputElement).type !== "password" && (i as HTMLInputElement).type !== "radio");
    expect(editable.length).toBe(0);
    expect(text()).toContain("gemini-3.5-flash-lite");
  });
});

// ---------- U2 ----------
describe("U2 Save", () => {
  it("calls soBrainSaveKey(provider, typed) exactly once and clears the field at once", async () => {
    await mount({ saveDelayMs: 60 });
    await type(KEY);
    expect(input().value, "positive control: the typed key is in the field before Save").toBe(KEY);
    expect(hasKey(pageDump()), "positive control: the dump scanner can see a key in a field").toBe(true);
    await act(async () => void button("Save").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
    expect(input().value, "the field must clear at once, before the reply").toBe("");
    await flush(80);
    expect(cmds.soBrainSaveKey).toHaveBeenCalledTimes(1);
    expect(cmds.soBrainSaveKey.mock.calls[0]).toEqual(["gemini", KEY]);
    expect(input().value).toBe("");
    expect(text()).toContain("Key saved ✓");
  });

  it("saves for the provider that is selected", async () => {
    await mount();
    await chooseProvider("claude");
    await type(KEY);
    await click(button("Save"));
    expect(cmds.soBrainSaveKey.mock.calls).toEqual([["claude", KEY]]);
  });

  it("Remove key calls soBrainRemoveKey(provider) once and the status goes back to 'No key'", async () => {
    await mount({ rows: newWorld().rows.map((r) => ({ ...r, key_saved: true })) });
    expect(text()).toContain("Key saved ✓");
    await click(button("Remove key"));
    expect(cmds.soBrainRemoveKey.mock.calls).toEqual([["gemini"]]);
    expect(text()).toContain("No key");
  });
});

// ---------- U3 ----------
describe("U3 the key goes nowhere else", () => {
  it("is absent from every other command's args, the DOM, console, localStorage and sessionStorage", async () => {
    await mount({ extra: true });
    await type(KEY);
    await click(button("Save"));
    await click(button("Check key"));
    await chooseProvider("claude");
    await click(button("Get a key"));
    await click(button("Set a spend limit"));
    await click(button("Remove key"));
    await chooseProvider("gemini");
    // positive control: the key WAS passed once, to the one inbound command
    expect(cmds.soBrainSaveKey.mock.calls[0][1]).toBe(KEY);
    expect(JSON.stringify(cmds.soBrainSaveKey.mock.calls)).toContain(KEY);
    // every other command, every call
    for (const name of ["soBrainStatus", "soBrainSetProvider", "soBrainRemoveKey", "soBrainCheckKey", "soBrainOpenConsole"]) {
      expect(cmds[name].mock.calls.length, `${name} was never called: the test did not exercise it`).toBeGreaterThan(0);
      expect(hasKey(JSON.stringify(cmds[name].mock.calls)), `${name} args hold the key`).toBe(false);
    }
    expect(hasKey(pageDump()), "the key is in the DOM, a field, storage or the title").toBe(false);
    expect(hasKey(consoleDump()), "the key reached console.*").toBe(false);
    // positive control for the console scan
    console.log("leak " + KEY);
    expect(hasKey(consoleDump())).toBe(true);
  });

  it("is absent after a failed save and after a thrown save", async () => {
    await mount({ saveResult: async () => ({ status: "error", error: "Couldn't save the key" }) });
    await type(KEY);
    await click(button("Save"));
    expect(hasKey(pageDump())).toBe(false);
    expect(hasKey(consoleDump())).toBe(false);
  });
});

// ---------- U4 ----------
describe("U4 the field", () => {
  it("is a password input, autocomplete off, spellcheck false, empty and never pre-filled", async () => {
    await mount({ extra: true, rows: newWorld().rows.map((r) => ({ ...r, key_saved: true })) });
    const i = input();
    expect(i.getAttribute("type")).toBe("password");
    expect(i.getAttribute("autocomplete")).toBe("off");
    expect(i.getAttribute("spellcheck")).toBe("false");
    expect(i.hasAttribute("value"), "no value / defaultValue attribute").toBe(false);
    expect(i.value).toBe("");
    expect(hasKey(pageDump())).toBe(false);
    expect(i.closest("form"), "a <form> invites password-manager autofill and Enter-submits").toBeNull();
  });

  it("source: no innerHTML, storage, console, fetch or links", () => {
    const src = readFileSync(join(SRC, "so/settings/BrainSection.tsx"), "utf8") + readFileSync(join(SRC, "so/settings/brainView.ts"), "utf8");
    for (const bad of ["dangerouslySetInnerHTML", "innerHTML", "localStorage", "sessionStorage", "console.", "fetch(", "XMLHttpRequest", "window.open", "href=", "eval("]) {
      expect(src.includes(bad), `BrainSection must not contain ${bad}`).toBe(false);
    }
    expect(src).toContain('type="password"');
    expect(src).toMatch(/autoComplete=["{]+"?off/);
    expect(src).toMatch(/spellCheck=\{false\}/);
  });
});

// ---------- U5 ----------
describe("U5 save errors", () => {
  it("shows the fixed text from the reply and never the key", async () => {
    await mount({ saveResult: async () => ({ status: "error", error: "Key looks wrong: paste the whole key" }) });
    await type(KEY);
    await click(button("Save"));
    expect(text()).toContain("Key looks wrong: paste the whole key");
    expect(hasKey(pageDump())).toBe(false);
    expect(text()).not.toContain("Key saved ✓");
  });

  it("a thrown error shows a fixed text and not its own message", async () => {
    await mount({
      saveResult: async () => {
        throw new Error(`boom ${KEY}`);
      },
    });
    await type(KEY);
    await click(button("Save"));
    expect(text()).toContain("Couldn't save the key");
    expect(hasKey(pageDump())).toBe(false);
    expect(hasKey(consoleDump()), "the page logged the error text").toBe(false);
  });

  it("an error text that carries a key-like value is never rendered", async () => {
    await mount({ saveResult: async () => ({ status: "error", error: `rejected ${KEY}` }) });
    await type(KEY);
    await click(button("Save"));
    expect(hasKey(pageDump())).toBe(false);
    expect(text().length).toBeGreaterThan(0);
  });
});

// ---------- U6 ----------
describe("U6 console buttons", () => {
  it("pass only the provider id and the page name, never a URL", async () => {
    await mount();
    await click(button("Get a key"));
    await click(button("Set a spend limit"));
    expect(cmds.soBrainOpenConsole.mock.calls).toEqual([
      ["gemini", "Key"],
      ["gemini", "Spend"],
    ]);
    await chooseProvider("claude");
    await click(button("Get a key"));
    expect(cmds.soBrainOpenConsole.mock.calls[2]).toEqual(["claude", "Key"]);
    expect(host!.querySelectorAll("a[href]").length, "no links: Rust opens the fixed URL").toBe(0);
    expect(JSON.stringify(cmds.soBrainOpenConsole.mock.calls)).not.toMatch(/https?:/);
  });
});

// ---------- U7 ----------
describe("U7 provider switch, Check key, tips", () => {
  it("switching provider calls soBrainSetProvider with the id only and shows that provider's key status", async () => {
    await mount({ rows: newWorld().rows.map((r) => (r.id === "claude" ? { ...r, key_saved: true } : r)) });
    expect(text()).toContain("No key");
    await chooseProvider("claude");
    expect(cmds.soBrainSetProvider.mock.calls).toEqual([["claude"]]);
    expect(text()).toContain("Key saved ✓");
    await chooseProvider("gemini");
    expect(cmds.soBrainSetProvider.mock.calls.at(-1)).toEqual(["gemini"]);
    expect(text()).toContain("No key");
  });

  it("Check key calls soBrainCheckKey(provider) and shows a distinct text for each result", async () => {
    const seen = new Map<string, string>();
    for (const result of ["Ok", "Rejected", "LimitReached", "RateLimited", "Offline", "Failed", "NoKey"]) {
      await mount({ check: result });
      const before = text();
      await click(button("Check key"));
      expect(cmds.soBrainCheckKey.mock.calls).toEqual([["gemini"]]);
      const after = text();
      expect(after, `${result}: Check key showed nothing`).not.toBe(before);
      expect(after.includes(result) && !before.includes(result), `${result}: the raw enum name is on screen`).toBe(false);
      seen.set(result, after);
      await act(async () => root!.unmount());
      root = undefined;
      host!.remove();
    }
    expect(new Set(seen.values()).size, "each KeyCheck result needs its own text").toBe(7);
    // The two wordings the founder's live run (L2, L2b) looks for.
    await mount({ check: "Ok" });
    await click(button("Check key"));
    expect(text()).toContain("Key works");
    await act(async () => root!.unmount());
    root = undefined;
    host!.remove();
    await mount({ check: "Rejected" });
    await click(button("Check key"));
    expect(text()).toContain("Key rejected");
  });

  it("Check key is disabled while a check is in flight and sends one command for a double click", async () => {
    await mount({ checkDelayMs: 80 });
    await act(async () => void button("Check key").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
    expect(button("Check key").disabled, "Check key must be disabled during the check").toBe(true);
    await act(async () => void button("Check key").dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
    await flush(100);
    expect(cmds.soBrainCheckKey).toHaveBeenCalledTimes(1);
    expect(button("Check key").disabled, "enabled again after the reply").toBe(false);
  });

  it("Gemini tips: free-tier human review, restrict the key, spend cap, and what is sent", async () => {
    await mount();
    const t = text();
    expect(t).toContain("Restrict the key to the Gemini API");
    expect(t).toContain("human reviewers may read");
    expect(t).toContain("Google may use your requests to improve its products");
    expect(t).toContain("never say personal information");
    expect(t.toLowerCase()).toContain("experimental");
    expect(t).toContain("Only your spoken command and the list of actions are sent.");
  });

  it("Claude tips: set a spend limit; Gemini's free-tier text is not shown for Claude", async () => {
    await mount();
    await chooseProvider("claude");
    const t = text();
    expect(t).toContain("Set a monthly spend limit");
    expect(t).toContain("spending limit reached");
    expect(t).toContain("Only your spoken command and the list of actions are sent.");
    expect(t).not.toContain("human reviewers");
  });
});

// ---------- U8 ----------
describe("U8 GeneralPane", () => {
  it("renders BrainSection once, in the AI group, with no Sidebar edit", () => {
    expect(GeneralModuleText).toMatch(/import\s*\{\s*BrainSection\s*\}\s*from\s*["']\.\/BrainSection["']/);
    expect(GeneralModuleText.match(/<BrainSection\s*\/>/g)?.length).toBe(1);
    const sidebar = readFileSync(join(SRC, "components/Sidebar.tsx"), "utf8");
    expect(sidebar.includes("BrainSection")).toBe(false);
  });
});

// ---------- U9: the view file ----------
describe("U9 brainView stays pure", () => {
  it("has no React, IPC or DOM access", () => {
    const src = readFileSync(join(SRC, "so/settings/brainView.ts"), "utf8");
    for (const bad of ["from \"react\"", "@/bindings", "document.", "window.", "invoke(", "fetch("]) {
      expect(src.includes(bad), `brainView.ts must not contain ${bad}`).toBe(false);
    }
    expect(src).toContain("export type BrainKeyCheck");
    expect(src, "the view logic is functions here, not in the component").toMatch(/export (function|const) \w+/);
  });
});
