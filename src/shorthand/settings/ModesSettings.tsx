import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { PushToTalk } from "@/components/settings/PushToTalk";
import { PasteMethodSetting } from "@/components/settings/PasteMethod";
import { TypingToolSetting } from "@/components/settings/TypingTool";
import { ClipboardHandlingSetting } from "@/components/settings/ClipboardHandling";
import { AutoSubmit } from "@/components/settings/AutoSubmit";
import { AppendTrailingSpace } from "@/components/settings/AppendTrailingSpace";
import { PostProcessingToggle } from "@/components/settings/PostProcessingToggle";
import { SaveRecordings } from "@/components/settings/SaveRecordings";
import { SaveTranscripts } from "@/components/settings/SaveTranscripts";
import { useSettings } from "@/hooks/useSettings";
import { DictationAutoSubmit } from "../dictation/DictationAutoSubmit";
import { DictationClipboardHandling } from "../dictation/DictationClipboardHandling";
import { DictationEnableToggle } from "../dictation/DictationEnableToggle";
import { DictationPasteMethod } from "../dictation/DictationPasteMethod";
import { DictationPostProcessPrompt } from "../dictation/DictationPostProcessPrompt";
import { DictationOverlayStyleRow } from "../ui/OverlayRows";
import { DictationToggleField } from "../dictation/DictationToggleField";
import { DictationTypingTool } from "../dictation/DictationTypingTool";
import { AdvancedOnly } from "../ui/AdvancedOnly";
import { useAdvanced } from "../useAdvanced";
import { OverlayStyleRow } from "../ui/OverlayRows";
import { Sheet } from "../ui/Sheet";
import { TabPanel, Tabs } from "../ui/Tabs";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Modes" section: one tab per capture mode over the rows that
 * genuinely differ between them, and a single shared group beneath.
 *
 * What governs membership of this pane, from
 * docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md Part 2:
 *
 *   A row is per-mode iff it has a `DictationSettings` counterpart **or** a
 *   mode-specific binding id. Everything else is shared and appears exactly
 *   once — which is why `cancel` renders below the tabs rather than inside
 *   both of them, and why `overlay_position` is not a row of its own here.
 *
 * Three things sit outside that rule, and are listed rather than bent to fit,
 * because a rule with unstated exceptions is not a rule:
 *
 *   1. `dictation.enabled` — a meta-control, not a setting. It has no
 *      counterpart because meeting mode cannot be switched off. It renders as
 *      the Dictation tab's first row, so the mode is discoverable rather than
 *      hidden behind its own absence.
 *   2. `AccessibilityPermissions` — an OS permission prompt, not a persisted
 *      setting at all. It renders in the Dictation tab, because that is the
 *      mode that needs the permission.
 *   3. `external_script_path` — bound inside `PasteMethod` and revealed only
 *      by the Linux-only "external script" method. It has no dictation
 *      counterpart, so by the rule it is shared; in practice it travels with
 *      whichever paste-method row is on screen. Named here so that wrapping
 *      `PasteMethod` cannot silently lose it.
 *
 * Which tab is open is deliberately component state, not a setting. It is a
 * view position rather than a preference, and persisting it would open the app
 * on a screen describing a mode the user may have since switched off.
 */

type ModeTab = "transcription" | "dictation";

