// SPDX-License-Identifier: FSL-1.1-MIT
// The "Chrome extension" group in our General pane (step 5): status line (polled every 2 s while open),
// Pair button, the code, Cancel. The code lives only in React state. All text is rendered as React text
// children (never as HTML); nothing from a reply is shown unless linkView maps it to a fixed text.
import React, { useCallback, useEffect, useRef, useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import {
  LINK_POLL_MS,
  linkStatusText,
  pairErrorText,
  pairHelpText,
  type LinkStateView,
  type LinkStatusView,
} from "./linkView";
import { STRINGS } from "./strings";

type Pairing = { code: string; expiresAt: number };

export const LinkSection: React.FC = () => {
  const [state, setState] = useState<LinkStateView>("NotFound");
  const [pairing, setPairing] = useState<Pairing | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [error, setError] = useState("");

  const pairedAt = useRef(0); // when the shown code was made: an older poll must not clear it
  const refresh = useCallback(async () => {
    const asked = Date.now();
    try {
      const s = (await commands.soLinkStatus()) as LinkStatusView;
      setState(s.state);
      // Connected, locked after wrong codes, or the app closed the window: the code is no longer useful.
      const windowClosed = s.pairing_secs_left === null && asked >= pairedAt.current;
      if (s.state === "Connected" || s.locked_secs !== null || windowClosed) setPairing(null);
    } catch {
      setState("NotFound");
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), LINK_POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  // The countdown ticks only while a code is shown.
  const shown = pairing !== null;
  useEffect(() => {
    if (!shown) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [shown]);
  useEffect(() => {
    if (pairing && now >= pairing.expiresAt) setPairing(null);
  }, [pairing, now]);

  const pair = async () => {
    setError("");
    try {
      const r = await commands.soLinkStartPairing();
      if (r.status === "ok") {
        const t = Date.now();
        pairedAt.current = t;
        setNow(t);
        setPairing({ code: r.data.code, expiresAt: t + r.data.secs_left * 1000 });
      } else {
        setError(pairErrorText(r.error));
      }
    } catch {
      setError(STRINGS.linkPairFailed);
    }
  };

  const cancel = async () => {
    setPairing(null);
    try {
      await commands.soLinkCancelPairing();
    } catch {
      setError(STRINGS.linkPairFailed);
    }
  };

  return (
    <SettingsGroup title={STRINGS.linkGroup}>
      <div className="px-4 p-2 space-y-2">
        <p className="text-sm">{linkStatusText(state)}</p>
        {pairing ? (
          <>
            <p className="text-3xl font-mono tracking-widest">{pairing.code}</p>
            <p className="text-sm">{pairHelpText(Math.ceil((pairing.expiresAt - now) / 1000))}</p>
            <Button size="sm" variant="secondary" onClick={() => void cancel()}>
              {STRINGS.linkCancel}
            </Button>
          </>
        ) : (
          <Button size="sm" onClick={() => void pair()}>
            {STRINGS.linkPair}
          </Button>
        )}
        {error ? <p className="text-sm">{error}</p> : null}
      </div>
    </SettingsGroup>
  );
};
