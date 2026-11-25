import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "@/hooks/useSettings";
import { GoogleDocsLayoutRow } from "@/shorthand/notes/GoogleDocsLayoutRow";
import { GoogleDocsRow } from "@/shorthand/notes/GoogleDocsRow";
import { GoogleOAuthClientRow } from "@/shorthand/notes/GoogleOAuthClientRow";
import { NotesAgentRows } from "@/shorthand/notes/NotesAgentRow";
import { NotesPromptsRows } from "@/shorthand/notes/NotesPromptsRows";
import { NotesTargetRow } from "@/shorthand/notes/NotesTargetRow";
import { ObsidianPluginRow } from "@/shorthand/notes/ObsidianPluginRow";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { Sheet } from "@/shorthand/ui/Sheet";
import { TabPanel, Tabs } from "@/shorthand/ui/Tabs";

type NotesTab = "setup" | "prompts";

/**
 * Fork-only "Notes" section: where captured notes end up, and what has to
 * be in place for that destination.
 *
 * A choice of destination for meetings, then the setup for the one chosen,
 * so the page only ever asks for what the choice needs. Obsidian's setup is
 * a hand-off: the plugin runs inside Obsidian, so it is installed there.
 * Google Docs is set up here: one doc with a tab per meeting or a folder
 * with a doc per meeting, the Google sign-in and doc or folder, and who
 * writes the notes, plus, under Advanced, your own OAuth client. The
 * note-taking prompts the note taker follows have a tab of their own, using
 * the same tab bar as Modes, because together they made the page too long.
 * The bar only shows for Google Docs, the one target with prompts. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md
 * and docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md.
 */
export const NotesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const target = getSetting("notes_target") ?? "obsidian";
  const [activeTab, setActiveTab] = useState<NotesTab>("setup");

  const targetSheet = (
    <Sheet
      title={t("settings.notes.target.sheetTitle")}
      description={t("settings.notes.target.sheetDescription")}
    >
      <NotesTargetRow grouped={true} />
    </Sheet>
  );

  if (target !== "google_docs") {
    return (
      <div className="max-w-3xl w-full mx-auto space-y-8">
        {targetSheet}
        <Sheet
          title={t("settings.notes.obsidian.title")}
          description={t("settings.notes.obsidian.description")}
        >
          <ObsidianPluginRow grouped={true} />
        </Sheet>
      </div>
    );
  }

  const tabs = [
    { id: "setup" as const, label: t("settings.notes.tabs.setup") },
    { id: "prompts" as const, label: t("settings.notes.tabs.prompts") },
  ];

  // No `space-y-8` on this element: a TabPanel is a sibling of its tab bar,
  // so the gap would land between the bar and the panel. See ModesSettings.
  return (
    <div className="max-w-3xl w-full mx-auto">
      <Tabs
        tabs={tabs}
        active={activeTab}
        onChange={setActiveTab}
        label={t("settings.notes.tabs.label")}
      />
      {activeTab === "setup" && (
        <TabPanel id="setup">
          <div className="space-y-8">
            {targetSheet}
            <Sheet
              title={t("settings.notes.google.title")}
              description={t("settings.notes.google.description")}
            >
              <GoogleDocsLayoutRow grouped={true} />
              <GoogleDocsRow grouped={true} />
              <NotesAgentRows grouped={true} />
              <AdvancedOnly>
                <GoogleOAuthClientRow grouped={true} />
              </AdvancedOnly>
            </Sheet>
          </div>
        </TabPanel>
      )}
      {activeTab === "prompts" && (
        <TabPanel id="prompts">
          <Sheet
            title={t("settings.notes.prompts.title")}
            description={t("settings.notes.prompts.description")}
          >
            <NotesPromptsRows grouped={true} />
          </Sheet>
        </TabPanel>
      )}
    </div>
  );
};
