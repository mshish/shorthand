import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ShorthandWordmark } from "@/shorthand/brand";
import { Button } from "@/components/ui/Button";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

const TELEMETRY_DOC_URL =
  "https://github.com/mshish/shorthand/blob/main/TELEMETRY.md";

interface TelemetryOnboardingProps {
  onComplete: () => void;
}

/**
 * Fork-only first-run consent for crash reports and usage counts. Shown to
 * new installs only, between permissions and model download. The switch is
 * pre-set to on; nothing is sent until Continue writes the choice, because
 * the stored default is off. TELEMETRY.md is the copy's source of truth.
 */
const TelemetryOnboarding: React.FC<TelemetryOnboardingProps> = ({
  onComplete,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  // A relaunch mid-onboarding shows the previously chosen position.
  const [enabled, setEnabled] = useState<boolean>(
    getSetting("telemetry_enabled") ?? true,
  );
  const [saving, setSaving] = useState(false);

  const handleContinue = async () => {
    setSaving(true);
    try {
      await updateSetting("telemetry_enabled", enabled);
    } finally {
      setSaving(false);
    }
    onComplete();
  };

  return (
    <div className="h-screen w-screen flex flex-col p-6 gap-6 items-center justify-center">
      <div className="flex flex-col items-center gap-2">
        <ShorthandWordmark height={40} />
      </div>

      <div className="max-w-md w-full flex flex-col gap-4">
        <div className="text-center mb-2">
          <h2 className="text-xl font-semibold text-text mb-2">
            {t("onboarding.telemetry.title")}
          </h2>
          <p className="text-text/70">{t("onboarding.telemetry.intro")}</p>
        </div>

        <div className="w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20 space-y-2">
          <h3 className="font-medium text-text">
            {t("onboarding.telemetry.sends.heading")}
          </h3>
          <ul className="list-disc ps-5 space-y-1 text-sm text-text/70">
            <li>{t("onboarding.telemetry.sends.errors")}</li>
            <li>{t("onboarding.telemetry.sends.usage")}</li>
          </ul>
        </div>

        <div className="w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20 space-y-2">
          <h3 className="font-medium text-text">
            {t("onboarding.telemetry.never.heading")}
          </h3>
          <p className="text-sm text-text/70">
            {t("onboarding.telemetry.never.body")}
          </p>
        </div>

        <ToggleSwitch
          checked={enabled}
          onChange={setEnabled}
          label={t("onboarding.telemetry.toggle")}
          description=""
          descriptionMode="inline"
        />

        <button
          type="button"
          className="self-start text-sm font-medium text-text/60 underline underline-offset-2 hover:text-text transition-colors"
          onClick={() => openUrl(TELEMETRY_DOC_URL)}
        >
          {t("onboarding.telemetry.link")}
        </button>

        <div className="flex justify-end">
          <Button
            variant="primary"
            size="lg"
            data-testid="telemetry-continue"
            disabled={saving}
            onClick={handleContinue}
          >
            {t("onboarding.telemetry.continue")}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default TelemetryOnboarding;
