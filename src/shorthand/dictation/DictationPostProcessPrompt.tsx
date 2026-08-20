import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

interface DictationPostProcessPromptProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only prompt picker for dictation's AI cleanup. Reads the shared
 * top-level post_process_prompts list (prompt authoring stays in upstream's
 * Post-processing section, per the spec — this section only picks) but
 * writes the selection to settings.dictation.post_process_selected_prompt_id
 * so dictation and meeting mode can select different prompts from the same
 * shared list.
 */
export const DictationPostProcessPrompt: React.FC<
  DictationPostProcessPromptProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = dictation?.post_process_selected_prompt_id || "";

  return (
    <SettingContainer
      title={t("settings.postProcessing.prompts.selectedPrompt.title")}
      description={t(
        "settings.postProcessing.prompts.selectedPrompt.description",
      )}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      layout="stacked"
    >
      <Dropdown
        options={prompts.map((p) => ({ value: p.id, label: p.name }))}
        selectedValue={selectedPromptId || null}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            post_process_selected_prompt_id: value,
          } as DictationSettings)
        }
        placeholder={
          prompts.length === 0
            ? t("settings.postProcessing.prompts.noPrompts")
            : t("settings.postProcessing.prompts.selectPrompt")
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
