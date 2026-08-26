/**
 * Fork-only. Regenerates the golden hashes in
 * `src/shorthand/branding.golden.json`.
 *
 * Run this ONLY when rendered output has deliberately changed, and review the
 * resulting diff. Running it to make a failing test pass defeats the point:
 * the test exists to prove a refactor changed nothing.
 *
 * Run: bun scripts/write-branding-golden.ts
 */

import fs from "fs";
import path from "path";
import crypto from "crypto";
import { fileURLToPath } from "url";
import { applyBranding } from "../src/shorthand/branding";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const GOLDEN = path.join(
  __dirname,
  "..",
  "src",
  "shorthand",
  "branding.golden.json",
);

/**
 * Deterministic serialisation, sorting keys at every depth.
 *
 * NOT `JSON.stringify(value, Object.keys(value).sort())`: the replacer-array
 * form filters by key name at *every* level, so a list of top-level names
 * silently drops every nested key and the hash would cover almost nothing.
 */
function stableStringify(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value as Record<string, unknown>)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableStringify(item)}`);
    return `{${entries.join(",")}}`;
  }
  return JSON.stringify(value) ?? "null";
}

export function hashLocale(locale: string): string {
  const file = path.join(LOCALES_DIR, locale, "translation.json");
  const raw = JSON.parse(fs.readFileSync(file, "utf8"));
  const { translation } = applyBranding(raw, locale);
  return crypto
    .createHash("sha256")
    .update(stableStringify(translation))
    .digest("hex");
}

export function localeNames(): string[] {
  return fs
    .readdirSync(LOCALES_DIR, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
}

if (import.meta.main) {
  const golden: Record<string, string> = {};
  for (const locale of localeNames()) golden[locale] = hashLocale(locale);
  fs.writeFileSync(GOLDEN, JSON.stringify(golden, null, 2) + "\n");
  console.log(`Wrote ${Object.keys(golden).length} golden hashes.`);
}
