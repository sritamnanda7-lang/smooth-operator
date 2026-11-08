// SPDX-License-Identifier: FSL-1.1-MIT
// Read-only Fn clash check: runs when Settings opens, when the talk key changes, and on "Check again".
// Messages are rendered as React text children (never as HTML).
import React, { useCallback, useEffect, useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { fnCheckView, type FnCheckReport } from "./fnCheckView";
import { useHotkey } from "./HotkeySetting";
import { STRINGS } from "./strings";

export const FnCheckNotice: React.FC = () => {
  const { current, save } = useHotkey();
  const [report, setReport] = useState<FnCheckReport | null>(null);
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  const run = useCallback(async () => {
    setBusy(true);
    try {
      setReport((await commands.soFnCheck()) as FnCheckReport);
      setFailed(false);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  }, []);

  // Also re-run when the talk key changes.
  useEffect(() => {
    void run();
  }, [run, current]);

  // Nothing to show on systems or choices where the check does not apply.
  if (report !== null && !report.applies) return null;

  const view = report ? fnCheckView(report) : null;
  return (
    <div className="px-4 p-2 space-y-2">
      <div>
        <h3 className="text-sm font-medium">{STRINGS.fnCheckTitle}</h3>
        <p className="text-xs text-ink-3">{STRINGS.fnCheckDescription}</p>
      </div>
      {failed && (
        <p className="text-sm text-warning">{STRINGS.fnCheckUnknown}</p>
      )}
      {busy && report === null && (
        <p className="text-sm">{STRINGS.fnCheckChecking}</p>
      )}
      {view && view.messages.length === 0 && (
        <p className="text-sm text-moss">{STRINGS.fnCheckClear}</p>
      )}
      {view && view.messages.length > 0 && (
        <ul className="space-y-1 text-sm text-warning">
          {view.messages.map((m, i) => (
            <li key={i}>{m}</li>
          ))}
        </ul>
      )}
      <div className="flex gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={() => void run()}
        >
          {STRINGS.checkAgain}
        </Button>
        {view?.offerRightCmd && (
          <Button
            variant="primary-soft"
            size="sm"
            onClick={() => void save("right_cmd")}
          >
            {STRINGS.useRightCmd}
          </Button>
        )}
      </div>
    </div>
  );
};
