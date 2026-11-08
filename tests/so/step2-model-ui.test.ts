// SPDX-License-Identifier: FSL-1.1-MIT
// Step 2 Vitest V1-V3: the one-model pane, the onboarding model step, no Handy model catalog in src/so.
// The API is faked; nothing here touches IPC or the network.
import { describe, expect, it, vi } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { SO_MODEL_ID } from "../../desktop/src/so/settings/soModel";
import {
  modelPaneView,
  runModelAction,
  type DlState,
  type ModelApi,
  type ModelRow,
} from "../../desktop/src/so/settings/modelPaneView";
import { retryDownload, soModelStepView } from "../../desktop/src/so/onboarding/soModelStepView";

const ROOT = join(import.meta.dirname, "..", "..");
const row = (id: string, over: Partial<ModelRow> = {}): ModelRow => ({
  id,
  name: `Model ${id}`,
  size_mb: 700,
  is_downloaded: false,
  is_downloading: false,
  partial_size: 0,
  ...over,
});
// Handy's own models are in the list too; the pane must ignore them.
const handy = [row("small"), row("turbo", { is_downloaded: true }), row("parakeet-tdt-0.6b-v3")];
const fakeApi = (): ModelApi & { calls: [string, string][] } => {
  const calls: [string, string][] = [];
  return {
    calls,
    downloadModel: vi.fn(async (id: string) => void calls.push(["download", id])),
    cancelDownload: vi.fn(async (id: string) => void calls.push(["cancel", id])),
    deleteModel: vi.fn(async (id: string) => void calls.push(["delete", id])),
  };
};
const idle: DlState = { kind: "idle" };

describe("V1 ModelPane view", () => {
  it("TS model id equals the Rust one", () => {
    const rs = readFileSync(join(ROOT, "desktop/src-tauri/src/so/stt/model.rs"), "utf8");
    const m = rs.match(/SO_MODEL_ID:\s*&str\s*=\s*"([^"]+)"/);
    expect(m, "SO_MODEL_ID in model.rs").not.toBeNull();
    expect(SO_MODEL_ID).toBe(m![1]);
  });

  it("shows only the SO model, whatever else is in the list", () => {
    const v = modelPaneView([...handy, row(SO_MODEL_ID, { name: "Speech model" })], idle);
    expect(v.modelId).toBe(SO_MODEL_ID);
    expect(v.name).toBe("Speech model");
  });

  it("not downloaded: label and a Download action only", () => {
    const v = modelPaneView([...handy, row(SO_MODEL_ID)], idle);
    expect(v.status).toBe("not_downloaded");
    expect(v.label).toBe("Not downloaded");
    expect(v.actions).toEqual(["download"]);
  });

  it("downloading: percent label, Cancel only; percent is clamped to 0-100", () => {
    const rows = [row(SO_MODEL_ID, { is_downloading: true })];
    const v = modelPaneView(rows, { kind: "downloading", percent: 42 });
    expect(v.status).toBe("downloading");
    expect(v.label).toBe("Downloading 42%");
    expect(v.actions).toEqual(["cancel"]);
    expect(modelPaneView(rows, { kind: "downloading", percent: 150 }).label).toBe("Downloading 100%");
    expect(modelPaneView(rows, { kind: "downloading", percent: -5 }).label).toBe("Downloading 0%");
  });

  it("verifying: label, and neither Download nor Delete", () => {
    const v = modelPaneView([row(SO_MODEL_ID, { is_downloading: true })], { kind: "verifying" });
    expect(v.status).toBe("verifying");
    expect(v.label).toBe("Verifying");
    expect(v.actions).not.toContain("download");
    expect(v.actions).not.toContain("delete");
  });

  it("ready: label and a Delete action only", () => {
    const v = modelPaneView([row(SO_MODEL_ID, { is_downloaded: true })], idle);
    expect(v.status).toBe("ready");
    expect(v.label).toBe("Ready");
    expect(v.actions).toEqual(["delete"]);
  });

  it("error: shows an error status and offers Download again", () => {
    const v = modelPaneView([row(SO_MODEL_ID)], { kind: "error", message: "offline" });
    expect(v.status).toBe("error");
    expect(v.actions).toContain("download");
    expect(v.label.length).toBeGreaterThan(0);
  });

  it("always has a licence credit line", () => {
    for (const dl of [idle, { kind: "verifying" } as DlState]) {
      const v = modelPaneView([row(SO_MODEL_ID)], dl);
      expect(v.credit).toBe("Licensed by NVIDIA Corporation under the NVIDIA Open Model License");
      expect(v.credit).not.toMatch(/handy/i);
    }
  });

  it("each action calls the API once, with the SO id and nothing else", async () => {
    for (const [action, name] of [["download", "downloadModel"], ["cancel", "cancelDownload"], ["delete", "deleteModel"]] as const) {
      const api = fakeApi();
      await runModelAction(api, action);
      expect(api.calls, name).toEqual([[action, SO_MODEL_ID]]);
    }
  });
});

describe("V2 SoModelStep view", () => {
  it("Continue is allowed while downloading, verifying and when ready", () => {
    expect(soModelStepView({ kind: "downloading", percent: 10 }, false).continueEnabled).toBe(true);
    expect(soModelStepView({ kind: "verifying" }, false).continueEnabled).toBe(true);
    expect(soModelStepView(idle, true).continueEnabled).toBe(true);
  });

  it("no Retry unless the download failed", () => {
    expect(soModelStepView({ kind: "downloading", percent: 10 }, false).showRetry).toBe(false);
    expect(soModelStepView(idle, true).showRetry).toBe(false);
  });

  it("after an error: Retry is shown, and onboarding is not blocked", () => {
    const v = soModelStepView({ kind: "error", message: "offline" }, false);
    expect(v.showRetry).toBe(true);
    expect(v.continueEnabled).toBe(true);
    expect(v.status.length).toBeGreaterThan(0);
  });

  it("Retry restarts the download of the SO model only", async () => {
    const api = fakeApi();
    await retryDownload(api);
    expect(api.calls).toEqual([["download", SO_MODEL_ID]]);
  });
});

describe("V3 no Handy model catalog under src/so", () => {
  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((n) => {
      const p = join(dir, n);
      return statSync(p).isDirectory() ? walk(p) : /\.(ts|tsx)$/.test(n) ? [p] : [];
    });
  it("imports nothing from @handy-computer/models", () => {
    const files = walk(join(ROOT, "desktop/src/so"));
    expect(files.length).toBeGreaterThan(0);
    const bad = files.filter((f) => /@handy-computer\/models/.test(readFileSync(f, "utf8")));
    expect(bad).toEqual([]);
  });
});
