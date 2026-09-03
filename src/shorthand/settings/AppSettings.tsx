import React from "react";
import { useTranslation } from "react-i18next";
import { AppLanguageSelector } from "@/components/settings/AppLanguageSelector";
import { AudioFeedback } from "@/components/settings/AudioFeedback";
import { AutostartToggle } from "@/components/settings/AutostartToggle";
import { ExperimentalToggle } from "@/components/settings/ExperimentalToggle";
import { LazyStreamClose } from "@/components/settings/LazyStreamClose";
import { OutputDeviceSelector } from "@/components/settings/OutputDeviceSelector";
import { ShowTrayIcon } from "@/components/settings/ShowTrayIcon";
import { ShowWhatsNewOnUpdate } from "@/components/settings/ShowWhatsNewOnUpdate";
import { SoundPicker } from "@/components/settings/SoundPicker";
import { StartHidden } from "@/components/settings/StartHidden";
import { ThemeSelector } from "@/components/settings/ThemeSelector";
import { UpdateChecksToggle } from "@/components/settings/UpdateChecksToggle";
import { VolumeSlider } from "@/components/settings/VolumeSlider";
import { KeyboardImplementationSelector } from "@/components/settings/debug/KeyboardImplementationSelector";
import { useSettings } from "@/hooks/useSettings";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { Dependents } from "@/shorthand/ui/Dependents";
import { OverlayPositionRow } from "@/shorthand/ui/OverlayRows";
import { Sheet } from "@/shorthand/ui/Sheet";
import { TelemetryToggle } from "@/shorthand/telemetry/TelemetryToggle";

/**
 * Fork-only "App" section: how the application starts, how it looks, and how
 * it tells you it is listening. Everything here is shared — nothing in this
 * file has a `DictationSettings` counterpart, so by the rule in Part 2 of
 * `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md` it
 * appears exactly once, and this is where.
 *
 * Fork-owned rather than an edit to upstream's General/Advanced screens
 * because it is a different information architecture, not a restyle: it draws
 * from four upstream screens at once (General, Advanced, Debug, About) and
 * splits each row into default or Advanced. Upstream's screens stay in the
 * tree untouched and unregistered, so a change there still merges cleanly.
 *
 * Advanced rows are unmounted, not hidden — see `AdvancedOnly`.
 */
export const AppSettings: React.FC = () => {
  const { t } = useTranslation();
  const { audioFeedbackEnabled } = useSettings();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet title={t("settings.advanced.groups.app")}>
        <ThemeSelector descriptionMode="inline" grouped={true} />
        <AppLanguageSelector descriptionMode="inline" grouped={true} />
        <AutostartToggle descriptionMode="inline" grouped={true} />
        <TelemetryToggle descriptionMode="inline" grouped={true} />
        <AdvancedOnly>
          <StartHidden descriptionMode="tooltip" grouped={true} />
          <ShowTrayIcon descriptionMode="tooltip" grouped={true} />
          {/* `UpdateChecksToggle` is Debug-only upstream. Promoting it is
              deliberate: whether the app phones home for a new version is a
              user-facing preference, not a diagnostic, and burying it in Debug
              means the only people who can turn it off are the ones who least
              need to. */}
          <UpdateChecksToggle descriptionMode="tooltip" grouped={true} />
          <ShowWhatsNewOnUpdate descriptionMode="tooltip" grouped={true} />
        </AdvancedOnly>
      </Sheet>

      <Sheet title={t("settings.sound.title")}>
        <AudioFeedback descriptionMode="inline" grouped={true} />
        <AdvancedOnly>
          {/* All three follow the feedback toggle: an output device, a volume
              and a sound theme for sounds that are never played are dead
              controls. They were greyed out before, which said they were
              unavailable without ever saying what would make them available;
              nesting them under the toggle says both at once. See
              ui/Dependents.

              `SoundPicker` was not even greyed — it takes no `disabled` prop —
              so it sat live next to two dead rows governed by the same switch.
              Hiding the block fixes that without an edit to an upstream
              file. */}
          <Dependents on={audioFeedbackEnabled}>
            <OutputDeviceSelector descriptionMode="tooltip" grouped={true} />
            {/* `VolumeSlider` takes only `disabled` — it hardcodes its own
                tooltip description and exposes no `descriptionMode`. It is an
                Advanced row, which is allowed to keep a tooltip, so it keeps
                one rather than earning an edit to an upstream file. */}
            <VolumeSlider />
            {/* `SoundPicker` is Debug-only upstream, and promoting it is the
                same deliberate call as `UpdateChecksToggle`: which sound plays
                when recording starts is a preference, not a diagnostic. It
                takes `label`/`description` strings instead of the usual
                title/description key pair, and has no `descriptionMode` at
                all. */}
            <SoundPicker
              label={t("settings.debug.soundTheme.label")}
              description={t("settings.debug.soundTheme.description")}
            />
          </Dependents>
        </AdvancedOnly>
      </Sheet>

      <AdvancedOnly>
        {/* Only the shared position lives here. The per-mode `overlay_style`
            rows belong to the Modes tabs; `OverlayRows` splits the two so
            upstream's combined `ShowOverlay` is left untouched. */}
        <Sheet title={t("settings.advanced.overlay.style.title")}>
          <OverlayPositionRow descriptionMode="tooltip" grouped={true} />
        </Sheet>

        <Sheet title={t("settings.advanced.groups.experimental")}>
          {/* `FollowStreamOutput` used to be the first row here. It moved to
              Modes: following the live transcript is a per-mode field now, so
              by the rule above it no longer belongs in this file, which is
              only for settings that appear exactly once. */}
          <ExperimentalToggle descriptionMode="tooltip" grouped={true} />
          <LazyStreamClose descriptionMode="tooltip" grouped={true} />
          <KeyboardImplementationSelector
            descriptionMode="tooltip"
            grouped={true}
          />
        </Sheet>
      </AdvancedOnly>
    </div>
  );
};
