/**
 * Fork-only. Turns upstream Handy's translation catalogues into Shorthand's.
 *
 * The catalogues under `src/i18n/locales/` are kept byte-identical to
 * upstream, so `git merge upstream/main` never conflicts on them and new
 * upstream strings arrive cleanly. The rename happens here instead, applied at
 * build time by the Vite plugin in `./vite-branding-plugin.ts`.
 *
 * Editing the locale files directly was rejected: it would put ~400 changed
 * lines into the 24 files upstream churns most, and — worse — every future
 * upstream string containing "Handy" would arrive unrenamed with nothing to
 * catch it. `scripts/check-branding.ts` is the guard that makes this
 * approach safe rather than merely cheap.
 *
 * Order matters: substitution runs first, then fork-only strings are merged on
 * top. That is why the strings below can say "Handy" and mean it — they are
 * never passed through the substitution.
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import englishCopy from "./english-copy.json";

const BRAND_FROM = "Handy";
const BRAND_TO = "Shorthand";

/**
 * Strings that exist only in this fork, in `./locales/*.json` so they can be
 * translated. Contributors add a language exactly as they would upstream —
 * see `./locales/README.md`.
 *
 * Separate from `english-copy.json` on purpose. An audit against upstream's
 * catalogue found 43 of the original 81 entries here differed from upstream
 * only in English capitalisation, and because this merge is locale-independent
 * each one replaced 23 real translations with an English string. Those live in
 * `english-copy.json` now and reach `en` alone. A second audit found 24 more
 * fork-only keys that had bypassed this file entirely, written straight into
 * the locale files instead (`docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md`,
 * Task 2) — those are folded in here too.
 *
 * Flat dotted keys; `setByPath` expands them. i18next accepts either shape.
 */
const LOCALES_DIR = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "locales",
);

/**
 * Every fork catalogue on disk, keyed by locale.
 *
 * Read at module load rather than via `import.meta.glob`, because this module
 * runs in two environments: Vite (build plugin) and plain Bun
 * (`scripts/check-branding.ts`), and `import.meta.glob` is a Vite transform
 * that does not exist in the second.
 *
 * This makes the module depend on `node:fs`. That is fine for both current
 * consumers — neither is browser code — but it does mean this file must never
 * be imported into the app bundle itself.
 */
const FORK_CATALOGUES: Record<
  string,
  Record<string, string>
> = Object.fromEntries(
  fs
    .readdirSync(LOCALES_DIR)
    .filter((file) => file.endsWith(".json"))
    .map((file) => [
      file.slice(0, -".json".length),
      JSON.parse(fs.readFileSync(path.join(LOCALES_DIR, file), "utf8")),
    ]),
);

/**
 * The fork's strings for one locale: English as the base, that locale's own
 * catalogue layered on top.
 *
 * English is the base rather than a lookup-time fallback so every locale
 * receives a complete key set. A partially-translated catalogue then renders
 * its translated keys and leaves the rest in English, instead of rendering a
 * raw key path where a string should be.
 */
export function forkStringsFor(locale: string): Record<string, string> {
  const base = FORK_CATALOGUES["en"] ?? {};
  const override = FORK_CATALOGUES[locale];
  return override ? { ...base, ...override } : { ...base };
}

/**
 * English copy preferences: upstream labels settings in Title Case, the fork's
 * copy rule is sentence case. Applied all-or-nothing — an earlier pass
 * converted three labels and left the rest, which moved the inconsistency from
 * between-tabs to *within* one screen, and half-converted reads as a bug in a
 * way uniformly Title Case did not. So every Title Case label the settings
 * tree renders is overridden here.
 *
 * English only, and that is now enforced rather than incidental: sentence case
 * is an English typographic convention, and German capitalises every noun.
 * A locale gets its own translation, not this.
 *
 * Acronyms and proper nouns keep their capitals: API, URL, ONNX, English,
 * Handy, Beta, What's New.
 */
const ENGLISH_COPY: Record<string, string> = englishCopy;

/**
 * The union, for consumers asking the locale-independent question "is this key
 * deliberately ours?" — `scripts/check-branding.ts` uses it that way.
 *
 * Policy notes for individual entries that used to sit beside them:
 * - `settings.about.showAllSettings.*` say "Handy" on purpose: they name the
 *   upstream project. The merge order is what protects them from substitution.
 * - The shortcut rows end in `.name`, not `.label`/`.title`, which is why an
 *   early sweep missed them.
 * - `settings.modes.tabs.meetings` renames upstream's "Transcription" because
 *   transcription is what both modes do; "Meetings" names what the mode is
 *   for. User-facing only — the `transcribe` binding ids and the Rust fields
 *   keep their names.
 */
