import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { ClipboardHandling, DictationSettings } from "@/bindings";

interface DictationClipboardHandlingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const DictationClipboardHandling: React.FC<
  DictationClipboardHandlingProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const selectedHandling = (dictation?.clipboard_handling ||
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
          updateSetting("dictation", {
            ...dictation,
            clipboard_handling: value as ClipboardHandling,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
