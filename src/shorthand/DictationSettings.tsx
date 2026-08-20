import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";
import { DictationToggleField } from "./dictation/DictationToggleField";
import { DictationEnableToggle } from "./dictation/DictationEnableToggle";
import { DictationPasteMethod } from "./dictation/DictationPasteMethod";
import { DictationClipboardHandling } from "./dictation/DictationClipboardHandling";
import { DictationAutoSubmit } from "./dictation/DictationAutoSubmit";
import { DictationTypingTool } from "./dictation/DictationTypingTool";
import { DictationShowOverlay } from "./dictation/DictationShowOverlay";
import { DictationPostProcessPrompt } from "./dictation/DictationPostProcessPrompt";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Dictation" section. See
 * docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md for
 * the full row inventory and rationale. Rows below the enable toggle stay
 * mounted and are individually disabled rather than hidden while dictation
 * is off; the Accessibility row and the AI-cleanup prompt picker are the two
 * exceptions (see inline comments).
 */
export const DictationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;
  const postProcessEnabled =
    dictationEnabled && (dictation?.post_process_enabled ?? false);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup>
        <DictationEnableToggle />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.shortcut")}>
        {/* Not rendered-disabled like the row below: SettingContainer's
            `disabled` prop only fades the label text, it never reaches
            these rows' key-recorder chip or Reset button, so a disabled
            row here would still register a live global shortcut while
            dictation is off. Hide instead of disable until that is fixed
            upstream. */}
        {dictationEnabled && (
          <ShortcutInput shortcutId="dictate" grouped={true} />
        )}
        <DictationToggleField
          field="push_to_talk"
          label={t("settings.general.pushToTalk.label")}
          description={t("settings.general.pushToTalk.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        {dictationEnabled && (
          <ShortcutInput
            shortcutId="dictate_with_post_process"
            grouped={true}
          />
        )}
      </SettingsGroup>

      {/* Not disabled-when-off like the rows above: AccessibilityPermissions
          has no disabled prop, only self-hide/show-a-Grant-button states, so
          gating on dictationEnabled means not rendering it at all rather
          than rendering it inert. */}
      {dictationEnabled && <AccessibilityPermissions />}

      <SettingsGroup title={t("settings.advanced.groups.output")}>
        <DictationPasteMethod grouped={true} disabled={!dictationEnabled} />
        <DictationTypingTool grouped={true} disabled={!dictationEnabled} />
        <DictationClipboardHandling
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationAutoSubmit grouped={true} disabled={!dictationEnabled} />
        <DictationToggleField
          field="append_trailing_space"
          label={t("settings.debug.appendTrailingSpace.label")}
          description={t("settings.debug.appendTrailingSpace.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationShowOverlay grouped={true} disabled={!dictationEnabled} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.aiCleanup")}>
        <DictationToggleField
          field="post_process_enabled"
          label={t("settings.debug.postProcessingToggle.label")}
          description={t("settings.debug.postProcessingToggle.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        {/* Disabled whenever post-processing itself is off, not just when
            dictation is off — picking a prompt for a toggle that won't run
            is a dead control. */}
        <DictationPostProcessPrompt
          grouped={true}
          disabled={!postProcessEnabled}
        />
      </SettingsGroup>
      <p className="px-4 text-xs text-mid-gray">
        {t("settings.dictation.postProcessing.hint")}
      </p>

      <SettingsGroup title={t("settings.dictation.groups.privacy")}>
        <DictationToggleField
          field="save_recordings"
          label={t("settings.dictation.privacy.saveRecordings.label")}
          description={t(
            "settings.dictation.privacy.saveRecordings.description",
          )}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationToggleField
          field="save_transcripts"
          label={t("settings.dictation.privacy.saveTranscripts.label")}
          description={t(
            "settings.dictation.privacy.saveTranscripts.description",
          )}
          grouped={true}
          disabled={!dictationEnabled}
        />
      </SettingsGroup>

      <p className="px-4 text-xs text-mid-gray">
        {t("settings.dictation.footer")}
      </p>
    </div>
  );
};
