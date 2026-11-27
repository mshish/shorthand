import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import { ModeShortcutActivation } from "@/shorthand/ModeShortcutActivation";
import { Dependents } from "@/shorthand/ui/Dependents";
import type { LiveTypeSettings } from "@/bindings";

/**
 * Experimental: type the live transcript into the focused window (see
 * `shorthand::live_type`). The switch, then the shortcut and how it behaves.
 *
 * Shown only with Experimental features on: the backend registers the
 * shortcut only when both switches are on, so a row here with experimental
 * features off would describe a shortcut that does nothing.
 *
 * The switch reads its result back for the same reason
 * `AssistedNotesEnableToggle` does: turning it on registers a global
 * shortcut, which fails when another app owns the combo, and the store rolls
 * the switch back without saying why.
 */
export const LiveTypeRows: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const experimental = getSetting("experimental_enabled") ?? false;
  const liveType = getSetting("live_type") as LiveTypeSettings | undefined;
  const enabled = liveType?.enabled ?? false;
  const [failed, setFailed] = React.useState(false);

  if (!experimental) return null;

  const handleChange = async (value: boolean) => {
    setFailed(false);
    await updateSetting("live_type", {
      shortcut_activation: "hold_or_toggle",
      ...liveType,
      enabled: value,
    } as LiveTypeSettings);
    const persisted =
      (getSetting("live_type") as LiveTypeSettings | undefined)?.enabled ??
      false;
    setFailed(value && !persisted);
  };

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={isUpdating("live_type")}
        label={t("settings.liveType.enable.label")}
        description={t("settings.liveType.enable.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {failed && (
        <p className="px-4 pb-2 text-sm text-red-500">
          {t("settings.liveType.enable.shortcutConflict")}
        </p>
      )}
      <Dependents on={enabled}>
        <ShortcutInput shortcutId="live_type" grouped={true} />
        <ModeShortcutActivation
          mode="live_type"
          descriptionMode="tooltip"
          grouped={true}
        />
      </Dependents>
    </>
  );
};
