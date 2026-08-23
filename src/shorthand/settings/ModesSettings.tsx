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
import { FollowStreamOutput } from "@/components/settings/advanced/FollowStreamOutput";
import { SystemAudioCapture } from "@/components/settings/advanced/SystemAudioCapture";
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
import { Dependents } from "../ui/Dependents";
import { useAdvanced } from "../useAdvanced";
import { OverlayStyleRow } from "../ui/OverlayRows";
import { Sheet } from "../ui/Sheet";
import { TabPanel, Tabs } from "../ui/Tabs";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Modes" section: one tab per capture mode over the rows that
 * genuinely differ between them, and a single shared group beneath.
 *
 * The two modes are Meetings and Dictation. "Meetings" is a user-facing name
 * only: the binding ids (`transcribe`, `transcribe_with_post_process`) and
 * every Rust field keep the transcription wording, because both modes
 * transcribe — renaming those would be renaming the machinery rather than the
 * mode, and would cost a conflict against upstream for nothing.
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

type ModeTab = "meetings" | "dictation";

export const ModesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [activeTab, setActiveTab] = useState<ModeTab>("meetings");

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
  // System-audio capture is a Windows-only feature upstream. The shared
  // `SystemAudioCapture` row self-hides on other platforms; the dictation row
  // below is a plain boolean field with no such guard, so it needs this or it
  // offers a toggle for something that cannot happen.
  const isWindows = type() === "windows";
  // Dictation's push-to-talk only counts when dictation is actually on.
  // Without the `dictationEnabled &&`, a disabled mode's default suppresses
  // the row for everyone: dictation ships with push_to_talk true, so on a
  // fresh install Cancel was hidden by a mode the user had never enabled —
  // and it stayed hidden even after meetings' own default flipped to off.
  const anyPushToTalk =
    (getSetting("push_to_talk") ?? false) ||
    (dictationEnabled && (dictation?.push_to_talk ?? false));

  // Gates the dedicated AI-cleanup hotkey in the Meetings tab, the same way
  // `postProcessEnabled` above gates the prompt picker in the Dictation one.
  // Both are per-mode fields, so neither can stand in for the other.
  const meetingsPostProcessEnabled =
    getSetting("post_process_enabled") ?? false;

  // Read directly rather than via <AdvancedOnly>, because the shared group's
  // heading has to be inside the same condition as its only row.
  const { advanced } = useAdvanced();

  const tabs = [
    { id: "meetings" as const, label: t("settings.modes.tabs.meetings") },
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

      {activeTab === "meetings" && (
        <TabPanel id="meetings">
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
            {/* Meeting mode's copy of system-audio capture. The toggle is
                per-mode now — the Dictation tab has its own row bound to
                `dictation.system_audio_enabled` — but the *device* being
                captured is still shared and still lives in Audio, because
                there is only one of it. This row self-hides outside Windows. */}
            <SystemAudioCapture descriptionMode="inline" grouped={true} />
            <SaveRecordings descriptionMode="inline" grouped={true} />
            <SaveTranscripts descriptionMode="inline" grouped={true} />
            <AdvancedOnly>
              {/* The AI-cleanup rows are Advanced in this tab and stay in the
                  default view on the Dictation tab. The asymmetry is
                  deliberate and was asked for: cleanup is a routine part of
                  dictating and an occasional one in a meeting, so each tab
                  shows it at the weight that mode gives it. Do not "fix" this
                  by making the two tabs match. */}
              <PostProcessingToggle descriptionMode="tooltip" grouped={true} />
              {/* The one row this toggle unlocks, drawn as belonging to it —
                  see ui/Dependents. It was already hidden rather than
                  disabled, because a "disabled" shortcut row still binds a
                  live global key; what it was missing was any indication that
                  the row above is what makes it appear. */}
              <Dependents on={meetingsPostProcessEnabled}>
                <ShortcutInput
                  shortcutId="transcribe_with_post_process"
                  descriptionMode="tooltip"
                  grouped={true}
                />
              </Dependents>
              {/* Meeting mode's copy of the follow-stream field; Dictation has
                  its own, also Advanced. It is a hook for local tooling, not
                  something anyone sets while learning the app. */}
              <FollowStreamOutput descriptionMode="tooltip" grouped={true} />
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
            {/* Everything below is hidden, not greyed, while dictation is off.
                It used to be a wall of about a dozen disabled rows, which said
                "this mode has a lot of settings" at exactly the moment the user
                had not chosen the mode. Hiding them makes the off state one
                switch.

                It also removes a hazard the greyed version carried:
                SettingContainer's `disabled` prop fades the title and nothing
                else, so a "disabled" ShortcutInput still registered a live
                global hotkey for a mode that was switched off. Two rows here
                were already hiding rather than disabling for that reason; this
                makes the whole panel consistent instead of carrying the
                exception in a comment. */}
            {dictationEnabled && (
              <>
                <ShortcutInput
                  shortcutId="dictate"
                  descriptionMode="inline"
                  grouped={true}
                />
                <DictationToggleField
                  field="push_to_talk"
                  label={t("settings.general.pushToTalk.label")}
                  description={t("settings.general.pushToTalk.description")}
                  descriptionMode="inline"
                  grouped={true}
                />
                <AccessibilityPermissions />
                {/* Tooltip, matching the Meetings tab. Its description runs to
                    six lines; left inline here it became the loudest thing on
                    this tab, which is the defect that was just fixed on the
                    other one. Same setting, same weight. */}
                <DictationOverlayStyleRow
                  descriptionMode="tooltip"
                  grouped={true}
                />
                {/* Dictation's copy of system-audio capture. It borrows the
                    shared row's strings, because it is the same setting for
                    the other mode, and its Windows-only guard, because
                    DictationToggleField is a plain boolean row with no
                    predicates of its own. Without the guard this offers macOS
                    and Linux users a switch for a feature that does not exist
                    there, in the one tab where the Meetings row correctly
                    hides itself. */}
                {isWindows && (
                  <DictationToggleField
                    field="system_audio_enabled"
                    label={t("settings.advanced.systemAudio.label")}
                    description={t("settings.advanced.systemAudio.description")}
                    descriptionMode="inline"
                    grouped={true}
                  />
                )}
                {/* In the default view here and Advanced on the Meetings tab.
                    See the note above that tab's <AdvancedOnly>. */}
                <DictationToggleField
                  field="post_process_enabled"
                  label={t("settings.debug.postProcessingToggle.label")}
                  description={t(
                    "settings.debug.postProcessingToggle.description",
                  )}
                  descriptionMode="inline"
                  grouped={true}
                />
                {/* Both rows cleanup unlocks, gathered directly beneath it.
                    See ui/Dependents. The hotkey used to render eight rows
                    higher, beside the plain dictation shortcut, so turning
                    cleanup on made one row appear in a part of the pane the
                    user was not looking at and another appear further down.
                    One toggle, one place its consequences show up. */}
                <Dependents on={postProcessEnabled}>
                  <ShortcutInput
                    shortcutId="dictate_with_post_process"
                    descriptionMode="inline"
                    grouped={true}
                  />
                  <DictationPostProcessPrompt grouped={true} />
                </Dependents>
                <DictationToggleField
                  field="save_recordings"
                  label={t("settings.dictation.privacy.saveRecordings.label")}
                  description={t(
                    "settings.dictation.privacy.saveRecordings.description",
                  )}
                  descriptionMode="inline"
                  grouped={true}
                />
                <DictationToggleField
                  field="save_transcripts"
                  label={t("settings.dictation.privacy.saveTranscripts.label")}
                  description={t(
                    "settings.dictation.privacy.saveTranscripts.description",
                  )}
                  descriptionMode="inline"
                  grouped={true}
                />
                {/* Default here and advanced on the Meetings tab: a field is
                    shown by default in the tab where it is load-bearing, and
                    dictation is the mode whose entire job is putting text into
                    another window. */}
                <DictationPasteMethod descriptionMode="inline" grouped={true} />
                <AdvancedOnly>
                  {/* Dictation's copy of the follow-stream field. Advanced in
                      both tabs, unlike AI cleanup: a tooling hook is a tooling
                      hook in either mode. */}
                  <DictationToggleField
                    field="follow_stream_enabled"
                    label={t("settings.advanced.followStream.label")}
                    description={t(
                      "settings.advanced.followStream.description",
                    )}
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <DictationTypingTool
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <DictationClipboardHandling
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <DictationAutoSubmit
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <DictationToggleField
                    field="append_trailing_space"
                    label={t("settings.debug.appendTrailingSpace.label")}
                    description={t(
                      "settings.debug.appendTrailingSpace.description",
                    )}
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                </AdvancedOnly>
              </>
            )}
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
