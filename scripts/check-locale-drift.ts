/**
 * Fork-only. Fails if `src/i18n/locales/` has drifted from `upstream/main`.
 *
 * Those catalogues are supposed to stay byte-identical to upstream so
 * `git merge upstream/main` never conflicts on them — fork content belongs in
 * `src/shorthand/locales/` and `src/shorthand/english-copy.json` instead. This
 * is the gate that makes "supposed to" a fact rather than a hope.
 *
 * Written after finding it had already failed silently: a feature branch
 * added 1568 lines of English-only fork content (dictation mode, system audio
 * capture) directly into all 24 locale files, and nothing caught it —
 * `check:translations` only compares key parity *between* locales, so 24
 * files agreeing with each other looked fine.
 *
 * Deliberately checks key PRESENCE only, not value equality on keys that
 * exist in both fork and upstream. A translator improving an existing
 * translation's wording directly in these files is normal and welcome
 * (`CONTRIBUTING_TRANSLATIONS.md`); this script must not treat that as the
 * same defect as fork content that was never translated at all. See
 * check-locale-value-drift below for the separate, non-blocking check that
 * surfaces those for a human instead.
 *
 * Run: bun scripts/check-locale-drift.ts
 * Fix things automatically: bun scripts/check-locale-drift.ts --fix
 *   (deletes any key absent upstream from every locale file; it does NOT add
 *   it anywhere else — move it into src/shorthand/locales/en.json or
 *   english-copy.json yourself, by hand, so the classification is a decision
 *   and not a script's guess)
 */

import fs from "fs";
import path from "path";
import { execFileSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const FIX = process.argv.includes("--fix");

function flatten(node: unknown, prefix: string, out: Map<string, unknown>): void {
  if (Array.isArray(node)) {
    node.forEach((item, i) => flatten(item, `${prefix}[${i}]`, out));
    return;
  }
  if (typeof node === "object" && node !== null) {
    for (const [key, value] of Object.entries(node)) {
      flatten(value, prefix ? `${prefix}.${key}` : key, out);
    }
    return;
  }
  out.set(prefix, node);
}

function readUpstream(locale: string): Record<string, unknown> | null {
  try {
    const raw = execFileSync(
      "git",
      ["show", `upstream/main:src/i18n/locales/${locale}/translation.json`],
      { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
    );
    return JSON.parse(raw);
  } catch {
    return null; // locale does not exist upstream
  }
}

/** Deletes one dotted path, then prunes any ancestor object left empty. */
function deleteKey(root: Record<string, unknown>, dotted: string): void {
  const parts = dotted.split(".");
  const stack: [Record<string, unknown>, string][] = [];
  let cursor: Record<string, unknown> = root;
  for (const part of parts.slice(0, -1)) {
    if (typeof cursor !== "object" || cursor === null) return;
    stack.push([cursor, part]);
    cursor = cursor[part] as Record<string, unknown>;
  }
  if (typeof cursor === "object" && cursor !== null) {
    delete cursor[parts[parts.length - 1]];
  }
  for (let i = stack.length - 1; i >= 0; i--) {
    const [obj, key] = stack[i];
    const child = obj[key] as Record<string, unknown>;
    if (
      typeof child === "object" &&
      child !== null &&
      !Array.isArray(child) &&
      Object.keys(child).length === 0
    ) {
      delete obj[key];
    }
  }
}

const locales = fs
  .readdirSync(LOCALES_DIR, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

let failed = false;
const report: string[] = [];

for (const locale of locales) {
  const file = path.join(LOCALES_DIR, locale, "translation.json");
  const local = JSON.parse(fs.readFileSync(file, "utf8"));
  const upstream = readUpstream(locale);
  if (upstream === null) {
    failed = true;
    report.push(`${locale}: exists in the fork but not in upstream/main`);
    continue;
  }

  const localFlat = new Map<string, unknown>();
  flatten(local, "", localFlat);
  const upstreamFlat = new Map<string, unknown>();
  flatten(upstream, "", upstreamFlat);

  const drifted = [...localFlat.keys()].filter((k) => !upstreamFlat.has(k));
  if (drifted.length > 0) {
    failed = true;
    report.push(`${locale}: ${drifted.length} key(s) absent upstream`);
    for (const key of drifted) report.push(`    ${key}`);
    if (FIX) {
      for (const key of drifted) deleteKey(local, key);
      fs.writeFileSync(file, JSON.stringify(local, null, 2) + "\n");
    }
  }
}

if (report.length > 0) console.log(report.join("\n"));

if (failed && !FIX) {
  console.log(
    "\nEvery key above must move to src/shorthand/locales/en.json (or\n" +
      "english-copy.json, if it's an English-only casing edit of a real upstream\n" +
      "string) before removal, or the fork loses it entirely. This script only\n" +
      "removes, it does not classify. Then re-run with --fix.",
  );
  process.exit(1);
}

if (failed && FIX) {
  console.log("\nRemoved. Re-run without --fix to confirm a clean tree.");
  process.exit(0);
}

console.log(`✓ ${locales.length} locale file(s) match upstream/main exactly.`);
process.exit(0);
