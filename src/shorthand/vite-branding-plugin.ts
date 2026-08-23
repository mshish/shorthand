/**
 * Fork-only Vite plugin. Rebrands upstream's translation catalogues on the way
 * into the bundle, so the files on disk stay byte-identical to upstream and
 * merges from `cjpais/Handy` never conflict on them.
 *
 * `enforce: "pre"` matters: Vite's own JSON plugin rewrites `.json` into an ES
 * module, and this must see the raw text before that happens.
 */

import type { Plugin } from "vite";
import { applyBranding, type BrandingWarning } from "./branding";

const LOCALE_FILE =
  /[\\/]src[\\/]i18n[\\/]locales[\\/]([^\\/]+)[\\/]translation\.json$/;

export function shorthandBranding(): Plugin {
  const seen: BrandingWarning[] = [];

  return {
    name: "shorthand-branding",
    enforce: "pre",

    transform(code, id) {
      const match = id.match(LOCALE_FILE);
      if (!match) return null;

      const locale = match[1];
      const { translation, warnings } = applyBranding(JSON.parse(code), locale);
      seen.push(...warnings);

      // No source map: this is a JSON value transform, and the positions of a
      // data file carry nothing a debugger could use.
      return { code: JSON.stringify(translation), map: null };
    },

    buildEnd() {
      if (seen.length === 0) return;

      // Warn rather than fail. A build that dies because a translator used the
      // German word for "mobile phone" would be worse than one that tells you
      // about it; `scripts/check-branding.ts` is where enforcement belongs.
      this.warn(
        `shorthand-branding: ${seen.length} string(s) need a human look:\n` +
          seen
            .map((w) => `  [${w.locale}] ${w.key}: ${w.reason}\n    ${w.value}`)
            .join("\n"),
      );
    },
  };
}
