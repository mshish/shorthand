import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type {
  AssistedNotesSettings,
  DictationSettings,
  ShortcutActivation,
} from "@/bindings";

interface ModeShortcutActivationProps {
  /** The per-mode settings object this row edits. */
  mode: "dictation" | "assisted_notes";
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Per-mode twin of upstream's `ShortcutActivationSetting`, which edits the
 * top-level `shortcut_activation` Meetings use. Dictation and Assisted notes
 * each carry their own, so this writes the nested field instead. Kept as a
 * fork file rather than a prop on upstream's component so upstream's stays
 * byte-identical; the labels are upstream's keys, so all 26 locales are
 * translated.
 */
export const ModeShortcutActivation: React.FC<ModeShortcutActivationProps> = ({
  mode,
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const modeSettings = getSetting(mode) as
    | DictationSettings
    | AssistedNotesSettings
    | undefined;

  const options = [
    {
      value: "hold_or_toggle",
      label: t("settings.general.shortcutActivation.options.holdOrToggle"),
      description: t(
        "settings.general.shortcutActivation.descriptions.hold_or_toggle",
      ),
    },
    {
      value: "push_to_talk",
      label: t("settings.general.shortcutActivation.options.pushToTalk"),
      description: t(
        "settings.general.shortcutActivation.descriptions.push_to_talk",
      ),
    },
    {
      value: "toggle",
      label: t("settings.general.shortcutActivation.options.toggle"),
      description: t("settings.general.shortcutActivation.descriptions.toggle"),
    },
  ];

  const selected = (modeSettings?.shortcut_activation ||
    "hold_or_toggle") as ShortcutActivation;

  return (
    <SettingContainer
      title={t("settings.general.shortcutActivation.title")}
      description={t("settings.general.shortcutActivation.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        menuClassName="right-0 w-80 max-w-[calc(100vw-2rem)]"
        selectedValue={selected}
        onSelect={(value) =>
          updateSetting(mode, {
            ...modeSettings,
            shortcut_activation: value as ShortcutActivation,
          } as DictationSettings & AssistedNotesSettings)
        }
        disabled={disabled || isUpdating(mode)}
      />
    </SettingContainer>
  );
};
