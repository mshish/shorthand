/**
 * Fork-only. The guard that makes the build-time rebrand safe.
 *
 * The locale catalogues under `src/i18n/locales/` stay byte-identical to
 * upstream and are rebranded on the way into the bundle. That keeps merges
 * clean, but it means a new upstream string containing "Handy" arrives with
 * nothing to notice it. This script is that notice.
 *
 * Forgejo — the closest comparable fork, which rebranded Gitea while
 * continuing to merge from it — never built this and relies on people
 * spotting the wrong name in the UI. That is the failure mode being avoided
 * here.
 *
 * Run: bun scripts/check-branding.ts
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import {
  applyBranding,
  BRAND_EXEMPT_KEYS,
  FORK_ONLY_STRINGS,
} from "../src/shorthand/branding";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const LOCALES_DIR = path.join(__dirname, "..", "src", "i18n", "locales");
const SRC_TAURI = path.join(__dirname, "..", "src-tauri", "src");

const colors: Record<string, string> = {
  reset: "\x1b[0m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
};

const colorize = (text: string, color: string): string =>
  `${colors[color]}${text}${colors.reset}`;

/**
 * Brand literals that live outside i18n and so cannot be rebranded by the
 * build plugin. They were renamed by hand once; this pins them so that an
 * upstream edit to one of these lines surfaces in review instead of quietly
 * restoring "Handy" to the UI.
 */
const RUST_BRAND_SITES: ReadonlyArray<{ file: string; needle: string }> = [
  { file: "lib.rs", needle: '.title("Shorthand")' },
  { file: "cli.rs", needle: 'name = "shorthand"' },
  { file: "llm_client.rs", needle: 'HeaderValue::from_static("Shorthand")' },
  { file: "follow_stream/name.rs", needle: '"shorthand.follow-stream.' },
];

type Failure = { where: string; detail: string };

const failures: Failure[] = [];
const warnings: string[] = [];

function flatten(
  node: unknown,
  prefix: string,
  out: Map<string, string>,
): void {
  if (typeof node === "string") {
    out.set(prefix, node);
    return;
  }
  if (Array.isArray(node)) {
    node.forEach((item, i) => flatten(item, `${prefix}[${i}]`, out));
    return;
  }
  if (typeof node === "object" && node !== null) {
    for (const [key, value] of Object.entries(node)) {
      flatten(value, prefix ? `${prefix}.${key}` : key, out);
    }
  }
}

const locales = fs
  .readdirSync(LOCALES_DIR, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();

for (const locale of locales) {
  const file = path.join(LOCALES_DIR, locale, "translation.json");
  const raw = JSON.parse(fs.readFileSync(file, "utf8"));
  const { translation, warnings: localeWarnings } = applyBranding(raw, locale);

  for (const w of localeWarnings) {
    warnings.push(`[${w.locale}] ${w.key}: ${w.reason}\n    ${w.value}`);
  }

  const rendered = new Map<string, string>();
  flatten(translation, "", rendered);

  // 1. Nothing the user sees should still say "Handy" unless we said so.
  for (const [key, value] of rendered) {
    if (!/\bHandy\b/.test(value)) continue;
    const deliberate =
      BRAND_EXEMPT_KEYS.has(key) ||
      Object.prototype.hasOwnProperty.call(FORK_ONLY_STRINGS, key);
    if (!deliberate) {
      failures.push({
        where: `${locale}:${key}`,
        detail: `renders as "Handy" but is not exempt — a new upstream string?\n    ${value}`,
      });
    }
  }

  // 2. An exemption that no longer contains the word is stale: upstream
  //    reworded the sentence out from under it, and it is now silently
  //    protecting nothing.
  for (const key of BRAND_EXEMPT_KEYS) {
    const value = rendered.get(key);
    if (value === undefined) {
      failures.push({
        where: `${locale}:${key}`,
        detail:
          "exempt key no longer exists — remove it from BRAND_EXEMPT_KEYS",
      });
    } else if (!/\bHandy\b/.test(value)) {
      failures.push({
        where: `${locale}:${key}`,
        detail: `exempt key no longer says "Handy" — the exemption is stale`,
      });
    }
  }
}

// 3. Our own fork-only strings must survive into every locale.
{
  const file = path.join(LOCALES_DIR, "en", "translation.json");
  const raw = JSON.parse(fs.readFileSync(file, "utf8"));
  const { translation } = applyBranding(raw, "en");
  const rendered = new Map<string, string>();
  flatten(translation, "", rendered);
  for (const [key, expected] of Object.entries(FORK_ONLY_STRINGS)) {
    if (rendered.get(key) !== expected) {
      failures.push({
        where: `en:${key}`,
        detail: `fork-only string did not survive the merge (got ${JSON.stringify(rendered.get(key))})`,
      });
    }
  }
}

// 4. The out-of-band Rust literals.
for (const site of RUST_BRAND_SITES) {
  const file = path.join(SRC_TAURI, site.file);
  const contents = fs.readFileSync(file, "utf8");
  if (!contents.includes(site.needle)) {
    failures.push({
      where: `src-tauri/src/${site.file}`,
      detail: `expected brand literal is gone: ${site.needle}\n    An upstream merge may have reverted it.`,
    });
  }
}

if (warnings.length > 0) {
  console.log(
    colorize(`\n${warnings.length} string(s) need a human look:`, "yellow"),
  );
  for (const w of warnings) console.log(`  ${w}`);
}

if (failures.length > 0) {
  console.log(colorize(`\n✗ ${failures.length} branding failure(s):`, "red"));
  for (const f of failures) console.log(`  ${f.where}\n    ${f.detail}`);
  console.log(
    colorize(
      "\nRebrand new strings by leaving them alone (the build plugin handles them),\n" +
        "or add a key to BRAND_EXEMPT_KEYS in src/shorthand/branding.ts if it must\n" +
        "keep naming upstream Handy.",
      "yellow",
    ),
  );
  process.exit(1);
}

console.log(
  colorize(
    `\n✓ Branding is consistent across ${locales.length} locales and ${RUST_BRAND_SITES.length} out-of-band sites.`,
    "green",
  ),
);
process.exit(0);
