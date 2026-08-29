import React from "react";
import { MARK_PATHS } from "./mark.paths";

/**
 * Fork-only. The Shorthand mark: a bird perched on a fountain pen, reduced to
 * the approved one-colour silhouette so the same story survives when the clay
 * artwork's material detail disappears at small sizes.
 *
 * Fills with `currentColor` so the same component serves as the sidebar icon
 * (inheriting the row's text colour), the wordmark, and the tray artwork,
 * without a themed fill of its own.
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
    viewBox="0 0 128 128"
    fill="currentColor"
    shapeRendering="geometricPrecision"
    aria-hidden="true"
    focusable="false"
    xmlns="http://www.w3.org/2000/svg"
    {...props}
  >
    {MARK_PATHS.map((path) => (
      <path
        key={path.d}
        d={path.d}
        fill="currentColor"
        fillRule={path.fillRule}
      />
    ))}
  </svg>
);

export default ShorthandMark;
