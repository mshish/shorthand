import React from "react";
import { useTranslation } from "react-i18next";
import type { DictationSettings } from "@/bindings";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface MuteWhileRecordingToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const MuteWhileRecording: React.FC<MuteWhileRecordingToggleProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const muteEnabled = getSetting("mute_while_recording") ?? false;
    // Both scopes count. The exclusion is mutual — each system-audio toggle
    // disables itself while mute is on — so reading only the Meetings scope
    // let a user enable mute on top of Dictation's capture and leave that
    // toggle checked-and-disabled with no way back except turning mute off.
    const dictation = getSetting("dictation") as DictationSettings | undefined;
    const systemAudioEnabled =
      (getSetting("system_audio_enabled") ?? false) ||
      (dictation?.system_audio_enabled ?? false);

    return (
      <ToggleSwitch
        checked={muteEnabled}
        onChange={(enabled) => updateSetting("mute_while_recording", enabled)}
        isUpdating={isUpdating("mute_while_recording")}
        disabled={systemAudioEnabled}
        label={t("settings.debug.muteWhileRecording.label")}
        description={
          systemAudioEnabled
            ? t("settings.advanced.systemAudio.muteConflict")
            : t("settings.debug.muteWhileRecording.description")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
