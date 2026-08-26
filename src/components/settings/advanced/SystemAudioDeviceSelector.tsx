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
  // Plain map over the store's already-normalised list, matching
  // OutputDeviceSelector/MicrophoneSelector/ClamshellMicrophoneSelector. The
  // store (settingsStore.ts refreshOutputDevices) is the sole owner of the
  // "Default" sentinel: it filters out whatever the backend enumeration
  // injected and prepends its own DEFAULT_AUDIO_DEVICE, so outputDevices
  // already contains exactly one Default entry. Adding a second one here
  // was this component's own bug. The sentinel's raw name, "Default", is
  // deliberately left untranslated to match the other three selectors,
  // which are upstream components that also render device.name unchanged —
  // even though settings.advanced.systemAudioDevice.default is a genuinely
  // translatable fork string today, translating it here alone would make
  // the same "Default" entry read in the user's language in this dropdown
  // while staying English in the three adjacent ones on the same page.
  // Making all four consistent would mean editing three upstream files for
  // a translation-only change, which is out of scope for this fix.
  const options = outputDevices.map((device: AudioDevice) => ({
    value: device.name,
    label: device.name,
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
