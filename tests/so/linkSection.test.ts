// SPDX-License-Identifier: FSL-1.1-MIT
// @vitest-environment happy-dom
// Step 5 U1-U5 (+U6): the "Chrome extension" settings group (desktop/src/so/settings/LinkSection.tsx and linkView.ts).
// The 3 IPC commands are mocked (tests/so/support/bindingsMock.ts stands in for "@/bindings"); fake timers; no Tauri.
// Same loader as brainSection.test.ts. Assumptions about the component (see the report): it reads its data through
// `commands.soLinkStatus()` (polled: once at mount, then every 2 s while mounted), starts a pairing with
// `commands.soLinkStartPairing()` and cancels with `commands.soLinkCancelPairing()`, all called with NO arguments;
// buttons are labelled "Pair" and "Cancel"; the code is rendered as text; it renders nothing else from a reply.
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { createServer, type ViteDevServer } from "vite";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dirname, "..", "..");
const SRC = join(ROOT, "desktop", "src");
const CODE = "482915";
const TIMERS = ["setInterval", "clearInterval", "setTimeout", "clearTimeout", "Date"] as const;

type Cmd = ReturnType<typeof vi.fn>;
type World = { state: string; pairingStart: number | null; startResult: () => unknown; extra: boolean };

let server: ViteDevServer;
let React: typeof import("react");
let createRoot: typeof import("react-dom/client").createRoot;
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
  vi.useRealTimers();
  consoleSpies.forEach((s) => s.mockRestore());
  consoleSpies = [];
  localStorage.clear();
  sessionStorage.clear();
});

const act = (f: () => Promise<void> | void) => (React as unknown as { act: (f: () => unknown) => Promise<void> }).act(f);
/** Let promises settle and move the fake clock. */
const advance = async (ms = 0) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
const ok = (code = CODE, secs = 120) => ({ status: "ok", data: { code, secs_left: secs } });

async function mount(over: Partial<World> = {}) {
  vi.useFakeTimers({ toFake: [...TIMERS] });
  world = { state: "NotPaired", pairingStart: null, startResult: () => ok(), extra: false, ...over };
  const status = () => {
    const left = world.pairingStart === null ? null : Math.max(0, 120 - Math.floor((Date.now() - world.pairingStart) / 1000));
    return {
      state: world.state,
      pairing_secs_left: left === 0 ? null : left,
      locked_secs: null,
      ...(world.extra ? { token: CODE, code: CODE } : {}),
    };
  };
  cmds = {
    soLinkStatus: vi.fn(async () => status()),
    soLinkStartPairing: vi.fn(async () => {
      const r = world.startResult() as { status?: string };
      if (r?.status === "ok") world.pairingStart = Date.now();
      return r;
    }),
    soLinkCancelPairing: vi.fn(async () => {
      world.pairingStart = null;
      return null;
    }),
  };
  (globalThis as unknown as { __so: unknown }).__so = { commands: cmds };
  for (const m of ["log", "info", "warn", "error", "debug", "trace"] as const) {
    consoleSpies.push(vi.spyOn(console, m).mockImplementation(() => {}) as unknown as Cmd);
  }
  server.moduleGraph.invalidateAll();
  const mod = (await server.ssrLoadModule(join(SRC, "so/settings/LinkSection.tsx"))) as { LinkSection: () => unknown };
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  await act(async () => root!.render(React.createElement(mod.LinkSection as never)));
  await advance(0);
}

const text = () => host!.textContent ?? "";
const buttons = () => Array.from(host!.querySelectorAll("button"));
function button(label: string): HTMLButtonElement {
  const want = label.toLowerCase();
  const b = buttons().find((x) => (x.textContent ?? "").trim().toLowerCase() === want);
  if (!b) throw new Error(`no button "${label}"; buttons: ${JSON.stringify(buttons().map((x) => x.textContent))}`);
  return b;
}
const hasButton = (label: string) => buttons().some((x) => (x.textContent ?? "").trim().toLowerCase() === label.toLowerCase());
async function click(b: HTMLElement) {
  await act(async () => void b.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true })));
  await advance(0);
}
function pageDump(): string {
  const parts = [host!.innerHTML, text(), JSON.stringify({ ...localStorage }), JSON.stringify({ ...sessionStorage }), document.title];
  host!.querySelectorAll("input,textarea").forEach((i) => parts.push((i as HTMLInputElement).value, i.getAttribute("value") ?? ""));
  return parts.join("\n");
}
const consoleDump = () => consoleSpies.map((s) => JSON.stringify(s.mock.calls)).join("\n");

