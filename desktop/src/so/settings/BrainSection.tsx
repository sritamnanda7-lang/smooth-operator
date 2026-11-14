// SPDX-License-Identifier: FSL-1.1-MIT
// The "AI" group in our General pane (step 4): provider choice, write-only key field, Save, Remove,
// Check key, "Get a key". The key lives only in the (uncontrolled) input: it is read once on Save, the
// field is cleared at once, and it is never kept in state, logged or stored. All text is rendered as
// React text children (never as HTML). Rust opens the console pages; the page passes ids only.
import React, { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import {
  checkText,
  keyStatusText,
  providerLabel,
  saveErrorText,
  tipsFor,
  type BrainKeyCheck,
  type BrainProviderId,
  type BrainStatusView,
} from "./brainView";
import { STRINGS } from "./strings";

export const BrainSection: React.FC = () => {
  const [status, setStatus] = useState<BrainStatusView | null>(null);
  const [message, setMessage] = useState("");
  const [checking, setChecking] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const checkingNow = useRef(false); // sync guard: two clicks in one tick see the same render

  const refresh = useCallback(async () => {
    try {
      setStatus((await commands.soBrainStatus()) as BrainStatusView);
    } catch {
      setMessage(STRINGS.aiCheckFailed);
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);

  const current = status?.providers.find((p) => p.id === status.provider);
  if (!status || !current) return null;
  const id = current.id;

  const clear = () => {
    if (field.current) field.current.value = "";
  };

  const choose = async (next: BrainProviderId) => {
    clear();
    setMessage("");
    try {
      await commands.soBrainSetProvider(next);
    } catch {
      setMessage(STRINGS.aiNotSaved);
    }
    await refresh();
  };

  const save = async () => {
    const typed = field.current?.value ?? "";
    clear(); // before the reply: the key must not linger on screen
    if (!typed) return;
    try {
      const r = await commands.soBrainSaveKey(id, typed);
      setMessage(r.status === "ok" ? "" : saveErrorText(r.error));
    } catch {
      setMessage(STRINGS.aiNotSaved);
    }
    await refresh();
  };

  const remove = async () => {
    try {
      const r = await commands.soBrainRemoveKey(id);
      setMessage(r.status === "ok" ? "" : STRINGS.aiNotRemoved);
    } catch {
      setMessage(STRINGS.aiNotRemoved);
    }
    await refresh();
  };

  const check = async () => {
    if (checkingNow.current) return;
    checkingNow.current = true;
    setChecking(true);
    try {
      setMessage(checkText((await commands.soBrainCheckKey(id)) as BrainKeyCheck));
    } catch {
      setMessage(STRINGS.aiCheckFailed);
    } finally {
      checkingNow.current = false;
      setChecking(false);
    }
  };

  const open = (page: "Key" | "Spend") => {
    void commands.soBrainOpenConsole(id, page).catch(() => undefined);
  };

  return (
    <SettingsGroup title={STRINGS.aiGroup}>
      <div className="px-4 p-2 space-y-2">
        <label className="flex items-center gap-2 text-sm">
          <span>{STRINGS.aiProvider}</span>
          <select
            className="border border-line rounded-lg px-2 py-1 bg-background"
            value={id}
            onChange={(e) => void choose(e.target.value as BrainProviderId)}
          >
            {status.providers.map((p) => (
              <option key={p.id} value={p.id}>
                {providerLabel(p)}
              </option>
            ))}
          </select>
        </label>
        <p className="text-sm">
          {STRINGS.aiModel}: {current.model}
        </p>
        <p className="text-sm">{keyStatusText(current)}</p>
        <input
          ref={field}
          type="password"
          autoComplete="off"
          spellCheck={false}
          aria-label={STRINGS.aiKeyField}
          placeholder={STRINGS.aiKeyPlaceholder}
          className="w-full border border-line rounded-lg px-2 py-1 bg-background text-sm"
        />
        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={() => void save()}>
            {STRINGS.aiSave}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => void remove()}>
            {STRINGS.aiRemove}
          </Button>
          <Button size="sm" variant="secondary" disabled={checking} onClick={() => void check()}>
            {STRINGS.aiCheck}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => open("Key")}>
            {STRINGS.aiGetKey}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => open("Spend")}>
            {STRINGS.aiSpend}
          </Button>
        </div>
        {message && <p className="text-sm">{message}</p>}
        <ul className="text-xs text-ink-3 space-y-1 list-disc pl-4">
          {tipsFor(id).map((t) => (
            <li key={t}>{t}</li>
          ))}
        </ul>
      </div>
    </SettingsGroup>
  );
};
