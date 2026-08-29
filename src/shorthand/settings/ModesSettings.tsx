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
import { DictationSystemAudioCapture } from "@/components/settings/advanced/DictationSystemAudioCapture";
import { useSettings } from "@/hooks/useSettings";
import { DictationAutoSubmit } from "../dictation/DictationAutoSubmit";
import { DictationClipboardHandling } from "../dictation/DictationClipboardHandling";
import { DictationEnableToggle } from "../dictation/DictationEnableToggle";
import { DictationPasteMethod } from "../dictation/DictationPasteMethod";
import { DictationPostProcessPrompt } from "../dictation/DictationPostProcessPrompt";
import { DictationToggleField } from "../dictation/DictationToggleField";
import { DictationTypingTool } from "../dictation/DictationTypingTool";
import { AssistedNotesClipboardHandling } from "../assisted-notes/AssistedNotesClipboardHandling";
import { AssistedNotesEnableToggle } from "../assisted-notes/AssistedNotesEnableToggle";
import { AssistedNotesPostProcessPrompt } from "../assisted-notes/AssistedNotesPostProcessPrompt";
import { AssistedNotesToggleField } from "../assisted-notes/AssistedNotesToggleField";
import { AdvancedOnly } from "../ui/AdvancedOnly";
import { Dependents } from "../ui/Dependents";
import { useAdvanced } from "../useAdvanced";
import {
  AssistedNotesOverlayStyleRow,
  DictationOverlayStyleRow,
  OverlayStyleRow,
} from "../ui/OverlayRows";
import { Sheet } from "../ui/Sheet";
import { TabPanel, Tabs } from "../ui/Tabs";
import type {
  DictationSettings as DictationSettingsType,
  AssistedNotesSettings as AssistedNotesSettingsType,
} from "@/bindings";

/**
 * Fork-only "Modes" section: one tab per capture mode over the rows that
 * genuinely differ between them, and a single shared group beneath.
 *
 * Three modes: Meetings, Assisted Notes and Dictation. Meetings and Assisted
 * Notes share a *destination* — both stream to `--follow-stream` followers —
 * so they sit together inside a "Notetaking" group; Dictation, which
 * delivers to the focused window, is a separate peer alongside that group.
 * The nesting is two independent tablists (Notetaking/Dictation, then
 * Meetings/Assisted notes inside Notetaking) rather than one flat tablist of
 * three, because a flat `role="tablist"` cannot express a subgroup without
 * inventing markup the WAI-ARIA tabs pattern does not define and screen
 * readers handle inconsistently. "Meetings" is a user-facing name only: the
 * binding ids (`transcribe`, `transcribe_with_post_process`) and every Rust
 * field keep the transcription wording, because both modes transcribe —
 * renaming those would be renaming the machinery rather than the mode, and
 * would cost a conflict against upstream for nothing.
 *
 * What governs membership of this pane, from
 * docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md Part 2:
 *
 *   A row is per-mode iff it has a counterpart on the mode's own settings
 *   struct (`DictationSettings` or `AssistedNotesSettings`), **or** a
 *   mode-specific binding id. Everything else is shared and appears exactly
 *   once — which is why `cancel` renders below the tabs rather than inside
 *   any of them, and why `overlay_position` is not a row of its own here.
 *
 * Four things sit outside that rule, and are listed rather than bent to fit,
 * because a rule with unstated exceptions is not a rule:
 *
 *   1. `dictation.enabled` / `assisted_notes.enabled` — meta-controls, not
 *      settings. Neither has a counterpart because meeting mode cannot be
 *      switched off. Each renders as its own tab's first row, so the mode is
 *      discoverable rather than hidden behind its own absence.
 *   2. `AccessibilityPermissions` — an OS permission prompt, not a persisted
 *      setting at all. It renders in the Dictation tab, because that is the
 *      mode that needs the permission.
 *   3. `external_script_path` — bound inside `PasteMethod` and revealed only
 *      by the Linux-only "external script" method. It has no dictation
 *      counterpart, so by the rule it is shared; in practice it travels with
 *      whichever paste-method row is on screen. Named here so that wrapping
 *      `PasteMethod` cannot silently lose it.
 *   4. `paste_method` — per-mode for Dictation, where it is a real choice,
 *      and a fixed invariant for Assisted Notes, where `apply_mode` always
 *      resolves it to `PasteMethod::None`. It therefore appears in the
 *      Dictation tab and nowhere else; Assisted Notes has no paste-method row
 *      at all, because there is no value for the row to control.
 *
 * Which tabs are open is deliberately component state, not a setting. It is
 * a view position rather than a preference, and persisting it would open the
 * app on a screen describing a mode the user may have since switched off.
 */

