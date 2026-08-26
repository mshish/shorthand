import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { ClipboardHandling, AssistedNotesSettings } from "@/bindings";

interface AssistedNotesClipboardHandlingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const AssistedNotesClipboardHandling: React.FC<
  AssistedNotesClipboardHandlingProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const assistedNotes = getSetting("assisted_notes") as
    | AssistedNotesSettings
    | undefined;
  const selectedHandling = (assistedNotes?.clipboard_handling ||
    "dont_modify") as ClipboardHandling;

  const options = [
    {
      value: "dont_modify",
      label: t("settings.advanced.clipboardHandling.options.dontModify"),
    },
    {
      value: "copy_to_clipboard",
      label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.advanced.clipboardHandling.title")}
      description={t("settings.advanced.clipboardHandling.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        selectedValue={selectedHandling}
        onSelect={(value) =>
          updateSetting("assisted_notes", {
            ...assistedNotes,
            clipboard_handling: value as ClipboardHandling,
          } as AssistedNotesSettings)
        }
        disabled={disabled || isUpdating("assisted_notes")}
      />
    </SettingContainer>
  );
};
