// SPDX-License-Identifier: FSL-1.1-MIT
import React from "react";
import { useTranslation } from "react-i18next";

/** Text-only product name (placeholder until real branding exists). */
export const Wordmark: React.FC<{ className?: string }> = ({
  className = "",
}) => {
  const { t } = useTranslation();
  return (
    <span className={`font-display font-[650] leading-tight ${className}`}>
      {t("app.name")}
    </span>
  );
};
