// SPDX-License-Identifier: FSL-1.1-MIT
// View logic for the one-model pane. Pure: no DOM, no IPC. Tests: tests/so/step2-model-ui.test.ts (V1).
// STEP-2 STUBS: `modelPaneView` and `runModelAction` throw until the builder implements them (sub-step 4).
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
  /** Licence credit line (CC-BY needs attribution). Never empty. */
  credit: string;
}

/** The three calls the pane may make (Handy's existing commands). */
export interface ModelApi {
  downloadModel(id: string): Promise<unknown>;
  cancelDownload(id: string): Promise<unknown>;
  deleteModel(id: string): Promise<unknown>;
}

/** STEP-2 STUB. Pick the `SO_MODEL_ID` row out of `rows` and describe it for the pane. */
export function modelPaneView(_rows: ModelRow[], _dl: DlState): ModelPaneView {
  throw new Error("step 2 stub: modelPaneView");
}

/** STEP-2 STUB. Run one action against `SO_MODEL_ID` only. */
export async function runModelAction(_api: ModelApi, _action: PaneAction): Promise<void> {
  throw new Error("step 2 stub: runModelAction");
}

export { SO_MODEL_ID };
