import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { useModelStore } from "../../../stores/modelStore";
import { SystemAudioPermissionNotice } from "./SystemAudioPermissionNotice";

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
    isProbingSystemAudio,
  } = useSettings();
  const models = useModelStore((state) => state.models);

  // `null` means the probe has never answered, not that the answer was no.
  // A re-probe leaves the last answer in place, so this row stays mounted.
  if (
    systemAudioAvailability === null ||
    systemAudioAvailability === "unavailable_no_sound_server"
  ) {
    return null;
  }

  // `permission_denied` is only reachable once a capture attempt has failed
  // to change the permission answer. The toggle cannot help here — the
  // backend refuses the enable, so it would spin, round-trip and silently
  // revert — so replace it with the explanation and the way back.
  if (systemAudioAvailability === "permission_denied") {
    return <SystemAudioPermissionNotice grouped={grouped} />;
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
      isUpdating={isUpdating("system_audio_enabled") || isProbingSystemAudio}
      disabled={muteEnabled || !supportsStreaming}
      label={t("settings.advanced.systemAudio.label")}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
