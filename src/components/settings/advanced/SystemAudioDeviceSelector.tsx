import React from "react";
import { useTranslation } from "react-i18next";
import type { DictationSettings, SystemAudioDevice } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";

interface SystemAudioDeviceSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const SystemAudioDeviceSelector: React.FC<
  SystemAudioDeviceSelectorProps
> = ({ descriptionMode = "tooltip", grouped = false }) => {
  const { t } = useTranslation();
  const {
    getSetting,
    updateSetting,
    isUpdating,
    isLoading,
    systemAudioDevices,
    refreshSystemAudioDevices,
    systemAudioAvailability,
  } = useSettings();

  if (
    systemAudioAvailability === null ||
    systemAudioAvailability === "unavailable_no_sound_server"
  ) {
    return null;
  }

  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const enabled =
    (getSetting("system_audio_enabled") ?? false) ||
    (dictation?.system_audio_enabled ?? false);
  const muteEnabled = getSetting("mute_while_recording") ?? false;
  const selectedDevice = getSetting("system_audio_device") || "Default";
  const options = systemAudioDevices.map((device: SystemAudioDevice) => ({
    value: device.id,
    label: device.label,
  }));
  const disabled = !enabled || muteEnabled;

  return (
    <SettingContainer
      title={t("settings.advanced.systemAudioDevice.title")}
      description={t("settings.advanced.systemAudioDevice.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        selectedValue={selectedDevice}
        onSelect={(device) => updateSetting("system_audio_device", device)}
        placeholder={
          isLoading || systemAudioDevices.length === 0
            ? t("settings.sound.outputDevice.loading")
            : t("settings.sound.outputDevice.placeholder")
        }
        disabled={
          disabled ||
          isUpdating("system_audio_device") ||
          isLoading ||
          systemAudioDevices.length === 0
        }
        onRefresh={refreshSystemAudioDevices}
      />
    </SettingContainer>
  );
};
