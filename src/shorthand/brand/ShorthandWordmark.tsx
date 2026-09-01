import React from "react";
import logoLight from "./logo-light.png";
import logoDark from "./logo-dark.png";

/**
 * Fork-only. The Shorthand lockup, replacing upstream's `HandyTextLogo`.
 *
 * This renders the approved artwork as the single image it is. The pen's nib
 * flows into the S, the bird perches on the barrel, and the coral swash
 * underlines the word — one interlocked composition.
 *
 * It used to draw the mark and the word as two rasters stacked and sized
 * against each other. That could not reproduce the interlock at all: the pen
 * ended where the mark's crop ended, the S began separately below it, and the
 * result read as a bird hovering over a word rather than as the lockup. Sizing
 * two crops to meet convincingly is not a solvable problem — the connection
 * exists only in the composition, so the composition is what ships.
 *
 * The word used to be live Fraunces type before that, which was a workaround
 * for the artwork's fixed navy being invisible on the dark ground, and it cost
 * the clay: texture, bevel and swash all became flat type. Both variants are
 * now generated from the one source by `gen-brand-wordmark.mjs`, which remaps
 * only the word's ink to cream and leaves the illustration alone — see that
 * script for how, and BRANDING.md for why tracing and re-setting in a typeface
 * were both rejected.
 *
 * Both variants are rendered and CSS picks one, because the theme can come from
 * either the OS preference or an explicit `data-theme` override; only CSS sees
 * both. `.sh-wordmark-*` in marks.css mirrors upstream's own theme-selection
 * blocks to resolve that.
 */
/**
 * The product name is a proper noun and is deliberately not a translation key.
 * `alt` carries it for screen readers, since the word is artwork rather than
 * text in the DOM.
 */
const NAME = "Shorthand";

// Measured from the generated asset by gen-brand-wordmark.mjs, which prints
// both numbers. `height` is the cap height of the word, so the image scales up
// from it by these factors — the artwork carries the bird above the word and
// the swash below, which is why it is over three times taller than the word
// itself. Re-run the generator if the artwork is re-rendered; it reports the
// values to reconcile rather than letting a shifted composition scale silently.
const LOCKUP_WIDTH_IN_CAP_HEIGHTS = 4.9296;
const LOCKUP_HEIGHT_IN_CAP_HEIGHTS = 3.2535;

interface ShorthandWordmarkProps {
  /** Cap height of the word in px. The whole lockup scales from it. */
  height?: number;
  className?: string;
}

export const ShorthandWordmark: React.FC<ShorthandWordmarkProps> = ({
  height = 22,
  className = "",
}) => {
  const width = height * LOCKUP_WIDTH_IN_CAP_HEIGHTS;
  const lockupHeight = height * LOCKUP_HEIGHT_IN_CAP_HEIGHTS;

  return (
    <span
      className={`inline-flex ${className}`}
      // The product name remains left-to-right in every locale, and as artwork
      // it cannot be mirrored by the layout in the first place.
      dir="ltr"
    >
      {/* Only one of these is displayed; see `.sh-wordmark-*` in marks.css.
          The visible one carries the accessible name and the hidden one is
          removed from the tree, so the name is announced exactly once. */}
      <img
        src={logoLight}
        alt={NAME}
        width={width}
        height={lockupHeight}
        className="sh-wordmark-light block shrink-0"
      />
      <img
        src={logoDark}
        alt={NAME}
        width={width}
        height={lockupHeight}
        className="sh-wordmark-dark block shrink-0"
      />
    </span>
  );
};

export default ShorthandWordmark;
