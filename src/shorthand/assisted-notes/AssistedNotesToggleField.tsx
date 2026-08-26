import React from "react";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { AssistedNotesSettings } from "@/bindings";

type BooleanAssistedNotesField = {
  [K in keyof AssistedNotesSettings]: AssistedNotesSettings[K] extends boolean
    ? K
    : never;
}[keyof AssistedNotesSettings];

interface AssistedNotesToggleFieldProps {
  field: BooleanAssistedNotesField;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling covering every boolean row in the Assisted Notes section
 * (enabled, push_to_talk, append_trailing_space, save_recordings,
 * save_transcripts, post_process_enabled, follow_stream_enabled). Mirrors
 * `DictationToggleField` for the same reason that one exists:
 * useSettings's getSetting/updateSetting are `keyof Settings` only
 * (src/hooks/useSettings.ts), so there is no nested-path alternative, and
 * this reimplements the same read-spread-write pattern the rest of the fork
 * uses for nested settings, without editing any upstream component.
 * `isUpdating("assisted_notes")` covers the whole struct, not just this
 * field, since the store has one updater entry for the entire nested object.
 */
export const AssistedNotesToggleField: React.FC<
  AssistedNotesToggleFieldProps
> = ({
  field,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const assistedNotes = getSetting("assisted_notes") as
    | AssistedNotesSettings
    | undefined;
  const checked = (assistedNotes?.[field] as boolean | undefined) ?? false;

  return (
    <ToggleSwitch
      checked={checked}
      onChange={(value) =>
        updateSetting("assisted_notes", {
          ...assistedNotes,
          [field]: value,
        } as AssistedNotesSettings)
      }
      isUpdating={isUpdating("assisted_notes")}
      disabled={disabled}
      label={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
