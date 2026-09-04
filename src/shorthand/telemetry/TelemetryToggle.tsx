import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

interface TelemetryToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Fork-only. The consent switch for crash reports and usage counts; the
 * first-run step in `TelemetryOnboarding.tsx` writes the same setting.
 * Reads `?? false` deliberately: an absent key means an existing install,
 * which is opted out. TELEMETRY.md says what the switch controls.
 */
export const TelemetryToggle: React.FC<TelemetryToggleProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("telemetry_enabled") ?? false;

  return (
    <ToggleSwitch
      checked={enabled}
      onChange={(next) => updateSetting("telemetry_enabled", next)}
      isUpdating={isUpdating("telemetry_enabled")}
      label={t("settings.app.telemetry.label")}
      description={t("settings.app.telemetry.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
