import React from "react";
import { useTranslation } from "react-i18next";
import { useAdvanced } from "../useAdvanced";
import { ADVANCED_ANCHOR_ATTR } from "./AdvancedOnly";

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
/**
 * Scroll the first newly-revealed row into view.
 *
 * Not a flourish — a correctness fix, and one only measuring caught. At the
 * app's real window size (680x570, from `lib.rs`) the content pane is 532px
 * tall, the default Modes section is 539px, and the first row the switch
 * reveals starts at y=539. Seven pixels below the fold. Clicking the switch
 * changed nothing visible on screen except a 12px dot in the sidebar footer, so
 * the only feedback that it had worked was to guess and scroll.
 *
 * A control whose entire job is to reveal something has to show that it
 * revealed something.
 */
function revealFirstAdvancedRow() {
  // Two frames: one for React to commit the newly-mounted rows, one for layout
  // to settle before measuring.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      const anchor = document.querySelector(`[${ADVANCED_ANCHOR_ATTR}]`);
      // `display: contents` has no box of its own, so scroll to the first row
      // inside it.
      const target = anchor?.firstElementChild;
      if (!(target instanceof HTMLElement)) return;

      const reduced = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;
      target.scrollIntoView({
        block: "center",
        behavior: reduced ? "auto" : "smooth",
      });
    });
  });
}

export const AdvancedSwitch: React.FC = () => {
  const { t } = useTranslation();
  const { advanced, setAdvanced, isUpdating } = useAdvanced();

  const onToggle = () => {
    const next = !advanced;
    setAdvanced(next);
    if (next) revealFirstAdvancedRow();
  };

  return (
    <button
      type="button"
      role="switch"
      aria-checked={advanced}
      disabled={isUpdating}
      onClick={onToggle}
      className="flex w-full items-center justify-between gap-2 rounded-lg border-0 bg-transparent px-2 py-1.5 text-start text-xs text-mid-gray transition-colors hover:bg-mid-gray/15 hover:text-text focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-logo-primary disabled:opacity-50"
    >
      {/* `defaultValue` because this key exists only via FORK_ONLY_STRINGS and
          never in en/translation.json, so i18next's fallback for it is the raw
          key. That is exactly what shipped to the screen once: the footer read
          "settings.advanced....". check:settings now catches a missing key at
          build time; this makes the runtime failure mode readable English
          rather than a dotted path. */}
      <span className="truncate">
        {t("settings.advanced.switch.label", {
          defaultValue: "Advanced settings",
        })}
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
