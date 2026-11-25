import React from "react";
import { useTranslation } from "react-i18next";
import { GoogleDocsRow } from "@/shorthand/notes/GoogleDocsRow";
import { NotesTargetRow } from "@/shorthand/notes/NotesTargetRow";
import { ObsidianPluginRow } from "@/shorthand/notes/ObsidianPluginRow";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "Notes" section: where captured notes end up, and what has to
 * be in place for each destination.
 *
 * A choice of destination for meetings, then one sheet per destination.
 * Obsidian's setup is a hand-off — the plugin runs inside Obsidian, so it is
 * installed there. Google Docs is set up here: an OAuth sign-in and the doc
 * meetings go into. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md
 * and docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md.
 */
export const NotesSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet
        title={t("settings.notes.target.sheetTitle")}
        description={t("settings.notes.target.sheetDescription")}
      >
        <NotesTargetRow grouped={true} />
      </Sheet>
      <Sheet
        title={t("settings.notes.obsidian.title")}
        description={t("settings.notes.obsidian.description")}
      >
        <ObsidianPluginRow grouped={true} />
      </Sheet>
      <Sheet
        title={t("settings.notes.google.title")}
        description={t("settings.notes.google.description")}
      >
        <GoogleDocsRow grouped={true} />
      </Sheet>
    </div>
  );
};
