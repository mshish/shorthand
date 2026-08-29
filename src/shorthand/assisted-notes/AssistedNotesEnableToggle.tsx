import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { AssistedNotesSettings } from "@/bindings";

/**
 * The "Enable Assisted Notes" toggle.
 *
 * Enabling registers two global shortcuts, which fails when another app
 * already owns the combo. `register_shortcut` reports that, and
 * `change_assisted_notes_settings` propagates it, but `updateSetting`
 * swallows the rejection and rolls the optimistic write back — leaving a
 * toggle that springs back to off for no visible reason. Comparing the
 * requested value against the persisted one after the update settles is the
 * only signal the store leaves us, so that is what this reads.
 */
export const AssistedNotesEnableToggle: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const assistedNotes = getSetting("assisted_notes") as
    | AssistedNotesSettings
    | undefined;
  const enabled = assistedNotes?.enabled ?? false;
  const [failed, setFailed] = React.useState(false);

  const handleChange = async (value: boolean) => {
    setFailed(false);
    await updateSetting("assisted_notes", {
      ...assistedNotes,
      enabled: value,
    } as AssistedNotesSettings);
    const persisted =
      (getSetting("assisted_notes") as AssistedNotesSettings | undefined)
        ?.enabled ?? false;
    setFailed(value && !persisted);
  };

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={isUpdating("assisted_notes")}
        label={t("settings.assistedNotes.enable.label")}
        description={t("settings.assistedNotes.enable.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {failed && (
        <p className="px-4 pb-2 text-sm text-red-500">
          {t("settings.assistedNotes.enable.shortcutConflict")}
        </p>
      )}
    </>
  );
};
