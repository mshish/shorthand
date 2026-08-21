import React from "react";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

type BooleanDictationField = {
  [K in keyof DictationSettings]: DictationSettings[K] extends boolean
    ? K
    : never;
}[keyof DictationSettings];

interface DictationToggleFieldProps {
  field: BooleanDictationField;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling covering every boolean row in the Dictation section
 * (enabled, push_to_talk, append_trailing_space, save_recordings,
 * save_transcripts, post_process_enabled). Upstream's equivalent toggles
 * (PushToTalk.tsx, SaveRecordings.tsx, SaveTranscripts.tsx,
 * AppendTrailingSpace.tsx, PostProcessingToggle.tsx) each hardcode a
 * top-level getSetting/updateSetting key and cannot address
 * settings.dictation.*; useSettings's getSetting/updateSetting are
 * `keyof Settings` only (src/hooks/useSettings.ts), so there is no
 * nested-path alternative. This reimplements the same read-spread-write
 * pattern the rest of the fork uses for nested settings, without editing
 * any upstream component. `isUpdating("dictation")` covers the whole
 * struct, not just this field, since the store has one updater entry for
 * the entire nested object.
 */
export const DictationToggleField: React.FC<DictationToggleFieldProps> = ({
  field,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const checked = (dictation?.[field] as boolean | undefined) ?? false;

  return (
    <ToggleSwitch
      checked={checked}
      onChange={(value) =>
        updateSetting("dictation", {
          ...dictation,
          [field]: value,
        } as DictationSettings)
      }
      isUpdating={isUpdating("dictation")}
      disabled={disabled}
      label={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
