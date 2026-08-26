import React from "react";
import markColour from "../../../brand-assets/mark-full-colour-transparent.png";
import wordmarkLight from "./wordmark-light.png";
import wordmarkDark from "./wordmark-dark.png";

/**
 * Fork-only. The Shorthand wordmark, replacing upstream's `HandyTextLogo`.
 *
 * The approved artwork stacks the bird-and-pen mark above the product name and
 * its coral swash, and this renders exactly that: both halves are the real clay
 * artwork, not a reconstruction.
 *
 * The word used to be live Fraunces type. That was a workaround for the
 * artwork's fixed navy being invisible on the dark ground, and it cost the
 * clay: texture, bevel and swash all became flat type. The artwork is now used
 * directly, with `gen-brand-wordmark.mjs` deriving a cream-inked dark variant
 * that keeps the clay intact — see that script for how, and BRANDING.md for why
 * tracing and re-setting in a typeface were both rejected.
 *
 * Both variants are rendered and CSS picks one, because the theme can come from
 * either the OS preference or an explicit `data-theme` override; only CSS sees
 * both. `.sh-wordmark-*` in marks.css mirrors upstream's own theme-selection
 * blocks to resolve that.
 */
/**
 * The product name is a proper noun and is deliberately not a translation key.
 * `alt` carries it for screen readers, since the word is now artwork rather
 * than text in the DOM.
 */
const NAME = "Shorthand";

// mark-full-colour-transparent.png is 845x498 and tightly cropped.
const MARK_ASPECT_RATIO = 845 / 498;
// Sized so the mark keeps the proportion it had against the word in the
// approved lockup.
const MARK_HEIGHT_IN_CAP_HEIGHTS = 2.15;

// Measured from the generated asset (600x194, word occupying 138px of that).
// `height` is the cap height of the word, so the image has to be scaled up from
// it by these factors — the artwork carries the swash and its surrounding air
// below the word, which is why the image is taller than the word itself.
const WORDMARK_WIDTH_IN_CAP_HEIGHTS = 600 / 138;
const WORDMARK_HEIGHT_IN_CAP_HEIGHTS = 194 / 138;

interface ShorthandWordmarkProps {
  /** Cap height of the word in px. The mark scales from it too. */
  height?: number;
  className?: string;
}

export const ShorthandWordmark: React.FC<ShorthandWordmarkProps> = ({
  height = 22,
  className = "",
}) => {
  const markHeight = height * MARK_HEIGHT_IN_CAP_HEIGHTS;
  const markWidth = markHeight * MARK_ASPECT_RATIO;
  const wordWidth = height * WORDMARK_WIDTH_IN_CAP_HEIGHTS;
  const wordHeight = height * WORDMARK_HEIGHT_IN_CAP_HEIGHTS;

  return (
    <span
      className={`inline-flex flex-col items-center ${className}`}
      // The product name remains left-to-right in every locale, and as artwork
      // it cannot be mirrored by the layout in the first place.
      dir="ltr"
    >
      <img
        src={markColour}
        alt=""
        aria-hidden="true"
        width={markWidth}
        height={markHeight}
        className="block shrink-0"
        style={{ marginBottom: `${height * 0.08}px` }}
      />
      {/* Only one of these is displayed; see `.sh-wordmark-*` in marks.css.
          The visible one carries the accessible name and the hidden one is
          removed from the tree, so the name is announced exactly once. */}
      <img
        src={wordmarkLight}
        alt={NAME}
        width={wordWidth}
        height={wordHeight}
        className="sh-wordmark-light block shrink-0"
      />
      <img
        src={wordmarkDark}
        alt={NAME}
        width={wordWidth}
        height={wordHeight}
        className="sh-wordmark-dark block shrink-0"
      />
    </span>
  );
};

export default ShorthandWordmark;
