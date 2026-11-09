// SPDX-License-Identifier: FSL-1.1-MIT
// Pure view logic for the AI settings group (step 4). Stub: types only; the functions come in increment 6.
// These mirror the Rust types in so/brain/commands.rs (bindings.ts is regenerated in increment 6).

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
