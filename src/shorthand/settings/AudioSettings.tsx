import React from "react";
import { useTranslation } from "react-i18next";
import { MicrophoneSelector } from "@/components/settings/MicrophoneSelector";
import { ChannelSelector } from "@/components/settings/ChannelSelector";
import { MuteWhileRecording } from "@/components/settings/MuteWhileRecording";
import { VoiceActivityDetection } from "@/components/settings/VoiceActivityDetection";
import { AlwaysOnMicrophone } from "@/components/settings/AlwaysOnMicrophone";
import { ClamshellMicrophoneSelector } from "@/components/settings/ClamshellMicrophoneSelector";
import { SystemAudioCapture } from "@/components/settings/advanced/SystemAudioCapture";
import { SystemAudioDeviceSelector } from "@/components/settings/advanced/SystemAudioDeviceSelector";
import { Sheet } from "@/shorthand/ui/Sheet";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";

/**
 * Fork-only "Audio" section: what the recorder listens to, and nothing else.
 *
 * These rows are grouped because they all answer one question — where the
 * sound comes from. Anything that shapes the words after capture (model,
 * language, filler words) is Model; anything where the app makes sound at the
 * user rather than listening to them (output device, feedback, volume) is App.
 * Drawing the line at input is what keeps the section explicable in a sentence.
 *
 * Replaces the audio half of `src/shorthand/CaptureSettings.tsx`; the shortcut
 * and overlay rows that file also carried belong to the Modes pane. See Part 2
 * of `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md` for the
 * full destination map. Registration is handled elsewhere.
 *
 * Advanced rows are revealed in place rather than in a separate group, so the
 * page a user already knows grows instead of being replaced.
 */
export const AudioSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet title={t("settings.audio.groups.input")}>
        <MicrophoneSelector descriptionMode="inline" grouped={true} />
        {/* Both system-audio rows self-hide outside Windows, so the default
            row count is platform-dependent. Only the capture toggle also
            checks model capability; the device selector checks stored
            enablement and mute state but never the model. That asymmetry is
            upstream's and is deliberately left alone here. */}
        <SystemAudioCapture descriptionMode="inline" grouped={true} />
        <SystemAudioDeviceSelector descriptionMode="inline" grouped={true} />
        <AdvancedOnly>
          <ChannelSelector descriptionMode="tooltip" grouped={true} />
          <MuteWhileRecording descriptionMode="tooltip" grouped={true} />
          <VoiceActivityDetection descriptionMode="tooltip" grouped={true} />
          {/* AlwaysOnMicrophone and ClamshellMicrophoneSelector are reachable
              only from the Debug section today. Promoting them into
              Audio/Advanced is deliberate: both are preferences about which
              microphone is used and when it is held open — user-facing
              choices, not diagnostics — and Debug is for things you look at
              when something is wrong, not things you set once and forget. */}
          <AlwaysOnMicrophone descriptionMode="tooltip" grouped={true} />
          <ClamshellMicrophoneSelector
            descriptionMode="tooltip"
            grouped={true}
          />
        </AdvancedOnly>
      </Sheet>
    </div>
  );
};