export const FORK_ONLY_STRINGS: Record<string, string> = {
  ...forkStringsFor("en"),
  ...ENGLISH_COPY,
};

/**
 * Dotted key paths in the UPSTREAM catalogues whose value must keep saying
 * "Handy" — for example a string that credits the upstream project by name.
 *
 * Empty today. The mechanism exists because the alternative is discovering the
 * need for it by shipping a wrong string, and because `check-branding` asserts
 * that every key listed here still contains the word, which catches upstream
 * rewording a sentence out from under a stale entry.
 */
export const BRAND_EXEMPT_KEYS: ReadonlySet<string> = new Set<string>([]);

export interface BrandingWarning {
  locale: string;
  key: string;
  value: string;
  reason: string;
}

export interface BrandingResult {
  translation: Record<string, unknown>;
  warnings: BrandingWarning[];
}

/**
 * Matches the product name as a standalone word, optionally carrying a
 * Scandinavian/German genitive `s`. The genitive matters: Danish writes
 * "Handys lokale tale-til-tekst", and a bare `\bHandy\b` leaves that as
 * "Handys" while rebranding everything around it.
 *
 * Compounds like "Handy-Symbol" are already covered, because a hyphen is a
 * word boundary.
 */
const WORD_BOUNDED = new RegExp(`\\b${BRAND_FROM}(s?)\\b`, "g");
// Deliberately not global: `.test()` on a global regex advances `lastIndex`
// between calls, so it would alternate true/false across strings.
const ANY_OCCURRENCE = new RegExp(BRAND_FROM);

/**
 * German uses "Handy" as the everyday word for a mobile phone, so a
 * substitution there could in principle corrupt real prose rather than rename
 * the product. No current string hits that case — every German match is a
 * product reference — but the risk is specific to German and worth surfacing
 * when the text changes.
 *
 * Restricted to `de` deliberately: warning on every language's compounds
 * produced twelve false alarms and trained the reader to skim past them.
 */
const FALSE_FRIEND_LOCALES: ReadonlySet<string> = new Set(["de"]);

function setByPath(
  target: Record<string, unknown>,
  path: string,
  value: string,
): void {
  const parts = path.split(".");
  let cursor = target;
  for (const part of parts.slice(0, -1)) {
    const next = cursor[part];
    if (typeof next !== "object" || next === null) {
      cursor[part] = {};
    }
    cursor = cursor[part] as Record<string, unknown>;
  }
  cursor[parts[parts.length - 1]] = value;
}

/**
 * Rebrand one locale's catalogue. Pure: the input is never mutated, so the
 * same function backs both the build plugin and the guard script.
 */
export function applyBranding(
  translation: Record<string, unknown>,
  locale: string,
): BrandingResult {
  const warnings: BrandingWarning[] = [];

  const walk = (node: unknown, path: string): unknown => {
    if (typeof node === "string") {
      if (BRAND_EXEMPT_KEYS.has(path)) return node;

      const substituted = node.replace(WORD_BOUNDED, `${BRAND_TO}$1`);

      if (substituted !== node && FALSE_FRIEND_LOCALES.has(locale)) {
        warnings.push({
          locale,
          key: path,
          value: node,
          reason: `rebranded in German, where "${BRAND_FROM}" is also the everyday word for a mobile phone — confirm this names the product`,
        });
      }

      if (ANY_OCCURRENCE.test(substituted)) {
        warnings.push({
          locale,
          key: path,
          value: substituted,
          reason: `"${BRAND_FROM}" survives after substitution, glued to surrounding characters — needs a human`,
        });
      }

      return substituted;
    }

    if (Array.isArray(node)) {
      return node.map((item, index) => walk(item, `${path}[${index}]`));
    }

    if (typeof node === "object" && node !== null) {
      const out: Record<string, unknown> = {};
      for (const [key, value] of Object.entries(node)) {
        out[key] = walk(value, path ? `${path}.${key}` : key);
      }
      return out;
    }

    return node;
  };

  const rebranded = walk(translation, "") as Record<string, unknown>;

  // Fork strings are content and belong in every locale; English copy rules
  // are an English typographic convention (or, in one case, a wording choice
  // not yet ported to other locales) and belong only in English. Merging
  // the latter everywhere is what replaced 23 locales' translations with
  // English strings.
  const forkStrings = forkStringsFor(locale);
  const overlay =
    locale === "en" ? { ...forkStrings, ...ENGLISH_COPY } : forkStrings;

  for (const [path, value] of Object.entries(overlay)) {
    setByPath(rebranded, path, value);
  }

  return { translation: rebranded, warnings };
}
