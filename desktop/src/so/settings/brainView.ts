// SPDX-License-Identifier: FSL-1.1-MIT
// Pure view logic for the AI settings group (step 4): fixed texts for every result. Nothing from the
// backend is shown unless it is one of these texts. Mirrors the Rust types of the six AI settings IPC calls.

import { STRINGS } from "./strings";

export type BrainProviderId = "claude" | "gemini";

export type BrainProviderView = {
  id: BrainProviderId;
  label: string;
  recommended: boolean;
  key_saved: boolean;
  model: string;
};

export type BrainStatusView = {
  provider: BrainProviderId;
  providers: BrainProviderView[];
};

export type BrainKeyCheck =
  | "Ok"
  | "Rejected"
  | "LimitReached"
  | "RateLimited"
  | "Offline"
  | "Failed"
  | "NoKey";

export type BrainConsolePage = "Key" | "Spend";

const CHECK_TEXT: Record<BrainKeyCheck, string> = {
  Ok: STRINGS.aiCheckOk,
  Rejected: STRINGS.aiCheckRejected,
  LimitReached: STRINGS.aiCheckLimit,
  RateLimited: STRINGS.aiCheckBusy,
  Offline: STRINGS.aiCheckOffline,
  Failed: STRINGS.aiCheckFailed,
  NoKey: STRINGS.aiCheckNoKey,
};

/** One fixed text per check result; an unknown value reads as a failed check. */
export function checkText(r: BrainKeyCheck): string {
  return CHECK_TEXT[r] ?? STRINGS.aiCheckFailed;
}

/** Only the two texts Rust sends are shown; anything else (which could echo input) becomes the generic one. */
export function saveErrorText(fromRust: string): string {
  return fromRust === STRINGS.aiKeyWrong ? fromRust : STRINGS.aiNotSaved;
}

export function keyStatusText(p: BrainProviderView): string {
  return p.key_saved ? STRINGS.aiKeySaved : STRINGS.aiNoKey;
}

export function providerLabel(p: BrainProviderView): string {
  return p.recommended ? `${p.label} (${STRINGS.aiRecommended})` : p.label;
}

/** The fixed tips for a provider (what to restrict, what to cap, what is sent). */
export function tipsFor(id: BrainProviderId): string[] {
  return id === "claude"
    ? [STRINGS.aiTipClaudeSpend, STRINGS.aiTipSent]
    : [
        STRINGS.aiTipGeminiRestrict,
        STRINGS.aiTipGeminiFree,
        STRINGS.aiTipGeminiSpend,
        STRINGS.aiTipSent,
      ];
}