const LINE: Record<string, string> = {
  Connected: "Chrome extension: connected",
  NotPaired: "Chrome extension: not paired",
  NotFound: "Chrome extension: not found (open Chrome; check the extension is on)",
  PortBusy: "Port 47815 is used by another app",
  KeychainError: "Can't read the extension token from Keychain",
};

// ---------- U1 ----------
describe("U1 status line", () => {
  for (const [state, line] of Object.entries(LINE)) {
    it(`${state} reads "${line}"`, async () => {
      await mount({ state });
      expect(text()).toContain(line);
      expect(cmds.soLinkStatus).toHaveBeenCalled();
      for (const [other, otherLine] of Object.entries(LINE)) if (other !== state) expect(text(), `${state} must not show the ${other} text`).not.toContain(otherLine);
      expect(text().includes(state) && !line.includes(state), "the raw enum name is on screen").toBe(false);
    });
  }
  it("shows the group title and nothing else from the status (a decoy token field is never rendered)", async () => {
    await mount({ state: "Connected", extra: true });
    expect(text()).toContain("Chrome extension");
    expect(pageDump()).not.toContain(CODE);
  });
  it("a reply with an unknown state shows the fixed 'not found' text, not the raw value", async () => {
    await mount({ state: "Weird<b>State" });
    expect(text()).toContain(LINE.NotFound);
    expect(text()).not.toContain("Weird");
  });
});

// ---------- U2 ----------
describe("U2 Pair, the code, Cancel", () => {
  it("Pair calls soLinkStartPairing once with no arguments; the code and the countdown appear; the code counts down", async () => {
    await mount({ state: "NotFound" });
    expect(cmds.soLinkStartPairing).not.toHaveBeenCalled();
    expect(text()).not.toContain(CODE);
    await click(button("Pair"));
    expect(cmds.soLinkStartPairing).toHaveBeenCalledTimes(1);
    expect(cmds.soLinkStartPairing.mock.calls[0]).toEqual([]);
    expect(text()).toContain(CODE);
    expect(text()).toContain("Type this code in the Smooth Operator extension popup. Expires in 2:00.");
    expect(hasButton("Cancel")).toBe(true);
    await advance(30_000);
    expect(text()).toContain("Expires in 1:30.");
    expect(text()).toContain(CODE);
  });

  it("Cancel calls soLinkCancelPairing once and removes the code at once; Pair is back", async () => {
    await mount();
    await click(button("Pair"));
    expect(text()).toContain(CODE);
    await click(button("Cancel"));
    expect(cmds.soLinkCancelPairing).toHaveBeenCalledTimes(1);
    expect(cmds.soLinkCancelPairing.mock.calls[0]).toEqual([]);
    expect(pageDump()).not.toContain(CODE);
    expect(hasButton("Pair")).toBe(true);
    expect(hasButton("Cancel")).toBe(false);
    await advance(10_000);
    expect(pageDump(), "a later poll must not bring the code back").not.toContain(CODE);
  });

  it("the code is cleared when the status becomes Connected", async () => {
    await mount({ state: "NotFound" });
    await click(button("Pair"));
    expect(text()).toContain(CODE);
    world.state = "Connected";
    world.pairingStart = null;
    await advance(2000);
    expect(pageDump()).not.toContain(CODE);
    expect(text()).toContain(LINE.Connected);
  });

  it("the code is cleared when a poll says there is no window (pairing_secs_left null) while the local countdown is still running", async () => {
    await mount({ state: "NotFound" });
    await click(button("Pair"));
    await advance(10_000);
    expect(text()).toContain(CODE);
    world.pairingStart = null; // the app closed the window (cancelled elsewhere, locked, or used)
    await advance(2000);
    expect(pageDump()).not.toContain(CODE);
    expect(hasButton("Pair")).toBe(true);
  });

  it("the code is cleared when the window expires (120 s)", async () => {
    await mount({ state: "NotFound" });
    await click(button("Pair"));
    await advance(119_000);
    expect(text()).toContain(CODE);
    await advance(3_000);
    expect(pageDump()).not.toContain(CODE);
    expect(hasButton("Pair")).toBe(true);
  });

  it("the code lives only in the page text: not in any command argument, console, storage or attribute", async () => {
    await mount({ state: "NotFound" });
    await click(button("Pair"));
    await advance(4000);
    await click(button("Cancel"));
    for (const [name, c] of Object.entries(cmds)) expect(JSON.stringify(c.mock.calls), name).not.toContain(CODE);
    expect(consoleDump()).not.toContain(CODE);
    expect(JSON.stringify({ ...localStorage, ...sessionStorage })).not.toContain(CODE);
    // positive control for the dump: while it is shown the scanner finds it
    await click(button("Pair"));
    expect(pageDump()).toContain(CODE);
  });
});

