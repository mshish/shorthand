import React from "react";
import { useTranslation } from "react-i18next";
import { useAdvanced } from "../useAdvanced";

/**
 * The advanced-settings switch, for the sidebar footer.
 *
 * A separate file so `Sidebar.tsx` — an upstream file the fork already edits —
 * gains one import and one element rather than a block of fork-only markup.
 * The smaller the footprint there, the cheaper every future merge.
 *
 * It sits in the rail rather than inside About because it changes what every
 * section shows. Putting a global display control inside one section is how it
 * ended up undiscoverable the first time.
 *
 * Not a `ToggleSwitch`: that fills a checked track with the accent at full
 * strength, and a permanently-visible switch in the rail would then be one of
 * the loudest things on screen — competing with the mark for the eye in exactly
 * the way the brand direction is trying to avoid.
 */
export const AdvancedSwitch: React.FC = () => {
  const { t } = useTranslation();
  const { advanced, setAdvanced, isUpdating } = useAdvanced();

  return (
    <button
      type="button"
      role="switch"
      aria-checked={advanced}
      disabled={isUpdating}
      onClick={() => setAdvanced(!advanced)}
      className="flex w-full items-center justify-between gap-2 rounded-lg border-0 bg-transparent px-2 py-1.5 text-start text-xs text-mid-gray transition-colors hover:bg-mid-gray/15 hover:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary disabled:opacity-50"
    >
      <span className="truncate">
        {t("settings.about.showAllSettings.label")}
      </span>
      <span
        aria-hidden="true"
        className={`h-3 w-3 shrink-0 rounded-full border transition-colors ${
          advanced
            ? "border-logo-primary bg-logo-primary"
            : "border-mid-gray/50 bg-transparent"
        }`}
      />
    </button>
  );
};
