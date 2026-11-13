// SPDX-License-Identifier: FSL-1.1-MIT
// English strings for our settings items (no edit to translation.json).
export const STRINGS: Record<string, string> = {
  hotkeyGroup: "Talk key",
  hotkeyTitle: "Talk key",
  hotkeyDescription:
    "Hold it for a moment to talk. Double-tap it to keep listening; press it once more to stop.",
  hotkeyLoading: "Loading...",
  hotkeyFailed: "Could not change the talk key.",
  hotkeyFn: "Fn (Globe)",
  hotkeyRightCmd: "Right Cmd",
  hotkeyRightCtrl: "Right Ctrl",
  fnCheckTitle: "Fn key check",
  fnCheckDescription:
    "Looks for other things that also use the Fn (Globe) key. It only reads settings; it never changes them.",
  fnCheckClear: "No problems found.",
  fnCheckUnknown: "Could not check.",
  fnCheckChecking: "Checking...",
  checkAgain: "Check again",
  useRightCmd: "Use Right Cmd",
  globeAdvice:
    'Set "Press Globe key to" to Do Nothing in System Settings > Keyboard.',
  dictationMessage:
    "The Dictation shortcut (press Globe twice) is on. Turn it off in System Settings > Keyboard > Dictation.",
  appsMessage: "Running now and may also use Fn:",
  microphoneGroup: "Microphone",
  modelStepTitle: "Speech model",
  modelStepBody:
    "Smooth Operator needs one speech model (about 700 MB). You can continue while it downloads.",
  continue: "Continue",
  retry: "Retry",
  aiGroup: "AI",
  aiProvider: "Provider",
  aiRecommended: "recommended",
  aiModel: "Model",
  aiKeySaved: "Key saved ✓",
  aiNoKey: "No key",
  aiKeyField: "API key",
  aiKeyPlaceholder: "Paste your key here",
  aiSave: "Save",
  aiRemove: "Remove key",
  aiCheck: "Check key",
  aiGetKey: "Get a key",
  aiSpend: "Set a spend limit",
  aiKeyWrong: "Key looks wrong: paste the whole key",
  aiNotSaved: "Couldn't save the key",
  aiNotRemoved: "Couldn't remove the key",
  aiCheckOk: "Key works",
  aiCheckRejected: "Key rejected: check it and paste it again",
  aiCheckLimit: "Spending limit reached on this key",
  aiCheckBusy: "The provider is busy: try again in a minute",
  aiCheckOffline: "No internet: the AI can't be reached",
  aiCheckFailed: "Check didn't finish: try again",
  aiCheckNoKey: "Save a key first",
  aiTipSent: "Only your spoken command and the list of actions are sent.",
  aiTipClaudeSpend:
    "Set a monthly spend limit (Settings → Billing → Spend limits). If you hit it, Smooth Operator says 'spending limit reached'.",
  aiTipGeminiRestrict:
    "Restrict the key to the Gemini API in AI Studio; unrestricted keys are rejected.",
  aiTipGeminiFree:
    "Free tier: Google may use your requests to improve its products, and human reviewers may read your input and output. Use it for development only; never say personal information.",
  aiTipGeminiSpend:
    "Spend caps in AI Studio are experimental and only for paid projects.",
};
