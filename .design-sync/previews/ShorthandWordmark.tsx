import React from "react";
import { ShorthandWordmark } from "shorthand-app";

/**
 * The wordmark is artwork, not type: both halves are the real clay lockup, and
 * `height` is the cap height of the word rather than the image height — the
 * mark and the swash scale from it. So the stories worth showing are the ones
 * that pin down that unit and the lockup's behaviour in a real header.
 *
 * There is deliberately no dark-ground story. The cream-inked variant is chosen
 * by CSS from `:root[data-theme]` / `prefers-color-scheme` (see `.sh-wordmark-*`
 * in brand/marks.css), so a dark panel inside one card would still show the
 * navy variant and misrepresent the component. Switch the whole preview to dark
 * to see it.
 */

/** `height` is cap height, so these are the word's own sizes — the image around
 *  each is taller, carrying the swash and its air. */
export const AtCapHeights = () => (
  <div className="flex flex-wrap items-end gap-10 bg-background p-8">
    {[22, 36, 64].map((height) => (
      <div key={height} className="flex flex-col items-center gap-3">
        <ShorthandWordmark height={height} />
        <span className="font-mono text-xs text-mid-gray">height={height}</span>
      </div>
    ))}
  </div>
);

/** The lockup doing its actual job on a marketing page: brand at the left of a
 *  header, navigation and the call to action balanced against it. */
export const InAPageHeader = () => (
  <div className="bg-background p-6">
    <header className="flex items-center justify-between gap-8 border-b border-mid-gray/20 pb-5">
      <ShorthandWordmark height={20} />
      <nav className="flex items-center gap-6 text-sm text-mid-gray">
        <span>How it works</span>
        <span>Privacy</span>
        <span>Changelog</span>
      </nav>
    </header>
  </div>
);

/** Centred above the opening line, which is how the lockup is used in the app's
 *  own onboarding and About panels — the stacked artwork wants vertical air
 *  rather than a baseline to sit on. */
export const AsAHeroLockup = () => (
  <div className="flex flex-col items-center bg-background px-8 py-12 text-center">
    <ShorthandWordmark height={44} />
    <p className="mt-8 max-w-md text-lg leading-relaxed text-mid-gray">
      A fleeting voice, committed to the page.
    </p>
  </div>
);
