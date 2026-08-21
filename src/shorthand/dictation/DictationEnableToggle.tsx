import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

/**
 * The "Enable Dictation" toggle.
 *
 * Enabling registers two global shortcuts, which fails when another app
 * already owns the combo. `register_shortcut` reports that, and
 * `change_dictation_settings` propagates it, but `updateSetting` swallows
 * the rejection and rolls the optimistic write back — leaving a toggle that
 * springs back to off for no visible reason. Comparing the requested value
 * against the persisted one after the update settles is the only signal the
 * store leaves us, so that is what this reads.
 */
export const DictationEnableToggle: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const enabled = dictation?.enabled ?? false;
  const [failed, setFailed] = React.useState(false);

  const handleChange = async (value: boolean) => {
    setFailed(false);
    await updateSetting("dictation", {
      ...dictation,
      enabled: value,
    } as DictationSettings);
    const persisted =
      (getSetting("dictation") as DictationSettings | undefined)?.enabled ??
      false;
    setFailed(value && !persisted);
  };

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={isUpdating("dictation")}
        label={t("settings.dictation.enable.label")}
        description={t("settings.dictation.enable.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {failed && (
        <p className="px-4 pb-2 text-sm text-red-500">
          {t("settings.dictation.enable.shortcutConflict")}
        </p>
      )}
    </>
  );
};
