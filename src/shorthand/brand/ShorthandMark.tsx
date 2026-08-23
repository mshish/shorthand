import React from "react";
import { MARK_PATH } from "./mark.generated";

/**
 * Fork-only. The Shorthand mark: a lowercase "s" written with a pointed pen,
 * thinning to nothing at the entry, the waist and the exit.
 *
 * The curve is drawn by `scripts/gen-brand-mark.ts`; see the header there for
 * why the shape is generated rather than hand-authored.
 *
 * Fills with `currentColor` so the same component serves as the sidebar icon
 * (inheriting the row's text colour), the wordmark's initial, and the tray
 * artwork, without a themed fill of its own.
 */
interface ShorthandMarkProps extends React.SVGProps<SVGSVGElement> {
  width?: number | string;
  height?: number | string;
  /** Convenience for the square case; sets both width and height. */
  size?: number | string;
}

const ShorthandMark: React.FC<ShorthandMarkProps> = ({
  width,
  height,
  size,
  ...props
}) => (
  <svg
    width={width ?? size ?? 24}
    height={height ?? size ?? 24}
    viewBox="0 0 64 64"
    fill="none"
    aria-hidden="true"
    focusable="false"
    xmlns="http://www.w3.org/2000/svg"
    {...props}
  >
    <path d={MARK_PATH} fill="currentColor" />
  </svg>
);

export default ShorthandMark;
