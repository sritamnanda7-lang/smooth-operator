// SPDX-License-Identifier: FSL-1.1-MIT
// Onboarding model step: starts the one speech model's download. Continue is allowed while it
// runs. Text only. Tests: tests/so/step2-model-ui.test.ts (V2).
import React, { useEffect, useRef } from "react";
import { Button } from "@/components/ui/Button";
import {
  OnboardingShell,
  ScreenHeader,
  ScreenMain,
} from "@/components/onboarding/OnboardingShell";
import { MODEL_CREDIT, SO_MODEL_ID } from "../settings/modelPaneView";
import { STRINGS } from "../settings/strings";
import { useSoModel } from "../settings/useSoModel";
import { retryDownload, soModelStepView } from "./soModelStepView";

interface SoModelStepProps {
  progress: { index: number; count: number };
  /** Inert debug preview: no download starts. */
  preview?: boolean;
  onBack: () => void;
  onContinue: () => void;
}

export const SoModelStep: React.FC<SoModelStepProps> = ({
  progress,
  preview = false,
  onBack,
  onContinue,
}) => {
  const { dl, ready, api } = useSoModel();
  const view = soModelStepView(dl, ready);
  // Start the download once when the step opens, unless it is already there.
  const started = useRef(false);
  useEffect(() => {
    if (preview || ready || started.current || dl.kind !== "idle") return;
    started.current = true;
    void api.downloadModel(SO_MODEL_ID);
  }, [preview, ready, dl.kind, api]);

  return (
    <OnboardingShell
      progress={progress}
      onBack={onBack}
      primary={
        <Button variant="primary" size="lg" disabled={!view.continueEnabled} onClick={onContinue}>
          {STRINGS.continue}
        </Button>
      }
    >
      <ScreenMain header={<ScreenHeader title={STRINGS.modelStepTitle} />}>
        <p className="max-w-[460px] text-body text-ink-2">
          {STRINGS.modelStepBody}
        </p>
        <p className="max-w-[460px] text-body text-ink-2" data-status={dl.kind}>
          {view.status}
        </p>
        {view.showRetry && (
          <Button variant="primary-soft" size="md" onClick={() => void retryDownload(api)}>
            {STRINGS.retry}
          </Button>
        )}
        <p className="max-w-[460px] text-xs text-ink-2">{MODEL_CREDIT}</p>
      </ScreenMain>
    </OnboardingShell>
  );
};
