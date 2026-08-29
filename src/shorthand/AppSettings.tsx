import React from "react";
import { useTranslation } from "react-i18next";
import { AutostartToggle } from "@/components/settings/AutostartToggle";
import { StartHidden } from "@/components/settings/StartHidden";
import { ShowTrayIcon } from "@/components/settings/ShowTrayIcon";
import { AudioFeedback } from "@/components/settings/AudioFeedback";
import { VolumeSlider } from "@/components/settings/VolumeSlider";
import { OutputDeviceSelector } from "@/components/settings/OutputDeviceSelector";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";

/**
 * Fork-only "App" section: application lifecycle and feedback settings.
 * Replaces upstream's general/advanced sections in the simplified (default)
 * profile; see `src/shorthand/visibility.ts`.
 */
export const AppSettings: React.FC = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled } = useSettings();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.advanced.groups.app")}>
        <AutostartToggle descriptionMode="tooltip" grouped={true} />
        <StartHidden descriptionMode="tooltip" grouped={true} />
        <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <SettingsGroup title={t("settings.sound.title")}>
        <AudioFeedback descriptionMode="tooltip" grouped={true} />
        <OutputDeviceSelector
          descriptionMode="tooltip"
          grouped={true}
          disabled={!audioFeedbackEnabled}
        />
        <VolumeSlider disabled={!audioFeedbackEnabled} />
      </SettingsGroup>
    </div>
  );
};
