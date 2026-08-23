import React from "react";
import ShorthandMark from "./ShorthandMark";

/**
 * Fork-only. The Shorthand wordmark, replacing upstream's `HandyTextLogo`.
 *
 * The mark stands in for the initial S rather than sitting beside the word.
 * A logo bug next to "Shorthand" would put two S's in the lockup and say
 * nothing; substituting the written stroke for the typeset letter is the whole
 * idea of the product in one move — the word is half handwriting.
 *
 * Set in the app's own type rather than as outlines, so the wordmark stays
 * crisp at any size, follows the theme's ink colour, and never needs
 * re-exporting when the type scale moves.
 */
/**
 * The product name is a proper noun and is deliberately not a translation key.
 * It lives in constants rather than inline in the JSX so `i18next/no-literal-string`
 * has nothing to flag and no `eslint-disable` has to be carried past it.
 *
 * `NAME` is the accessible name; `NAME_TAIL` is what remains once the mark has
 * taken the initial S.
 */
const NAME = "Shorthand";
const NAME_TAIL = NAME.slice(1);

interface ShorthandWordmarkProps {
  /** Cap height of the lockup in px. The mark and the word both scale from it. */
  height?: number;
  className?: string;
}

export const ShorthandWordmark: React.FC<ShorthandWordmarkProps> = ({
  height = 22,
  className = "",
}) => (
  <span
    className={`inline-flex items-baseline text-text ${className}`}
    // `dir="ltr"` because the lockup is a name, not running text: in an RTL
    // locale the mark must stay on the left of "horthand" or it stops spelling
    // anything.
    dir="ltr"
  >
    <span className="sr-only">{NAME}</span>
    <ShorthandMark
      aria-hidden="true"
      height={height}
      width={height}
      // The generated glyph is padded inside its 64-unit box and carries a
      // swash below the baseline, so it needs pulling down and tightening in
      // to sit as an initial rather than as a bug parked next to the word.
      style={{
        marginInlineEnd: `${-height * 0.1}px`,
        transform: `translateY(${height * 0.11}px)`,
      }}
    />
    <span
      aria-hidden="true"
      className="font-medium"
      style={{
        fontSize: `${height * 0.82}px`,
        letterSpacing: "-0.015em",
        lineHeight: 1,
      }}
    >
      {NAME_TAIL}
    </span>
  </span>
);

export default ShorthandWordmark;