export const ModesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [activeTab, setActiveTab] = useState<ModeTab>("transcription");

  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;
  const postProcessEnabled =
    dictationEnabled && (dictation?.post_process_enabled ?? false);

  // Cancel's two existing predicates, carried over from GeneralSettings and
  // CaptureSettings: hidden on Linux (dynamic-shortcut instability), and
  // hidden while push-to-talk is on (releasing the key already cancels).
  // Push-to-talk is per-mode now, so the row survives only while *neither*
  // mode has it — the strictly safer reading, since a visible-but-redundant
  // shortcut beats a hidden one that still fires.
  const isLinux = type() === "linux";
  const anyPushToTalk =
    (getSetting("push_to_talk") ?? false) || (dictation?.push_to_talk ?? false);

  // Gates the dedicated AI-cleanup hotkey in the Transcription tab, the same
  // way `postProcessEnabled` above gates the prompt picker in the Dictation
  // one. Both are per-mode fields, so neither can stand in for the other.
  const transcriptionPostProcessEnabled =
    getSetting("post_process_enabled") ?? false;

  // Read directly rather than via <AdvancedOnly>, because the shared group's
  // heading has to be inside the same condition as its only row.
  const { advanced } = useAdvanced();

  const tabs = [
    { id: "transcription" as const, label: t("sidebar.transcription") },
    { id: "dictation" as const, label: t("sidebar.dictation") },
  ];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Tabs
        tabs={tabs}
        active={activeTab}
        onChange={setActiveTab}
        label={t("settings.modes.tabs.label")}
      />

      {activeTab === "transcription" && (
        <TabPanel id="transcription">
          <Sheet>
            <ShortcutInput
              shortcutId="transcribe"
              descriptionMode="inline"
              grouped={true}
            />
            <PushToTalk descriptionMode="inline" grouped={true} />
            {/* Tooltip, not inline: this row's description runs to six lines
                and was visually the loudest thing in the default view — for a
                secondary setting. "Descriptions inline by default" is a good
                rule right up to the point where one description outweighs
                every control around it. */}
            <OverlayStyleRow descriptionMode="tooltip" grouped={true} />
            <PostProcessingToggle descriptionMode="inline" grouped={true} />
            {/* Hidden rather than disabled while cleanup is off. A shortcut
                row is never inert: SettingContainer's `disabled` only fades
                the label, so the recorder would still bind a live global key
                for a feature that will not run. Showing it under a toggle
                that is off also reads as a broken control. */}
            {transcriptionPostProcessEnabled && (
              <ShortcutInput
                shortcutId="transcribe_with_post_process"
                descriptionMode="inline"
                grouped={true}
              />
            )}
            <SaveRecordings descriptionMode="inline" grouped={true} />
            <SaveTranscripts descriptionMode="inline" grouped={true} />
            <AdvancedOnly>
              <PasteMethodSetting descriptionMode="tooltip" grouped={true} />
              <TypingToolSetting descriptionMode="tooltip" grouped={true} />
              <ClipboardHandlingSetting
                descriptionMode="tooltip"
                grouped={true}
              />
              <AutoSubmit descriptionMode="tooltip" grouped={true} />
              <AppendTrailingSpace descriptionMode="tooltip" grouped={true} />
            </AdvancedOnly>
          </Sheet>
        </TabPanel>
      )}

      {activeTab === "dictation" && (
        <TabPanel id="dictation">
          <Sheet>
            <DictationEnableToggle />
            {/* Hidden, not disabled, while dictation is off:
                SettingContainer's `disabled` prop only fades the label text,
                it never reaches these rows' key-recorder chip or Reset
                button, so a disabled row here would still register a live
                global shortcut. */}
            {dictationEnabled && (
              <ShortcutInput
                shortcutId="dictate"
                descriptionMode="inline"
                grouped={true}
              />
            )}
            <DictationToggleField
              field="push_to_talk"
              label={t("settings.general.pushToTalk.label")}
              description={t("settings.general.pushToTalk.description")}
              descriptionMode="inline"
              grouped={true}
              disabled={!dictationEnabled}
            />
            {/* Gated on cleanup being ON, not merely on dictation being on —
                the same rule as the Transcription tab. A hotkey that always
                applies AI cleanup is a dead control while cleanup is off, and
                a shortcut row is never inert: it would still bind a live global
                key. The two tabs have to agree about this or the same row looks
                broken in one of them. */}
            {postProcessEnabled && (
              <ShortcutInput
                shortcutId="dictate_with_post_process"
                descriptionMode="inline"
                grouped={true}
              />
            )}
            {/* Also hidden rather than disabled, for a different reason:
                AccessibilityPermissions has no disabled state at all, only
                self-hide and show-a-Grant-button, so gating it on dictation
                means not rendering it rather than rendering it inert. */}
            {dictationEnabled && <AccessibilityPermissions />}
            {/* Tooltip, matching the Transcription tab. Its description runs
                to six lines; left inline here it became the loudest thing on
                this tab, which is the defect that was just fixed on the other
                one. Same setting, same weight. */}
            <DictationOverlayStyleRow
              descriptionMode="tooltip"
              grouped={true}
              disabled={!dictationEnabled}
            />
            <DictationToggleField
              field="post_process_enabled"
              label={t("settings.debug.postProcessingToggle.label")}
              description={t("settings.debug.postProcessingToggle.description")}
              descriptionMode="inline"
              grouped={true}
              disabled={!dictationEnabled}
            />
            {/* Hidden, not disabled — the same rule as the AI-cleanup
                shortcut above and on the other tab. A greyed-out prompt picker
                under an off toggle is a dead control, and it was the last row
                where the two tabs still disagreed: Transcription showed no
                prompt row at all while Dictation showed a disabled one. */}
            {postProcessEnabled && (
              <DictationPostProcessPrompt grouped={true} />
            )}
            <DictationToggleField
              field="save_recordings"
              label={t("settings.dictation.privacy.saveRecordings.label")}
              description={t(
                "settings.dictation.privacy.saveRecordings.description",
              )}
              descriptionMode="inline"
              grouped={true}
              disabled={!dictationEnabled}
            />
            <DictationToggleField
              field="save_transcripts"
              label={t("settings.dictation.privacy.saveTranscripts.label")}
              description={t(
                "settings.dictation.privacy.saveTranscripts.description",
              )}
              descriptionMode="inline"
              grouped={true}
              disabled={!dictationEnabled}
            />
            {/* Default here and advanced on the Transcription tab: a field is
                shown by default in the tab where it is load-bearing, and
                dictation is the mode whose entire job is putting text into
                another window. */}
            <DictationPasteMethod
              descriptionMode="inline"
              grouped={true}
              disabled={!dictationEnabled}
            />
            <AdvancedOnly>
              <DictationTypingTool
                descriptionMode="tooltip"
                grouped={true}
                disabled={!dictationEnabled}
              />
              <DictationClipboardHandling
                descriptionMode="tooltip"
                grouped={true}
                disabled={!dictationEnabled}
              />
              <DictationAutoSubmit
                descriptionMode="tooltip"
                grouped={true}
                disabled={!dictationEnabled}
              />
              <DictationToggleField
                field="append_trailing_space"
                label={t("settings.debug.appendTrailingSpace.label")}
                description={t(
                  "settings.debug.appendTrailingSpace.description",
                )}
                descriptionMode="tooltip"
                grouped={true}
                disabled={!dictationEnabled}
              />
            </AdvancedOnly>
          </Sheet>
        </TabPanel>
      )}

      {/* The heading is inside the guard, not outside it. Cancel is the only
          row here and it has three independent reasons to be absent — not
          advanced, on Linux, or push-to-talk on (release already cancels).
          Rendering the Sheet unconditionally left a heading and a description
          promising a setting with nothing beneath them, which is worse than
          saying nothing. */}
      {advanced && !isLinux && !anyPushToTalk && (
        <Sheet
          title={t("settings.modes.shared.title")}
          description={t("settings.modes.shared.description")}
        >
          <ShortcutInput
            shortcutId="cancel"
            descriptionMode="tooltip"
            grouped={true}
          />
        </Sheet>
      )}
    </div>
  );
};
