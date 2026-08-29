import React from "react";
import { useTranslation } from "react-i18next";
import { ModelsSettings } from "@/components/settings/models/ModelsSettings";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { CustomWords } from "@/components/settings/CustomWords";
import { FillerWordRemoval } from "@/components/settings/FillerWordRemoval";
import { ModelUnloadTimeoutSetting } from "@/components/settings/ModelUnloadTimeout";
import { AccelerationSelector } from "@/components/settings/AccelerationSelector";
import { VadBackendSelector } from "@/components/settings/VadBackendSelector";
import { Sheet } from "@/shorthand/ui/Sheet";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";

/**
 * Fork-only "Model" section: which model does the transcribing, and how its
 * output reads.
 *
 * These rows are grouped because they all change the text that comes back.
 * Choosing a model, telling it which language to expect, teaching it names it
 * would otherwise mangle and stripping filler words are one decision made in
 * four places — the quality of the transcript. How the audio got there is
 * Audio; what happens to the text afterwards is AI cleanup.
 *
 * Replaces `src/shorthand/TranscriptionSettings.tsx`. See Part 2 of
 * `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md` for the
 * full destination map. Registration is handled elsewhere.
 *
 * `ModelsSettings` (the catalog) and `ModelSettingsCard` draw their own
 * containers, so they are rendered bare rather than wrapped in a `Sheet` —
 * nesting a group inside a group would put a heading above a heading.
 * `ModelSettingsCard` self-hides when the active model supports neither
 * language selection nor translation; that behaviour is untouched here.
 */
export const ModelSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <ModelsSettings />
      <ModelSettingsCard />
      <Sheet title={t("settings.model.groups.output")}>
        <CustomWords descriptionMode="inline" grouped={true} />
        <FillerWordRemoval descriptionMode="inline" grouped={true} />
        <AdvancedOnly>
          <ModelUnloadTimeoutSetting descriptionMode="tooltip" grouped={true} />
          <AccelerationSelector descriptionMode="tooltip" grouped={true} />
          <VadBackendSelector descriptionMode="tooltip" grouped={true} />
        </AdvancedOnly>
      </Sheet>
    </div>
  );
};
