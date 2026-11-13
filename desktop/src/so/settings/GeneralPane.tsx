// SPDX-License-Identifier: FSL-1.1-MIT
// Our General pane: the talk key, the Fn check, the microphone and the model's own options.
// It imports only our items plus MicrophoneSelector and ModelSettingsCard (test V5).
import React from "react";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { MicrophoneSelector } from "@/components/settings/MicrophoneSelector";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { BrainSection } from "./BrainSection";
import { FnCheckNotice } from "./FnCheckNotice";
import { HotkeySetting } from "./HotkeySetting";
import { STRINGS } from "./strings";

export const GeneralPane: React.FC = () => (
  <div className="max-w-3xl w-full mx-auto space-y-6">
    <SettingsGroup title={STRINGS.hotkeyGroup}>
      <HotkeySetting />
      <FnCheckNotice />
    </SettingsGroup>
    <ModelSettingsCard />
    <BrainSection />
    <SettingsGroup title={STRINGS.microphoneGroup}>
      <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
    </SettingsGroup>
  </div>
);
