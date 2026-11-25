import React from "react";
import { useTranslation } from "react-i18next";
import type { NotesTarget } from "@/bindings";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";

interface NotesTargetRowProps {
  grouped?: boolean;
}

/**
 * Fork-only row: where meeting notes go. Obsidian leaves everything to the
 * Shorthand plugin, as before; Google Docs has the app start the notes
 * engine itself when a meeting begins (`shorthand::google::runner`).
 */
export const NotesTargetRow: React.FC<NotesTargetRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const selected = (getSetting("notes_target") ?? "obsidian") as NotesTarget;

  return (
    <SettingContainer
      title={t("settings.notes.target.title")}
      description={t("settings.notes.target.description")}
      descriptionMode="inline"
      grouped={grouped}
    >
      <Dropdown
        options={[
          { value: "obsidian", label: t("settings.notes.target.obsidian") },
          { value: "google_docs", label: t("settings.notes.target.googleDocs") },
        ]}
        selectedValue={selected}
        onSelect={(value) => updateSetting("notes_target", value as NotesTarget)}
        disabled={isUpdating("notes_target")}
      />
    </SettingContainer>
  );
};
