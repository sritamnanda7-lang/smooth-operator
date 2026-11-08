// SPDX-License-Identifier: FSL-1.1-MIT
// The talk-key choice. It also shares the current choice with the Fn check notice (a tiny store:
// `useSyncExternalStore` lets several components read one value and re-render when it changes).
import React, { useCallback, useEffect, useSyncExternalStore } from "react";
import { commands, type HotkeyChoice } from "@/bindings";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { STRINGS } from "./strings";

interface HotkeyState {
  current: HotkeyChoice | null;
  options: HotkeyChoice[];
  error: boolean;
}
let state: HotkeyState = { current: null, options: [], error: false };
const listeners = new Set<() => void>();
const set = (next: Partial<HotkeyState>) => {
  state = { ...state, ...next };
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

const LABELS: Record<HotkeyChoice, string> = {
  fn: STRINGS.hotkeyFn,
  right_cmd: STRINGS.hotkeyRightCmd,
  right_ctrl: STRINGS.hotkeyRightCtrl,
};

/** Current hotkey, the choices this OS offers, and a saver. The saver returns true when it worked. */
export function useHotkey() {
  const snap = useSyncExternalStore(subscribe, () => state);
  useEffect(() => {
    if (state.current !== null) return;
    Promise.all([commands.soGetHotkey(), commands.soHotkeyOptions()])
      .then(([current, options]) => set({ current, options, error: false }))
      .catch(() => set({ error: true }));
  }, []);
  const save = useCallback(async (choice: HotkeyChoice): Promise<boolean> => {
    try {
      const result = await commands.soSetHotkey(choice);
      if (result.status === "error") throw new Error(result.error);
      set({ current: choice, error: false });
      return true;
    } catch {
      set({ error: true });
      return false;
    }
  }, []);
  return { ...snap, save };
}

export const HotkeySetting: React.FC = () => {
  const { current, options, error, save } = useHotkey();
  return (
    <SettingContainer
      title={STRINGS.hotkeyTitle}
      description={STRINGS.hotkeyDescription}
      descriptionMode="tooltip"
      grouped={true}
    >
      <div className="flex flex-col items-end gap-1">
        <Dropdown
          options={options.map((o) => ({ value: o, label: LABELS[o] }))}
          selectedValue={current}
          onSelect={(value) => void save(value as HotkeyChoice)}
          placeholder={STRINGS.hotkeyLoading}
          disabled={current === null || options.length < 2}
        />
        {error && (
          <span className="text-xs text-error">{STRINGS.hotkeyFailed}</span>
        )}
      </div>
    </SettingContainer>
  );
};
