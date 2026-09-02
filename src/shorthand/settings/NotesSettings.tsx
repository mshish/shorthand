import React from "react";
import { useTranslation } from "react-i18next";
import { ObsidianPluginRow } from "@/shorthand/notes/ObsidianPluginRow";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "Notes" section: where captured notes end up, and what has to
 * be in place for each destination.
 *
 * One sheet per destination. Obsidian's setup is a hand-off — the plugin
 * runs inside Obsidian, so it is installed there — and that is the whole of
 * this section today. A destination configured in-app (an OAuth sign-in,
 * say) would be another sheet here, not another section. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md.
 */
export const NotesSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet
        title={t("settings.notes.obsidian.title")}
        description={t("settings.notes.obsidian.description")}
      >
        <ObsidianPluginRow grouped={true} />
      </Sheet>
    </div>
  );
};
