import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface SaveTranscriptsToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SaveTranscripts: React.FC<SaveTranscriptsToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const saveTranscriptsEnabled = getSetting("save_transcripts") ?? false;

    return (
      <ToggleSwitch
        checked={saveTranscriptsEnabled}
        onChange={(enabled) => updateSetting("save_transcripts", enabled)}
        isUpdating={isUpdating("save_transcripts")}
        label={t("settings.privacy.saveTranscripts.label")}
        description={t("settings.privacy.saveTranscripts.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
