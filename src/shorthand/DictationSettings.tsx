import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";
import { DictationToggleField } from "./dictation/DictationToggleField";
import { DictationEnableToggle } from "./dictation/DictationEnableToggle";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Dictation" section: the opt-in dictation mode that runs
 * alongside meeting transcription, with its own shortcuts and settings. See
 * docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.
 *
 * Rows below the enable toggle stay mounted and are individually disabled
 * rather than hidden while dictation is off, so the section previews what
 * enabling buys instead of reading as empty/broken. The Accessibility row is
 * the one exception: AccessibilityPermissions has no disabled state of its
 * own (it either self-hides or offers a live Grant button), so it is gated
 * on dictationEnabled by not rendering it at all rather than by disabling it.
 *
 * Extended by Task 9 with Output, AI Cleanup, Privacy groups and a footer
 * line; see that task for the full replacement of this file's body.
 */
export const DictationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup>
        <DictationEnableToggle />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.shortcut")}>
        <ShortcutInput
          shortcutId="dictate"
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationToggleField
          field="push_to_talk"
          label={t("settings.general.pushToTalk.label")}
          description={t("settings.general.pushToTalk.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <ShortcutInput
          shortcutId="dictate_with_post_process"
          grouped={true}
          disabled={!dictationEnabled}
        />
      </SettingsGroup>

      {dictationEnabled && <AccessibilityPermissions />}
    </div>
  );
};