// ---------- U3 ----------
describe("U3 polling", () => {
  it("polls at mount and then every 2 s; the interval is cleared on unmount", async () => {
    await mount();
    const n = cmds.soLinkStatus.mock.calls.length;
    expect(n, "one status read at mount").toBe(1);
    await advance(2000);
    expect(cmds.soLinkStatus).toHaveBeenCalledTimes(n + 1);
    await advance(4000);
    expect(cmds.soLinkStatus).toHaveBeenCalledTimes(n + 3);
    await advance(1999);
    expect(cmds.soLinkStatus, "not before the 2 s are up").toHaveBeenCalledTimes(n + 3);
    await act(async () => root!.unmount());
    root = undefined;
    const after = cmds.soLinkStatus.mock.calls.length;
    await advance(20_000);
    expect(cmds.soLinkStatus.mock.calls.length, "no poll after unmount").toBe(after);
    expect(vi.getTimerCount(), "no timer is left running").toBe(0);
  });

  it("a status line that changes is shown at the next poll", async () => {
    await mount({ state: "NotFound" });
    world.state = "Connected";
    await advance(2000);
    expect(text()).toContain(LINE.Connected);
  });

  it("a failing status call shows a fixed text and never its error message", async () => {
    await mount();
    cmds.soLinkStatus.mockImplementation(async () => {
      throw new Error(`boom ${CODE}`);
    });
    await advance(2000);
    expect(pageDump()).not.toContain(CODE);
    expect(consoleDump()).not.toContain(CODE);
    expect(text().length).toBeGreaterThan(0);
  });
});

// ---------- U4 ----------
describe("U4 pairing errors", () => {
  it("Locked: the fixed text with the seconds", async () => {
    await mount({ startResult: () => ({ status: "error", error: "Too many wrong codes. Try again in 42 s." }) });
    await click(button("Pair"));
    expect(text()).toContain("Too many wrong codes. Try again in 42 s.");
    expect(text()).not.toContain("Type this code");
  });

  it("PortBusy: the same text as the status line, from one constant", async () => {
    await mount({ state: "PortBusy", startResult: () => ({ status: "error", error: "Port 47815 is used by another app" }) });
    expect(text()).toContain(LINE.PortBusy);
    await click(button("Pair"));
    expect(text().split(LINE.PortBusy).length - 1, "the line and the error may both show it").toBeGreaterThanOrEqual(1);
    const view = (await server.ssrLoadModule(join(SRC, "so/settings/linkView.ts"))) as Record<string, (...a: never[]) => string>;
    expect(view.pairErrorText("Port 47815 is used by another app" as never)).toBe(LINE.PortBusy);
    expect(view.linkStatusText("PortBusy" as never)).toBe(LINE.PortBusy);
  });

  it("any other error text is replaced by the fixed generic one and never rendered", async () => {
    await mount({ startResult: () => ({ status: "error", error: `rejected ${CODE} <img src=x onerror=alert(1)>` }) });
    await click(button("Pair"));
    expect(pageDump()).not.toContain(CODE);
    expect(host!.querySelector("img")).toBeNull();
    expect(text()).toContain("Couldn't start pairing");
    expect(hasButton("Pair"), "the user can try again").toBe(true);
  });

  it("a thrown error shows the generic text, not its message", async () => {
    await mount();
    cmds.soLinkStartPairing.mockImplementation(async () => {
      throw new Error(`boom ${CODE}`);
    });
    await click(button("Pair"));
    expect(text()).toContain("Couldn't start pairing");
    expect(pageDump()).not.toContain(CODE);
    expect(consoleDump()).not.toContain(CODE);
  });
});