type ModeTab = "notetaking" | "dictation";
type NotetakingTab = "meetings" | "assisted";

export const ModesSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const [activeTab, setActiveTab] = useState<ModeTab>("notetaking");
  const [notetakingTab, setNotetakingTab] = useState<NotetakingTab>("meetings");

  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;
  const postProcessEnabled =
    dictationEnabled && (dictation?.post_process_enabled ?? false);

  const assistedNotes = getSetting("assisted_notes") as
    | AssistedNotesSettingsType
    | undefined;
  const assistedNotesEnabled = assistedNotes?.enabled ?? false;
  const assistedPostProcessEnabled =
    assistedNotesEnabled && (assistedNotes?.post_process_enabled ?? false);

  // Cancel's two existing predicates, carried over from GeneralSettings and
  // CaptureSettings: hidden on Linux (dynamic-shortcut instability), and
  // hidden while push-to-talk is on (releasing the key already cancels).
  // Push-to-talk is per-mode now, so the row survives only while *no* mode
  // has it — the strictly safer reading, since a visible-but-redundant
  // shortcut beats a hidden one that still fires.
  const isLinux = type() === "linux";
  // Each optional mode's push-to-talk only counts when that mode is actually
  // on. Without the `enabled &&` guards, a disabled mode's default suppresses
  // the row for everyone: dictation ships with push_to_talk true, so on a
  // fresh install Cancel was hidden by a mode the user had never enabled —
  // and it stayed hidden even after meetings' own default flipped to off.
  const anyPushToTalk =
    (getSetting("push_to_talk") ?? false) ||
    (dictationEnabled && (dictation?.push_to_talk ?? false)) ||
    (assistedNotesEnabled && (assistedNotes?.push_to_talk ?? false));

  // Gates the dedicated AI-cleanup hotkey in the Meetings tab, the same way
  // `postProcessEnabled` above gates the prompt picker in the Dictation one.
  // Both are per-mode fields, so neither can stand in for the other.
  const meetingsPostProcessEnabled =
    getSetting("post_process_enabled") ?? false;

  // Read directly rather than via <AdvancedOnly>, because the shared group's
  // heading has to be inside the same condition as its only row.
  const { advanced } = useAdvanced();

  const modeTabs = [
    {
      id: "notetaking" as const,
      label: t("settings.modes.tabs.notetaking"),
    },
    { id: "dictation" as const, label: t("sidebar.dictation") },
  ];

  const notetakingTabs = [
    { id: "meetings" as const, label: t("settings.modes.tabs.meetings") },
    {
      id: "assisted" as const,
      label: t("settings.modes.tabs.assistedNotes"),
    },
  ];

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Tabs
        tabs={modeTabs}
        active={activeTab}
        onChange={setActiveTab}
        label={t("settings.modes.tabs.label")}
      />

      {activeTab === "notetaking" && (
        <TabPanel id="notetaking">
          <Tabs
            tabs={notetakingTabs}
            active={notetakingTab}
            onChange={setNotetakingTab}
            label={t("settings.modes.tabs.notetakingLabel")}
          />

          {notetakingTab === "meetings" && (
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
                    there is only one of it. This row self-hides when the
                    current platform has no usable system-audio backend. */}
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
                  <PostProcessingToggle
                    descriptionMode="tooltip"
                    grouped={true}
                  />
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
                  {/* Meeting mode's copy of the follow-stream field; Dictation and
                      Assisted Notes each have their own, also Advanced. It is a
                      hook for local tooling, not something anyone sets while
                      learning the app. */}
                  <FollowStreamOutput
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <PasteMethodSetting
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <TypingToolSetting descriptionMode="tooltip" grouped={true} />
                  <ClipboardHandlingSetting
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                  <AutoSubmit descriptionMode="tooltip" grouped={true} />
                  <AppendTrailingSpace
                    descriptionMode="tooltip"
                    grouped={true}
                  />
                </AdvancedOnly>
              </Sheet>
            </TabPanel>
          )}

          {notetakingTab === "assisted" && (
            <TabPanel id="assisted">
              <Sheet>
                <AssistedNotesEnableToggle />
                {/* Everything below is hidden, not greyed, while assisted notes
                    is off — the same reasoning as the Dictation tab: a
                    "disabled" ShortcutInput still registers a live global
                    hotkey, and a wall of disabled rows says "this mode has a
                    lot of settings" at exactly the moment the user has not
                    chosen it. */}
                {assistedNotesEnabled && (
                  <>
                    <ShortcutInput
                      shortcutId="assisted_notes"
                      descriptionMode="inline"
                      grouped={true}
                    />
                    <AssistedNotesToggleField
                      field="push_to_talk"
                      label={t("settings.general.pushToTalk.label")}
                      description={t("settings.general.pushToTalk.description")}
                      descriptionMode="inline"
                      grouped={true}
                    />
                    {/* Tooltip, matching both other tabs: this description
                        outweighs every control around it when inline. */}
                    <AssistedNotesOverlayStyleRow
                      descriptionMode="tooltip"
                      grouped={true}
                    />
                    <AssistedNotesToggleField
                      field="save_recordings"
                      label={t(
                        "settings.assistedNotes.privacy.saveRecordings.label",
                      )}
                      description={t(
                        "settings.assistedNotes.privacy.saveRecordings.description",
                      )}
                      descriptionMode="inline"
                      grouped={true}
                    />
                    <AssistedNotesToggleField
                      field="save_transcripts"
                      label={t(
                        "settings.assistedNotes.privacy.saveTranscripts.label",
                      )}
                      description={t(
                        "settings.assistedNotes.privacy.saveTranscripts.description",
                      )}
                      descriptionMode="inline"
                      grouped={true}
                    />
                    {/* No system-audio, paste-method, typing-tool, or
                        auto-submit rows here: system audio and paste are
                        fixed mode invariants for Assisted Notes (see the
                        header comment), and the other fields are unreachable
                        because paste is always `PasteMethod::None`. */}
                    <AdvancedOnly>
                      {/* AI cleanup is Advanced for both notetaking modes —
                          matching Meetings, and matching the warning shown
                          on the AI cleanup page that enabling it for
                          notetaking is an advanced setting and is not
                          recommended. Do not move this to the default view. */}
                      <AssistedNotesToggleField
                        field="post_process_enabled"
                        label={t("settings.debug.postProcessingToggle.label")}
                        description={t(
                          "settings.debug.postProcessingToggle.description",
                        )}
                        descriptionMode="tooltip"
                        grouped={true}
                      />
                      <Dependents on={assistedPostProcessEnabled}>
                        <ShortcutInput
                          shortcutId="assisted_notes_with_post_process"
                          descriptionMode="tooltip"
                          grouped={true}
                        />
                        <AssistedNotesPostProcessPrompt grouped={true} />
                      </Dependents>
                      <AssistedNotesToggleField
                        field="follow_stream_enabled"
                        label={t("settings.advanced.followStream.label")}
                        description={t(
                          "settings.advanced.followStream.description",
                        )}
                        descriptionMode="tooltip"
                        grouped={true}
                      />
                      <AssistedNotesClipboardHandling
                        descriptionMode="tooltip"
                        grouped={true}
                      />
                      <AssistedNotesToggleField
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
                <DictationSystemAudioCapture
                  descriptionMode="inline"
                  grouped={true}
                />
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
