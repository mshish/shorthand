import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { AssistedNotesSettings } from "@/bindings";

interface AssistedNotesPostProcessPromptProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only prompt picker for assisted notes' AI cleanup. Reads the shared
 * top-level post_process_prompts list (prompt authoring stays in upstream's
 * Post-processing section, per the spec — this section only picks) but
 * writes the selection to
 * settings.assisted_notes.post_process_selected_prompt_id so assisted notes,
 * dictation and meeting mode can each select different prompts from the same
 * shared list.
 */
export const AssistedNotesPostProcessPrompt: React.FC<
  AssistedNotesPostProcessPromptProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const assistedNotes = getSetting("assisted_notes") as
    | AssistedNotesSettings
    | undefined;
  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = assistedNotes?.post_process_selected_prompt_id || "";

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
          updateSetting("assisted_notes", {
            ...assistedNotes,
            post_process_selected_prompt_id: value,
          } as AssistedNotesSettings)
        }
        placeholder={
          prompts.length === 0
            ? t("settings.postProcessing.prompts.noPrompts")
            : t("settings.postProcessing.prompts.selectPrompt")
        }
        disabled={disabled || isUpdating("assisted_notes")}
      />
    </SettingContainer>
  );
};
