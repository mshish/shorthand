import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ModelsSettings } from "@/components/settings/models/ModelsSettings";
import { ModelSettingsCard } from "@/components/settings/general/ModelSettingsCard";
import { CustomWords } from "@/components/settings/CustomWords";
import { FillerWordRemoval } from "@/components/settings/FillerWordRemoval";
import { ChineseScriptSetting } from "@/components/settings/ChineseScript";
import { PreferredLanguages } from "@/components/settings/PreferredLanguages";
import { LanguageDetection } from "@/components/settings/LanguageDetection";
import { LongFormChunking } from "@/components/settings/LongFormChunking";
import { ModelUnloadTimeoutSetting } from "@/components/settings/ModelUnloadTimeout";
import { AccelerationSelector } from "@/components/settings/AccelerationSelector";
import { VadBackendSelector } from "@/components/settings/VadBackendSelector";
import { Sheet } from "@/shorthand/ui/Sheet";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { TabPanel, Tabs } from "@/shorthand/ui/Tabs";

/**
 * Fork-only "Model" section: which model does the transcribing, and how its
 * output reads.
 *
 * These rows are grouped because they all change the text that comes back.
 * Choosing a model, telling it which language to expect, teaching it names it
 * would otherwise mangle, stripping filler words and choosing Simplified or
 * Traditional Chinese characters are one decision made in five places — the
 * quality of the transcript. How the audio got there is Audio; what happens to
 * the text afterwards is AI cleanup.
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
 *
 * The catalog has its own tab, the same `Tabs` bar the Modes page uses: it is
 * the long part, and the settings are what a user changes most, so they open
 * first rather than sitting above a screenful of models. The active tab is
 * view state, not a preference, so it is not persisted.
 */

type ModelTab = "settings" | "models";

export const ModelSettings: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<ModelTab>("settings");

  const tabs = [
    { id: "settings" as const, label: t("settings.model.tabs.settings") },
    { id: "models" as const, label: t("settings.models.title") },
  ];

  return (
    // No `space-y-8` on this element: TabPanel is a sibling of the tab bar,
    // and the gap would land on top of TabPanel's own (see ModesSettings).
    <div className="max-w-3xl w-full mx-auto">
      <Tabs
        tabs={tabs}
        active={activeTab}
        onChange={setActiveTab}
        label={t("settings.model.tabs.label")}
      />

      {activeTab === "settings" && (
        <TabPanel id="settings">
          <div className="space-y-8">
            <ModelSettingsCard />
            <Sheet title={t("settings.languages.title")}>
              <PreferredLanguages descriptionMode="tooltip" grouped={true} />
            </Sheet>
            <Sheet title={t("settings.model.groups.output")}>
              <CustomWords descriptionMode="inline" grouped={true} />
              <FillerWordRemoval descriptionMode="inline" grouped={true} />
              <ChineseScriptSetting descriptionMode="inline" grouped={true} />
              <AdvancedOnly>
                <ModelUnloadTimeoutSetting
                  descriptionMode="tooltip"
                  grouped={true}
                />
                <AccelerationSelector
                  descriptionMode="tooltip"
                  grouped={true}
                />
                <VadBackendSelector descriptionMode="tooltip" grouped={true} />
                <LongFormChunking descriptionMode="tooltip" grouped={true} />
                <LanguageDetection descriptionMode="tooltip" grouped={true} />
              </AdvancedOnly>
            </Sheet>
          </div>
        </TabPanel>
      )}

      {activeTab === "models" && (
        <TabPanel id="models">
          <ModelsSettings />
        </TabPanel>
      )}
    </div>
  );
};
