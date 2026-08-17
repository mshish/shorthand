import React from "react";
import { useTranslation } from "react-i18next";
import { ModelsSettings } from "@/components/settings/models/ModelsSettings";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { CustomWords } from "@/components/settings/CustomWords";
import { FillerWordRemoval } from "@/components/settings/FillerWordRemoval";
import { SettingsGroup } from "@/components/ui/SettingsGroup";

/**
 * Fork-only "Transcription" section: the model catalog plus the settings
 * that shape transcription output. Replaces upstream's models/postprocessing
 * sections in the simplified (default) profile; see
 * `src/shorthand/visibility.ts`.
 *
 * `ModelSettingsCard` self-hides when the active model doesn't support
 * language selection or translation — that behaviour is untouched here,
 * we just render the component as-is.
 */
export const TranscriptionSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <ModelsSettings />
      <ModelSettingsCard />
      <SettingsGroup title={t("settings.advanced.groups.transcription")}>
        <CustomWords descriptionMode="tooltip" grouped={true} />
        <FillerWordRemoval descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
    </div>
  );
};
