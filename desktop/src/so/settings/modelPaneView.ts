// SPDX-License-Identifier: FSL-1.1-MIT
// View logic for the one-model pane. Pure: no DOM, no IPC. Tests: tests/so/step2-model-ui.test.ts (V1).
import { SO_MODEL_ID } from "./soModel";

/** The fields of Handy's ModelInfo this pane reads. */
export interface ModelRow {
  id: string;
  name: string;
  size_mb: number;
  is_downloaded: boolean;
  is_downloading: boolean;
  partial_size: number;
}

/** Live download state, from the model-download-progress / verification events. */
export type DlState =
  | { kind: "idle" }
  | { kind: "downloading"; percent: number }
  | { kind: "verifying" }
  | { kind: "error"; message?: string };

export type PaneAction = "download" | "cancel" | "delete";
export type PaneStatus = "not_downloaded" | "downloading" | "verifying" | "ready" | "error";

export interface ModelPaneView {
  /** Always `SO_MODEL_ID`. Other rows (Handy's models) are ignored. */
  modelId: string;
  name: string;
  status: PaneStatus;
  /** "Not downloaded" | "Downloading 42%" | "Verifying" | "Ready" | an error line. */
  label: string;
  actions: PaneAction[];
  /** Licence credit line. Never empty. */
  credit: string;
}

/** The three calls the pane may make (Handy's existing commands). */
export interface ModelApi {
  downloadModel(id: string): Promise<unknown>;
  cancelDownload(id: string): Promise<unknown>;
  deleteModel(id: string): Promise<unknown>;
}

const MB = 1024 * 1024;
const clamp = (n: number) => Math.max(0, Math.min(100, Math.round(n)));

/** The licence credit. The model is under NVIDIA's licence (not CC-BY); the engine around it is Handy (MIT). */
export const MODEL_CREDIT =
  "Licensed by NVIDIA Corporation under the NVIDIA Open Model License. Speech engine: Handy (MIT).";

/** Pick the `SO_MODEL_ID` row out of `rows` and describe it for the pane. */
export function modelPaneView(rows: ModelRow[], dl: DlState): ModelPaneView {
  const row = rows.find((r) => r.id === SO_MODEL_ID);
  const base = { modelId: SO_MODEL_ID, name: row?.name || "Speech model", credit: MODEL_CREDIT };
  if (dl.kind === "downloading") {
    return { ...base, status: "downloading", label: `Downloading ${clamp(dl.percent)}%`, actions: ["cancel"] };
  }
  if (dl.kind === "verifying") {
    return { ...base, status: "verifying", label: "Verifying", actions: [] };
  }
  if (dl.kind === "error") {
    return { ...base, status: "error", label: "Download failed: try again", actions: ["download"] };
  }
  if (row?.is_downloaded) {
    return { ...base, status: "ready", label: "Ready", actions: ["delete"] };
  }
  if (row?.is_downloading) {
    const pct = row.size_mb > 0 ? (row.partial_size / (row.size_mb * MB)) * 100 : 0;
    return { ...base, status: "downloading", label: `Downloading ${clamp(pct)}%`, actions: ["cancel"] };
  }
  return { ...base, status: "not_downloaded", label: "Not downloaded", actions: ["download"] };
}

/** Run one action against `SO_MODEL_ID` only. */
export async function runModelAction(api: ModelApi, action: PaneAction): Promise<void> {
  if (action === "download") await api.downloadModel(SO_MODEL_ID);
  else if (action === "cancel") await api.cancelDownload(SO_MODEL_ID);
  else await api.deleteModel(SO_MODEL_ID);
}

export { SO_MODEL_ID };
