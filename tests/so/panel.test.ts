// SPDX-License-Identifier: FSL-1.1-MIT
// @vitest-environment happy-dom
// V1-V3: the panel page renders untrusted text safely. happy-dom stands in for the webview.
import { beforeEach, describe, expect, it } from "vitest";
import { KIND_CLASS, installSoPanel, renderPayload } from "../../desktop/src/so/panel/render";

type SoPanel = { set: (p: unknown) => void };
const win = () => window as unknown as { soPanel?: SoPanel };

let root: HTMLElement;
beforeEach(() => {
  document.body.innerHTML = "";
  root = document.createElement("div");
  root.id = "panel";
  document.body.appendChild(root);
  delete (window as unknown as Record<string, unknown>).soPanel;
});

const payload = (lines: unknown[], v: unknown = 1) => ({ v, lines });
const line = (text: string, kind = "info") => ({ text, kind });

describe("V1 text is literal", () => {
  it("renders <img onerror> as text with no extra elements", () => {
    installSoPanel(window, root);
    const evil = "<img src=x onerror=alert(1)>";
    win().soPanel!.set(payload([line(evil)]));
    expect(root.querySelectorAll("img").length).toBe(0);
    expect(root.querySelectorAll("*").length).toBe(root.children.length); // only our line elements, no nested markup
    expect(root.children.length).toBe(1);
    expect(root.textContent).toBe(evil);
    expect(root.innerHTML).not.toContain("<img");
  });

  it("renderPayload alone is safe too", () => {
    expect(renderPayload(root, payload([line("<script>alert(1)</script>", "warn")]))).toBe(true);
    expect(root.querySelector("script")).toBeNull();
    expect(root.textContent).toBe("<script>alert(1)</script>");
  });

  it("window.soPanel cannot be reassigned, deleted, redefined or changed", () => {
    installSoPanel(window, root);
    const original = win().soPanel!;
    expect(() => {
      (window as unknown as Record<string, unknown>).soPanel = { set: () => {} };
    }).toThrow(TypeError);
    expect(Reflect.set(window, "soPanel", {})).toBe(false);
    expect(Reflect.deleteProperty(window, "soPanel")).toBe(false);
    expect(() => Object.defineProperty(window, "soPanel", { value: {} })).toThrow(TypeError);
    expect(win().soPanel).toBe(original);
    expect(Object.isFrozen(original)).toBe(true);
    expect(() => {
      (original as unknown as Record<string, unknown>).set = () => {};
    }).toThrow(TypeError);
    expect(() => {
      (original as unknown as Record<string, unknown>).extra = 1;
    }).toThrow(TypeError);
    // still works after the attacks
    original.set(payload([line("still ok")]));
    expect(root.textContent).toBe("still ok");
  });

  it("set replaces the previous content (no stacking)", () => {
    installSoPanel(window, root);
    win().soPanel!.set(payload([line("one"), line("two")]));
    win().soPanel!.set(payload([line("three")]));
    expect(root.children.length).toBe(1);
    expect(root.textContent).toBe("three");
  });
});

describe("V2 bad payloads", () => {
  it("bad version is ignored (previous content stays)", () => {
    installSoPanel(window, root);
    win().soPanel!.set(payload([line("good")]));
    for (const v of [2, 0, "1", null, undefined]) {
      win().soPanel!.set(payload([line("bad")], v));
      expect(root.textContent).toBe("good");
    }
  });

  it("non-array lines and non-object payloads are ignored", () => {
    installSoPanel(window, root);
    win().soPanel!.set(payload([line("good")]));
    for (const bad of [{ v: 1, lines: "x" }, { v: 1, lines: { 0: line("x") } }, { v: 1 }, null, "str", 42, [], undefined]) {
      expect(() => win().soPanel!.set(bad)).not.toThrow();
      expect(root.textContent).toBe("good");
    }
    expect(renderPayload(root, { v: 1, lines: "x" })).toBe(false);
  });

  it("more than 6 lines are trimmed to 6", () => {
    installSoPanel(window, root);
    win().soPanel!.set(payload(Array.from({ length: 9 }, (_, i) => line(`l${i}`))));
    expect(root.children.length).toBeLessThanOrEqual(6);
    expect(root.children.length).toBeGreaterThan(0);
  });

  it("malformed line items never throw or inject", () => {
    installSoPanel(window, root);
    expect(() => win().soPanel!.set(payload([null, 5, "s", { text: 3, kind: "info" }, { kind: "info" }, line("ok")]))).not.toThrow();
    expect(root.querySelectorAll("img,script,iframe").length).toBe(0);
    expect(root.textContent).toContain("ok");
  });
});

describe("V3 css class only from the fixed map", () => {
  it("known kinds get exactly the mapped class", () => {
    for (const kind of ["info", "listening", "warn"]) {
      expect(typeof KIND_CLASS[kind], kind).toBe("string");
      expect(KIND_CLASS[kind].length).toBeGreaterThan(0);
    }
    installSoPanel(window, root);
    win().soPanel!.set(payload([line("a", "info"), line("b", "listening"), line("c", "warn")]));
    const els = Array.from(root.children);
    expect(els.map((e) => e.className)).toEqual([KIND_CLASS.info, KIND_CLASS.listening, KIND_CLASS.warn]);
  });

  it("unknown or hostile kinds never reach the class attribute", () => {
    installSoPanel(window, root);
    const hostile = ['x" onclick="alert(1)', "__proto__", "constructor", "toString", "hasOwnProperty", "INFO", "", "a b"];
    for (const kind of hostile) {
      win().soPanel!.set(payload([line("t", kind)]));
      const el = root.children[0];
      expect(el, kind).toBeDefined();
      const allowed = new Set(Object.values(KIND_CLASS).flatMap((c) => c.split(/\s+/)));
      for (const c of Array.from(el.classList)) expect(allowed.has(c), `${kind} -> ${c}`).toBe(true);
      expect(el.getAttribute("onclick")).toBeNull();
      expect(el.getAttributeNames().sort()).toEqual(el.className ? ["class"] : []);
    }
  });
});
