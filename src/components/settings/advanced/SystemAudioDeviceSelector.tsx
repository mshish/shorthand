import React from "react";
import { useTranslation } from "react-i18next";
import type { AudioDevice } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { useOsType } from "../../../hooks/useOsType";
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
    outputDevices,
    refreshOutputDevices,
  } = useSettings();
  const osType = useOsType();

  if (osType !== "windows") {
    return null;
  }

  const enabled = getSetting("system_audio_enabled") ?? false;
  const muteEnabled = getSetting("mute_while_recording") ?? false;
  const selectedDevice = getSetting("system_audio_device") || "Default";
  const options = [
    {
      value: "Default",
      label: t("settings.advanced.systemAudioDevice.default"),
    },
    ...outputDevices.map((device: AudioDevice) => ({
      value: device.name,
      label: device.name,
    })),
  ];
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
          isLoading || outputDevices.length === 0
            ? t("settings.sound.outputDevice.loading")
            : t("settings.sound.outputDevice.placeholder")
        }
        disabled={
          disabled ||
          isUpdating("system_audio_device") ||
          isLoading ||
          outputDevices.length === 0
        }
        onRefresh={refreshOutputDevices}
      />
    </SettingContainer>
  );
};
