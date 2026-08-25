import React from "react";
import ShorthandMark from "./ShorthandMark";

/**
 * Fork-only. The Shorthand wordmark, replacing upstream's `HandyTextLogo`.
 *
 * The approved artwork stacks the bird-and-pen mark above the complete product
 * name. The word remains live Fraunces rather than outlines, so it stays crisp
 * at any size, follows the theme's ink colour, and never needs re-exporting when
 * the type scale moves.
 */
/**
 * The product name is a proper noun and is deliberately not a translation key.
 * It lives in constants rather than inline in the JSX so `i18next/no-literal-string`
 * has nothing to flag and no `eslint-disable` has to be carried past it.
 */
const NAME = "Shorthand";

// The approved silhouette occupies x=8..120 and y=20..100 inside its square
// viewBox. These values let the stacked lockup size and centre the landscape
// drawing by its visible bounds rather than by the empty canvas around it.
const MARK_VIEWBOX_SIZE = 128;
const MARK_MIN_X = 8;
const MARK_MIN_Y = 20;
const MARK_DRAWN_WIDTH = 112;
const MARK_DRAWN_HEIGHT = 80;
const MARK_WIDTH_IN_CAP_HEIGHTS = 3;

interface ShorthandWordmarkProps {
  /** Cap height of the word in px. The mark and underline both scale from it. */
  height?: number;
  className?: string;
}

export const ShorthandWordmark: React.FC<ShorthandWordmarkProps> = ({
  height = 22,
  className = "",
}) => {
  const markWidth = height * MARK_WIDTH_IN_CAP_HEIGHTS;
  const markHeight = markWidth * (MARK_DRAWN_HEIGHT / MARK_DRAWN_WIDTH);
  const markCanvasSize = markWidth * (MARK_VIEWBOX_SIZE / MARK_DRAWN_WIDTH);
  const markLeft = -markCanvasSize * (MARK_MIN_X / MARK_VIEWBOX_SIZE);
  const markTop = -markCanvasSize * (MARK_MIN_Y / MARK_VIEWBOX_SIZE);

  return (
    <span
      className={`inline-flex flex-col items-center text-text ${className}`}
      // The product name remains left-to-right in every locale. Fixing its
      // direction keeps Fraunces shaping and the underline geometry stable;
      // the vertically stacked mark itself has no directional position.
      dir="ltr"
    >
      <span
        aria-hidden="true"
        className="relative block shrink-0"
        style={{
          width: `${markWidth}px`,
          height: `${markHeight}px`,
          marginBottom: `${height * 0.12}px`,
        }}
      >
        <ShorthandMark
          className="absolute max-w-none"
          size={markCanvasSize}
          style={{ left: `${markLeft}px`, top: `${markTop}px` }}
        />
      </span>
      <span
        // FONT.md's mark height, kerning, and baseline nudge describe the
        // superseded `[mark]horthand` lockup. The approved stack keeps only its
        // live-type decisions: weight 650, the display axes, and -0.015em
        // tracking.
        style={{
          fontFamily: "var(--brand-font-display)",
          fontSize: `${height}px`,
          fontVariationSettings: "var(--brand-font-display-variation-settings)",
          fontWeight: "var(--brand-font-display-weight)",
          letterSpacing: "-0.015em",
          lineHeight: 1,
          whiteSpace: "nowrap",
        }}
      >
        {NAME}
      </span>
      <span
        aria-hidden="true"
        className="block"
        style={{
          width: "86%",
          height: `${Math.max(height * 0.08, 2)}px`,
          marginTop: `${height * 0.12}px`,
          borderRadius: "9999px",
          backgroundColor: "var(--brand-highlighter)",
          transform: "rotate(-1deg)",
        }}
      />
    </span>
  );
};

export default ShorthandWordmark;
