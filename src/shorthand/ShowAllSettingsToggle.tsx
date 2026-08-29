import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

/**
 * The show_all_settings escape hatch toggle, rendered in About (visible in
 * both modes so the hatch is always reachable). Kept out of
 * `AboutSettings.tsx` so upstream changes to that file never conflict with
 * this fork-only addition.
 */
export const ShowAllSettingsToggle: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  return (
    <ToggleSwitch
      checked={getSetting("show_all_settings") ?? false}
      onChange={(nextEnabled) =>
        updateSetting("show_all_settings", nextEnabled)
      }
      isUpdating={isUpdating("show_all_settings")}
      label={t("settings.about.showAllSettings.label")}
      description={t("settings.about.showAllSettings.description")}
      descriptionMode="tooltip"
      grouped={true}
    />
  );
};
