import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

interface FollowStreamOutputProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const FollowStreamOutput: React.FC<FollowStreamOutputProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("follow_stream_enabled") ?? false}
      onChange={(enabled) => updateSetting("follow_stream_enabled", enabled)}
      isUpdating={isUpdating("follow_stream_enabled")}
      label={t("settings.advanced.followStream.label")}
      description={t("settings.advanced.followStream.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
