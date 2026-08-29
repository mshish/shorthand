import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import type { DictationSettings, PasteMethod } from "@/bindings";

interface DictationPasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling of PasteMethod.tsx bound to
 * settings.dictation.paste_method. "External Script" is intentionally not
 * offered: DictationSettings has no external_script_path field of its own,
 * only the shared top-level one — see the gap noted in this task's header.
 */
export const DictationPasteMethod: React.FC<DictationPasteMethodProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const selectedMethod = (dictation?.paste_method || "ctrl_v") as PasteMethod;

  const mod = osType === "macos" ? "Cmd" : "Ctrl";
  const options: DropdownOption[] = [
    {
      value: "ctrl_v",
      label: t("settings.advanced.pasteMethod.options.clipboard", {
        modifier: mod,
      }),
    },
  ];

  if (osType !== "macos" || selectedMethod === "direct") {
    options.push({
      value: "direct",
      label: t("settings.advanced.pasteMethod.options.direct"),
      disabled: osType === "macos",
    });
  }

  options.push({
    value: "none",
    label: t("settings.advanced.pasteMethod.options.none"),
  });

  if (osType === "windows" || osType === "linux") {
    options.push(
      {
        value: "ctrl_shift_v",
        label: t("settings.advanced.pasteMethod.options.clipboardCtrlShiftV"),
      },
      {
        value: "shift_insert",
        label: t("settings.advanced.pasteMethod.options.clipboardShiftInsert"),
      },
    );
  }

  return (
    <SettingContainer
      title={t("settings.advanced.pasteMethod.title")}
      description={t("settings.advanced.pasteMethod.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition="bottom"
    >
      <Dropdown
        options={options}
        selectedValue={selectedMethod}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            paste_method: value as PasteMethod,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
