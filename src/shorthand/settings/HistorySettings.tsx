import React from "react";
import { useTranslation } from "react-i18next";
import { HistoryLimit } from "@/components/settings/HistoryLimit";
import { RecordingRetentionPeriodSelector } from "@/components/settings/RecordingRetentionPeriod";
import { HistorySettings as UpstreamHistorySettings } from "@/components/settings/history/HistorySettings";
import { AdvancedOnly } from "@/shorthand/ui/AdvancedOnly";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "History" section: upstream's entry list, plus the two Advanced
 * rows that govern it.
 *
 * The destination map in Part 2 of
 * `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md` puts
 * `HistoryLimit` and `RecordingRetentionPeriod` in History, behind Advanced.
 * Upstream's `HistorySettings` has nowhere to put them: it renders exactly a
 * heading, the open-recordings button and the entry card, and takes no
 * children and no props. So the fork owns the *page*, and upstream keeps
 * owning the list.
 *
 * That is also why this is a thin wrapper rather than a reimplementation. The
 * entry list is several hundred lines of pagination, audio playback,
 * re-transcription and clipboard handling; copying it to move two rows would
 * be the most expensive possible way to merge upstream's next bug fix.
 *
 * Consequence, stated rather than hidden: upstream's component draws its own
 * bordered card and its own uppercase micro-label heading, so this one
 * section keeps a card that the rest of the redesign removes. It is a
 * documented exception to "kill the card", taken deliberately because the
 * alternative is forking the list. The two rows below it are borderless, as
 * everything else is.
 */
export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <UpstreamHistorySettings />

      <AdvancedOnly>
        <Sheet title={t("settings.advanced.groups.history")}>
          <HistoryLimit descriptionMode="tooltip" grouped={true} />
          <RecordingRetentionPeriodSelector
            descriptionMode="tooltip"
            grouped={true}
          />
        </Sheet>
      </AdvancedOnly>
    </div>
  );
};
