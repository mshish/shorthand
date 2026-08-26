/**
 * Fork-only. Key parity across `src/shorthand/locales/*.json`.
 *
 * `check-translations.ts` covers upstream's catalogues under
 * `src/i18n/locales/`. It structurally cannot see these: fork strings
 * deliberately never enter those files (and `check-locale-drift.ts`, from an
 * earlier task in this plan, is what keeps that true), so upstream merges
 * never conflict on them. This is their equivalent gate.
 *
 * Reads raw files rather than calling `forkStringsFor`, which merges English
 * in as a base and would therefore report every catalogue as complete.
 *
 * Run: bun scripts/check-fork-translations.ts
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES = path.join(__dirname, "..", "src", "shorthand", "locales");
const REFERENCE = "en";

const colors: Record<string, string> = {
  reset: "\x1b[0m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
};
const colorize = (text: string, color: string): string =>
  `${colors[color]}${text}${colors.reset}`;

const load = (locale: string): Record<string, string> =>
  JSON.parse(fs.readFileSync(path.join(LOCALES, `${locale}.json`), "utf8"));

const locales = fs
  .readdirSync(LOCALES)
  .filter((file) => file.endsWith(".json"))
  .map((file) => file.slice(0, -".json".length))
  .filter((locale) => locale !== REFERENCE)
  .sort();

const referenceKeys = new Set(Object.keys(load(REFERENCE)));
let failed = false;

for (const locale of locales) {
  const keys = new Set(Object.keys(load(locale)));
  const missing = [...referenceKeys].filter((key) => !keys.has(key));
  const extra = [...keys].filter((key) => !referenceKeys.has(key));

  if (missing.length === 0 && extra.length === 0) {
    console.log(
      colorize(`✓ ${locale}: all ${referenceKeys.size} keys`, "green"),
    );
    continue;
  }

  failed = true;
  console.log(colorize(`✗ ${locale}:`, "red"));
  if (missing.length > 0) {
    console.log(colorize(`  missing ${missing.length}:`, "yellow"));
    for (const key of missing.slice(0, 10)) console.log(`    - ${key}`);
    if (missing.length > 10)
      console.log(`    ... and ${missing.length - 10} more`);
  }
  if (extra.length > 0) {
    console.log(colorize(`  not in ${REFERENCE} (${extra.length}):`, "yellow"));
    for (const key of extra.slice(0, 10)) console.log(`    - ${key}`);
    if (extra.length > 10) console.log(`    ... and ${extra.length - 10} more`);
  }
}

if (failed) {
  console.log(
    colorize(
      `\nA missing key falls back to English silently, which is why this fails\n` +
        `the build rather than warning.`,
      "yellow",
    ),
  );
  process.exit(1);
}

console.log(
  colorize(
    `\n✓ ${locales.length} fork translation(s) match ${REFERENCE} (${referenceKeys.size} keys).`,
    "green",
  ),
);
process.exit(0);
