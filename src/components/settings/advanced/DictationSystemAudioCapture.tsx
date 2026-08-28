import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type DictationSettings } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";
import { useModelStore } from "@/stores/modelStore";
import { ToggleSwitch } from "../../ui/ToggleSwitch";

interface DictationSystemAudioCaptureProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Dictation's system-audio control is intentionally not a generic Dictation
 * field. Changing this one preference must coordinate the shared capture lane
 * (and, on macOS, can eventually request system-audio permission), whereas a
 * normal Dictation settings save must remain a pure settings transaction.
 */
export const DictationSystemAudioCapture: React.FC<
  DictationSystemAudioCaptureProps
> = ({ descriptionMode = "tooltip", grouped = false }) => {
  const { t } = useTranslation();
  const {
    getSetting,
    refreshSettings,
    refreshSystemAudioAvailability,
    systemAudioAvailability,
  } = useSettings();
  const models = useModelStore((state) => state.models);
  const [isUpdating, setIsUpdating] = useState(false);

  if (
    systemAudioAvailability === null ||
    systemAudioAvailability === "unavailable_no_sound_server"
  ) {
    return null;
  }

  const dictation = getSetting("dictation") as DictationSettings | undefined;
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
      checked={supportsStreaming && (dictation?.system_audio_enabled ?? false)}
      onChange={async (enabled) => {
        setIsUpdating(true);
        try {
          const result =
            await commands.changeDictationSystemAudioEnabledSetting(enabled);
          if (result.status === "error") {
            throw new Error(result.error);
          }
        } finally {
          await Promise.all([
            refreshSettings(),
            refreshSystemAudioAvailability(),
          ]);
          setIsUpdating(false);
        }
      }}
      isUpdating={isUpdating}
      disabled={muteEnabled || !supportsStreaming}
      label={t("settings.advanced.systemAudio.label")}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
