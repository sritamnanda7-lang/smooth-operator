// SPDX-License-Identifier: FSL-1.1-MIT
// Live state of the one speech model, from Handy's model store (download events, model list).
// The pure view logic is in modelPaneView.ts; this file only connects it to the store.
import { useMemo, useState } from "react";
import { useModelStore } from "@/stores/modelStore";
import { SO_MODEL_ID, type DlState, type ModelApi, type ModelRow } from "./modelPaneView";

export interface SoModel {
  rows: ModelRow[];
  dl: DlState;
  ready: boolean;
  api: ModelApi;
}

export const useSoModel = (): SoModel => {
  const models = useModelStore((s) => s.models);
  const progress = useModelStore((s) => s.downloadProgress[SO_MODEL_ID]);
  const downloading = useModelStore((s) => SO_MODEL_ID in s.downloadingModels);
  const verifying = useModelStore((s) => SO_MODEL_ID in s.verifyingModels);
  const [failed, setFailed] = useState(false);

  const ready = models.some((m) => m.id === SO_MODEL_ID && m.is_downloaded);
  let dl: DlState = { kind: "idle" };
  if (verifying) dl = { kind: "verifying" };
  else if (downloading || progress) dl = { kind: "downloading", percent: progress?.percentage ?? 0 };
  else if (failed && !ready) dl = { kind: "error" };

  const api = useMemo<ModelApi>(
    () => ({
      downloadModel: async (id) => {
        setFailed(false);
        const ok = await useModelStore.getState().downloadModel(id);
        if (!ok) setFailed(true);
        return ok;
      },
      cancelDownload: async (id) => {
        setFailed(false);
        return useModelStore.getState().cancelDownload(id);
      },
      deleteModel: (id) => useModelStore.getState().deleteModel(id),
    }),
    [],
  );
  return { rows: models, dl, ready, api };
};
