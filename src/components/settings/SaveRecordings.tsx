import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface SaveRecordingsToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SaveRecordings: React.FC<SaveRecordingsToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const saveRecordingsEnabled = getSetting("save_recordings") ?? false;

    return (
      <ToggleSwitch
        checked={saveRecordingsEnabled}
        onChange={(enabled) => updateSetting("save_recordings", enabled)}
        isUpdating={isUpdating("save_recordings")}
        label={t("settings.privacy.saveRecordings.label")}
        description={t("settings.privacy.saveRecordings.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);
