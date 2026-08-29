import React from "react";
import { useTranslation } from "react-i18next";
import { type } from "@tauri-apps/plugin-os";
import { MicrophoneSelector } from "@/components/settings/MicrophoneSelector";
import { ChannelSelector } from "@/components/settings/ChannelSelector";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import { PushToTalk } from "@/components/settings/PushToTalk";
import { MuteWhileRecording } from "@/components/settings/MuteWhileRecording";
import { VoiceActivityDetection } from "@/components/settings/VoiceActivityDetection";
import { ShowOverlay } from "@/components/settings/ShowOverlay";
import { SystemAudioCapture } from "@/components/settings/advanced/SystemAudioCapture";
import { SystemAudioDeviceSelector } from "@/components/settings/advanced/SystemAudioDeviceSelector";
import { FollowStreamOutput } from "@/components/settings/advanced/FollowStreamOutput";
import { SaveRecordings } from "@/components/settings/SaveRecordings";
import { SaveTranscripts } from "@/components/settings/SaveTranscripts";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";

/**
 * Fork-only "Capture" section: shortcuts and audio-input settings that drive
 * Shorthand's recorder. Replaces upstream's general/advanced sections in the
 * simplified (default) profile; see `src/shorthand/visibility.ts`.
 */
export const CaptureSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const pushToTalk = getSetting("push_to_talk");
  const isLinux = type() === "linux";

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.general.shortcut.title")}>
        <ShortcutInput shortcutId="transcribe" grouped={true} />
        <PushToTalk descriptionMode="tooltip" grouped={true} />
        {/* Cancel shortcut is hidden with push-to-talk (release key cancels) and on Linux (dynamic shortcut instability) */}
        {!isLinux && !pushToTalk && (
          <ShortcutInput shortcutId="cancel" grouped={true} />
        )}
      </SettingsGroup>
      <SettingsGroup title={t("settings.sound.title")}>
        <MicrophoneSelector descriptionMode="tooltip" grouped={true} />
        <ChannelSelector descriptionMode="tooltip" grouped={true} />
        <SystemAudioCapture descriptionMode="tooltip" grouped={true} />
        <SystemAudioDeviceSelector descriptionMode="tooltip" grouped={true} />
        <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
        <VoiceActivityDetection descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <SettingsGroup>
        <ShowOverlay descriptionMode="tooltip" grouped={true} />
        <FollowStreamOutput descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
      <SettingsGroup title={t("settings.privacy.title")}>
        <SaveRecordings descriptionMode="tooltip" grouped={true} />
        <SaveTranscripts descriptionMode="tooltip" grouped={true} />
      </SettingsGroup>
    </div>
  );
};
