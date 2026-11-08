// SPDX-License-Identifier: FSL-1.1-MIT
// View logic for the onboarding model step. Pure. Tests: tests/so/step2-model-ui.test.ts (V2).
// STEP-2 STUBS: both functions throw until the builder implements them (sub-step 4).
import type { DlState, ModelApi } from "../settings/modelPaneView";

export interface SoModelStepView {
  /** Continue is allowed while the download runs (and when it failed: onboarding never blocks on it). */
  continueEnabled: boolean;
  /** Show a Retry button (after an error). */
  showRetry: boolean;
  /** One short status line. */
  status: string;
}

/** STEP-2 STUB. */
export function soModelStepView(_dl: DlState, _ready: boolean): SoModelStepView {
  throw new Error("step 2 stub: soModelStepView");
}

/** STEP-2 STUB. Retry = start the download of `SO_MODEL_ID` again. */
export async function retryDownload(_api: Pick<ModelApi, "downloadModel">): Promise<void> {
  throw new Error("step 2 stub: retryDownload");
}
