import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { GoogleDocsRow } from "@/shorthand/notes/GoogleDocsRow";
import { GoogleOAuthClientRow } from "@/shorthand/notes/GoogleOAuthClientRow";
import { NotesAgentRows } from "@/shorthand/notes/NotesAgentRow";
import { NotesTargetRow } from "@/shorthand/notes/NotesTargetRow";
import { ObsidianPluginRow } from "@/shorthand/notes/ObsidianPluginRow";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "Notes" section: where captured notes end up, and what has to
 * be in place for that destination.
 *
 * A choice of destination for meetings, then the setup for the one chosen,
 * so the page only ever asks for what the choice needs. Obsidian's setup is
 * a hand-off: the plugin runs inside Obsidian, so it is installed there.
 * Google Docs is set up here: a Google sign-in, the doc meetings go into
 * and who writes the notes, plus, under Advanced, your own OAuth client. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md
 * and docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md.
 */
export const NotesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const target = getSetting("notes_target") ?? "obsidian";

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet
        title={t("settings.notes.target.sheetTitle")}
        description={t("settings.notes.target.sheetDescription")}
      >
        <NotesTargetRow grouped={true} />
      </Sheet>
      {target === "google_docs" ? (
        <Sheet
          title={t("settings.notes.google.title")}
          description={t("settings.notes.google.description")}
        >
          <GoogleDocsRow grouped={true} />
          <NotesAgentRows grouped={true} />
          <AdvancedOnly>
            <GoogleOAuthClientRow grouped={true} />
          </AdvancedOnly>
        </Sheet>
      ) : (
        <Sheet
          title={t("settings.notes.obsidian.title")}
          description={t("settings.notes.obsidian.description")}
        >
          <ObsidianPluginRow grouped={true} />
        </Sheet>
      )}
    </div>
  );
};
