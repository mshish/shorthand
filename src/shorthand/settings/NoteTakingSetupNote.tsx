import React from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";

/**
 * The setup guide on shorthand.ing, opened at its first step: installing the
 * assistant and signing in. That step is the one this row exists to point
 * at; the rest of the guide is a scroll away. `docs/AI_NOTE_TAKING.md` is the
 * same guide in this repository, for readers who are already on GitHub.
 */
const SETUP_GUIDE_URL = "https://shorthand.ing/setup#step-1";

interface NoteTakingSetupNoteProps {
  grouped?: boolean;
}

/**
 * Fork-only row pointing at the note-taking setup guide.
 *
 * Neither notetaking mode writes the note itself — a follower does, by driving
 * a Claude Code or Codex CLI the user has installed and signed in to. Nothing
 * else in this pane says so, and the failure that causes is silent from here:
 * capture and transcription work, the note never appears, and every setting
 * that looks like it governs note taking is set correctly. The prerequisite
 * lives outside the app, so the row explaining it has to come before the
 * controls in both notetaking tabs rather than as an error after a capture has
 * already been wasted.
 *
 * The row says only what the prerequisite is; the guide it links to carries
 * the how, and the fact most people do not know they already have: a paid
 * Claude or ChatGPT subscription includes the assistant, so note taking needs
 * no API key. The description was once four sentences long and outweighed
 * every control beneath it.
 */
export const NoteTakingSetupNote: React.FC<NoteTakingSetupNoteProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();

  return (
    <SettingContainer
      title={t("settings.notetaking.setup.title")}
      description={t("settings.notetaking.setup.description")}
      descriptionMode="inline"
      grouped={grouped}
    >
      <Button
        variant="secondary"
        size="md"
        onClick={() => openUrl(SETUP_GUIDE_URL)}
      >
        {t("settings.notetaking.setup.button")}
      </Button>
    </SettingContainer>
  );
};
