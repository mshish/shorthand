import React from "react";
import { useTranslation } from "react-i18next";
import type { GoogleDocsLayout } from "@/bindings";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";

interface GoogleDocsLayoutRowProps {
  grouped?: boolean;
}

/**
 * Fork-only row: whether meetings share one doc, a tab each, or get a doc
 * each in a folder Shorthand makes. Choosing the folder makes it when
 * Google is already connected; otherwise the row below sets it up.
 */
export const GoogleDocsLayoutRow: React.FC<GoogleDocsLayoutRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const selected = (getSetting("google_docs_layout") ??
    "tab_per_meeting") as GoogleDocsLayout;

  return (
    <SettingContainer
      title={t("settings.notes.google.layout.title")}
      description={t(
        selected === "doc_per_meeting"
          ? "settings.notes.google.layout.descriptionFolder"
          : "settings.notes.google.layout.descriptionTabs",
      )}
      descriptionMode="inline"
      grouped={grouped}
    >
      <Dropdown
        options={[
          {
            value: "tab_per_meeting",
            label: t("settings.notes.google.layout.tabs"),
          },
          {
            value: "doc_per_meeting",
            label: t("settings.notes.google.layout.folder"),
          },
        ]}
        selectedValue={selected}
        onSelect={(value) =>
          updateSetting("google_docs_layout", value as GoogleDocsLayout)
        }
        disabled={isUpdating("google_docs_layout")}
      />
    </SettingContainer>
  );
};
