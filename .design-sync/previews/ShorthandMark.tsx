import React from "react";
import { ShorthandMark } from "shorthand-app";

/**
 * The mark exists as a one-colour silhouette so the clay artwork's story
 * survives when its material detail cannot — so the stories that matter are the
 * ones that test exactly that: does it still read small, and does it take the
 * colour of whatever it is sitting on.
 */

/** The sizes the mark is actually used at, smallest first. 16px is the tray
 *  icon and the hardest case; if the bird and the pen are still separable
 *  there, the reduction did its job. */
export const AtUsedSizes = () => (
  <div className="flex items-end gap-8 p-8 text-logo-primary">
    {[16, 24, 48, 96].map((size) => (
      <div key={size} className="flex flex-col items-center gap-2">
        <ShorthandMark size={size} />
        <span className="font-mono text-xs text-mid-gray">{size}</span>
      </div>
    ))}
  </div>
);

/** The mark fills with `currentColor` and has no themed fill of its own, which
 *  is what lets one component serve the sidebar row, the wordmark and the tray.
 *  Each ground below sets a different text colour; the mark follows. */
export const TakesItsGroundsColour = () => (
  <div className="flex flex-wrap gap-4 p-8">
    {[
      { label: "on paper", cls: "bg-background text-logo-primary" },
      { label: "on ink", cls: "bg-background-ui text-white" },
      { label: "on coral", cls: "bg-highlighter text-highlighter-ink" },
      { label: "in body text", cls: "bg-background text-text" },
    ].map(({ label, cls }) => (
      <div
        key={label}
        className={`flex flex-col items-center gap-3 rounded-lg border border-mid-gray/20 px-6 py-5 ${cls}`}
      >
        <ShorthandMark size={40} />
        <span className="text-xs opacity-80">{label}</span>
      </div>
    ))}
  </div>
);

/** Hero scale. The mark is the only piece of the identity that survives being
 *  blown up without the wordmark's raster softening, so this is the one that
 *  carries a landing page's opening section. */
export const AtHeroScale = () => (
  <div className="flex items-center gap-8 bg-background p-10">
    <ShorthandMark size={128} className="shrink-0 text-logo-primary" />
    <div className="max-w-sm">
      <h2 className="font-display text-4xl leading-tight text-text">
        Say it once.
      </h2>
      <p className="mt-3 text-base leading-relaxed text-mid-gray">
        Shorthand listens while you talk and hands back the written thing —
        offline, on your own machine.
      </p>
    </div>
  </div>
);
