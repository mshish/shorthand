import React from "react";
import { useTranslation } from "react-i18next";
import type { DictationSettings, SystemAudioDevice } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { DEFAULT_SYSTEM_AUDIO_DEVICE } from "../../../stores/settingsStore";
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

  // `null` means the probe has never answered, not that the answer was no.
  //
  // `permission_denied` hides the row too. The notice that replaces the toggles
  // explains the situation; a device dropdown beside it can only mislead, and
  // on the Audio tab — where no toggle is rendered — it would otherwise sit
  // there alone with nothing to explain why choosing a device does nothing. It
  // is not reliably greyed out either: `enabled` is the OR of both scopes, so a
  // flag left true by a previously-granted session keeps it live, and every
  // selection then fails the backend's availability gate with a raw error.
  if (
    systemAudioAvailability === null ||
    systemAudioAvailability === "unavailable_no_sound_server" ||
    systemAudioAvailability === "permission_denied"
  ) {
    return null;
  }

  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const enabled =
    (getSetting("system_audio_enabled") ?? false) ||
    (dictation?.system_audio_enabled ?? false);
  const muteEnabled = getSetting("mute_while_recording") ?? false;
  const savedDevice = getSetting("system_audio_device");
  // "Follow the system default" is persisted as null, and the sentinel option
  // is matched by `id`, so the unset case has to resolve to that id or the
  // dropdown matches nothing and shows its placeholder instead. Legacy Windows
  // values are plain device names, which are also the ids the backend reports,
  // so they resolve as themselves. "Default" is the pre-sentinel spelling the
  // write path still maps back to null; accept it on the way in for symmetry.
  const selectedDevice =
    !savedDevice || savedDevice === "Default"
      ? DEFAULT_SYSTEM_AUDIO_DEVICE.id
      : savedDevice;
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
        // Deliberately not disabled on an empty list: opening the dropdown is
        // what calls onRefresh, so disabling it while empty would leave the
        // list with no way to fill itself.
        disabled={disabled || isUpdating("system_audio_device") || isLoading}
        onRefresh={refreshSystemAudioDevices}
      />
    </SettingContainer>
  );
};
