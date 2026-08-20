import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import type { AutoSubmitKey, DictationSettings } from "@/bindings";

interface DictationAutoSubmitProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

type AutoSubmitOptionValue = AutoSubmitKey | "off";

export const DictationAutoSubmit: React.FC<DictationAutoSubmitProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  const enabled = dictation?.auto_submit ?? false;
  const selectedKey = (dictation?.auto_submit_key || "enter") as AutoSubmitKey;
  const selectedValue: AutoSubmitOptionValue = enabled ? selectedKey : "off";
  const submitWithMetaLabel =
    osType === "macos"
      ? t("settings.advanced.autoSubmit.options.cmdEnter")
      : t("settings.advanced.autoSubmit.options.superEnter");

  const options = [
    { value: "off", label: t("settings.advanced.autoSubmit.options.off") },
    {
      value: "enter",
      label: t("settings.advanced.autoSubmit.options.enter"),
    },
    {
      value: "ctrl_enter",
      label: t("settings.advanced.autoSubmit.options.ctrlEnter"),
    },
    { value: "cmd_enter", label: submitWithMetaLabel },
  ];

  const handleSelect = (value: string) => {
    const selected = value as AutoSubmitOptionValue;
    if (selected === "off") {
      updateSetting("dictation", {
        ...dictation,
        auto_submit: false,
      } as DictationSettings);
      return;
    }
    updateSetting("dictation", {
      ...dictation,
      auto_submit: true,
      auto_submit_key: selected as AutoSubmitKey,
    } as DictationSettings);
  };

  return (
    <SettingContainer
      title={t("settings.advanced.autoSubmit.title")}
      description={t("settings.advanced.autoSubmit.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        selectedValue={selectedValue}
        onSelect={handleSelect}
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
