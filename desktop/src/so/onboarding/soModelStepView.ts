// SPDX-License-Identifier: FSL-1.1-MIT
// View logic for the onboarding model step. Pure. Tests: tests/so/step2-model-ui.test.ts (V2).
import { SO_MODEL_ID, type DlState, type ModelApi } from "../settings/modelPaneView";

export interface SoModelStepView {
  /** Continue is allowed while the download runs (and when it failed: onboarding never blocks on it). */
  continueEnabled: boolean;
  /** Show a Retry button (after an error). */
  showRetry: boolean;
  /** One short status line. */
  status: string;
}

/** Describe the step. Continue is always allowed: onboarding never waits for the download. */
export function soModelStepView(dl: DlState, ready: boolean): SoModelStepView {
  const status = ready
    ? "Ready"
    : dl.kind === "downloading"
      ? `Downloading ${Math.max(0, Math.min(100, Math.round(dl.percent)))}%`
      : dl.kind === "verifying"
        ? "Verifying"
        : dl.kind === "error"
          ? "Download failed"
          : "Not downloaded";
  return { continueEnabled: true, showRetry: dl.kind === "error", status };
}

/** Retry = start the download of `SO_MODEL_ID` again. */
export async function retryDownload(api: Pick<ModelApi, "downloadModel">): Promise<void> {
  await api.downloadModel(SO_MODEL_ID);
}