// ---------- U5 ----------
describe("U5 GeneralPane", () => {
  it("renders LinkSection once, imported from ./LinkSection, with no Sidebar edit", () => {
    expect(GeneralModuleText).toMatch(/import\s*\{\s*LinkSection\s*\}\s*from\s*["']\.\/LinkSection["']/);
    expect(GeneralModuleText.match(/<LinkSection\s*\/>/g)?.length).toBe(1);
    const sidebar = readFileSync(join(SRC, "components/Sidebar.tsx"), "utf8");
    expect(sidebar.includes("LinkSection")).toBe(false);
  });
  it("source: no innerHTML, storage, console, fetch, WebSocket, links or eval", () => {
    const src = readFileSync(join(SRC, "so/settings/LinkSection.tsx"), "utf8") + readFileSync(join(SRC, "so/settings/linkView.ts"), "utf8");
    for (const bad of ["dangerouslySetInnerHTML", "innerHTML", "localStorage", "sessionStorage", "console.", "fetch(", "XMLHttpRequest", "WebSocket", "window.open", "href=", "eval(", "navigator.clipboard"]) {
      expect(src.includes(bad), `LinkSection must not contain ${bad}`).toBe(false);
    }
  });
});

// ---------- U6: the view file ----------
describe("U6 linkView is pure and exact", () => {
  it("has no React, IPC or DOM access", () => {
    const src = readFileSync(join(SRC, "so/settings/linkView.ts"), "utf8");
    for (const bad of ['from "react"', "@/bindings", "document.", "window.", "invoke(", "fetch("]) expect(src.includes(bad), `linkView.ts must not contain ${bad}`).toBe(false);
    expect(src).toMatch(/export (function|const) \w+/);
  });
  it("texts, countdown and the poll period", async () => {
    const v = (await server.ssrLoadModule(join(SRC, "so/settings/linkView.ts"))) as Record<string, any>;
    expect(v.LINK_POLL_MS).toBe(2000);
    for (const [state, line] of Object.entries(LINE)) expect(v.linkStatusText(state)).toBe(line);
    expect(v.linkStatusText("Nope")).toBe(LINE.NotFound);
    expect(v.countdownText(120)).toBe("2:00");
    expect(v.countdownText(90)).toBe("1:30");
    expect(v.countdownText(59)).toBe("0:59");
    expect(v.countdownText(61)).toBe("1:01");
    expect(v.countdownText(0)).toBe("0:00");
    expect(v.countdownText(-5)).toBe("0:00");
    expect(v.pairHelpText(120)).toBe("Type this code in the Smooth Operator extension popup. Expires in 2:00.");
    expect(v.pairErrorText("Too many wrong codes. Try again in 7 s.")).toBe("Too many wrong codes. Try again in 7 s.");
    expect(v.pairErrorText("Port 47815 is used by another app")).toBe("Port 47815 is used by another app");
    expect(v.pairErrorText("anything else")).toBe("Couldn't start pairing");
    expect(v.pairErrorText("Too many wrong codes. Try again in x s.")).toBe("Couldn't start pairing");
    expect(v.pairErrorText("Too many wrong codes. Try again in 7 s.<b>")).toBe("Couldn't start pairing");
  });
});
