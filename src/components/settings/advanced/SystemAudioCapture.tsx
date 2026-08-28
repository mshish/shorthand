import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useModelStore } from "../../../stores/modelStore";

interface SystemAudioCaptureProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SystemAudioCapture: React.FC<SystemAudioCaptureProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    isUpdating,
    systemAudioAvailability,
    refreshSystemAudioAvailability,
  } = useSettings();
  const models = useModelStore((state) => state.models);

  if (
    systemAudioAvailability === null ||
    systemAudioAvailability === "unavailable_no_sound_server"
  ) {
    return null;
  }

  const muteEnabled = getSetting("mute_while_recording") ?? false;
  const selectedModel = getSetting("selected_model");
  const supportsStreaming =
    models.find((model) => model.id === selectedModel)?.supports_streaming ??
    false;
  const description = supportsStreaming
    ? t("settings.advanced.systemAudio.description")
    : t("settings.advanced.systemAudio.streamingRequired");
  return (
    <ToggleSwitch
      checked={
        supportsStreaming && (getSetting("system_audio_enabled") ?? false)
      }
      onChange={async (enabled) => {
        await updateSetting("system_audio_enabled", enabled);
        await refreshSystemAudioAvailability();
      }}
      isUpdating={isUpdating("system_audio_enabled")}
      disabled={muteEnabled || !supportsStreaming}
      label={t("settings.advanced.systemAudio.label")}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
