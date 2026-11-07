// SPDX-License-Identifier: FSL-1.1-MIT
import React from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/Button";
import {
  OnboardingShell,
  ScreenHeader,
  ScreenMain,
} from "@/components/onboarding/OnboardingShell";

interface AllSetProps {
  progress: { index: number; count: number };
  onBack: () => void;
  onDone: () => void;
}

/** Last onboarding screen. Text only; all strings come from i18n (no HTML). */
export const AllSet: React.FC<AllSetProps> = ({ progress, onBack, onDone }) => {
  const { t } = useTranslation();
  return (
    <OnboardingShell
      progress={progress}
      onBack={onBack}
      primary={
        <Button variant="primary" size="lg" onClick={onDone}>
          {t("onboarding.allSet.finish")}
        </Button>
      }
    >
      <ScreenMain
        header={<ScreenHeader title={t("onboarding.allSet.title")} />}
      >
        <p className="max-w-[460px] text-body text-ink-2">
          {t("onboarding.allSet.body")}
        </p>
      </ScreenMain>
    </OnboardingShell>
  );
};
