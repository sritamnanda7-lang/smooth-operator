// SPDX-License-Identifier: FSL-1.1-MIT
// The Models pane: the one speech model (status, Download / Cancel / Delete, licence credit).
// Text only (React text nodes); no HTML from outside. Tests: tests/so/step2-model-ui.test.ts (V1).
import React from "react";
import { Button } from "@/components/ui/Button";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { modelPaneView, runModelAction, type PaneAction } from "./modelPaneView";
import { useSoModel } from "./useSoModel";

const ACTION_LABEL: Record<PaneAction, string> = {
  download: "Download",
  cancel: "Cancel",
  delete: "Delete",
};

export const ModelPane: React.FC = () => {
  const { rows, dl, api } = useSoModel();
  const view = modelPaneView(rows, dl);
  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title="Speech model">
        <div className="flex items-center justify-between gap-4 px-4 py-3">
          <div className="min-w-0">
            <p className="text-sm font-medium">{view.name}</p>
            <p className="text-xs text-text/70" data-status={view.status}>
              {view.label}
            </p>
          </div>
          <div className="flex gap-2">
            {view.actions.map((action) => (
              <Button
                key={action}
                variant={action === "delete" ? "danger-ghost" : "primary-soft"}
                size="sm"
                onClick={() => void runModelAction(api, action)}
              >
                {ACTION_LABEL[action]}
              </Button>
            ))}
          </div>
        </div>
        <p className="px-4 pb-3 text-xs text-text/60">{view.credit}</p>
      </SettingsGroup>
    </div>
  );
};
