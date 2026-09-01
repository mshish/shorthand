import React from "react";
import { Button } from "shorthand-app";

/**
 * Button carries the two colour roles that most define the fork: `--color-
 * background-ui` at full strength for the one primary action, and the quiet
 * `logo-primary/20` tint for the softer one. The variant sweep is the story
 * that matters — the axis is semantic, not decorative, and picking the wrong
 * one is the mistake worth making visible.
 */

const VARIANTS = [
  "primary",
  "primary-soft",
  "secondary",
  "ghost",
  "warning",
  "danger",
  "danger-ghost",
] as const;

/** Every variant, labelled. `danger` and `warning` are status, not emphasis —
 *  `warning` deliberately borrows the semantic amber token rather than the
 *  brand accent, so it can sit on a warning surface without competing. */
export const AllVariants = () => (
  <div className="grid grid-cols-4 gap-x-4 gap-y-6 bg-background p-8">
    {VARIANTS.map((variant) => (
      <div key={variant} className="flex flex-col items-center gap-2">
        <Button variant={variant}>Transcribe</Button>
        <span className="font-mono text-xs text-mid-gray">{variant}</span>
      </div>
    ))}
  </div>
);

/** The three sizes against a single variant, so the only thing changing is the
 *  padding and type scale. */
export const Sizes = () => (
  <div className="flex flex-wrap items-center gap-4 bg-background p-8">
    {(["sm", "md", "lg"] as const).map((size) => (
      <div key={size} className="flex flex-col items-center gap-2">
        <Button size={size}>Start dictating</Button>
        <span className="font-mono text-xs text-mid-gray">{size}</span>
      </div>
    ))}
  </div>
);

/** Disabled drops to 50% and blocks the cursor; it is a single opacity rule, so
 *  it reads the same across every variant. */
export const Disabled = () => (
  <div className="flex flex-wrap items-center gap-4 bg-background p-8">
    <Button disabled>Primary</Button>
    <Button variant="secondary" disabled>
      Secondary
    </Button>
    <Button variant="ghost" disabled>
      Ghost
    </Button>
  </div>
);

/** The landing-page pairing: one primary action, one quiet companion. Two
 *  primaries side by side is the failure mode this story exists to rule out. */
export const CallToActionPair = () => (
  <div className="bg-background px-8 py-12">
    <h2 className="max-w-lg font-display text-5xl leading-tight text-text">
      Your voice, written down.
    </h2>
    <p className="mt-4 max-w-md text-lg leading-relaxed text-mid-gray">
      Runs entirely on your machine. No account, no upload, no transcript
      leaving the laptop.
    </p>
    <div className="mt-8 flex flex-wrap items-center gap-3">
      <Button size="lg">Download for macOS</Button>
      <Button size="lg" variant="ghost">
        See how it works
      </Button>
    </div>
  </div>
);
