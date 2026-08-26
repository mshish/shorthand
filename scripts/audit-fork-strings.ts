/**
 * Fork-only. Classifies every key in FORK_ONLY_STRINGS against upstream's
 * English catalogue, so the split between "translatable fork string" and
 * "English copy preference" is derived rather than curated by hand.
 *
 * This is Direction A of "The audit that motivates this plan" — it asks
 * whether content already inside FORK_ONLY_STRINGS is shaped correctly. It
 * cannot see fork content that bypasses FORK_ONLY_STRINGS entirely; that is
 * Direction B, and scripts/check-locale-drift.ts (Task 2) is its permanent
 * gate.
 *
 * Kept after the split: rerun it when adding a fork string to confirm the
 * string is genuinely new, rather than a reworded upstream one that would
 * silently replace 23 translations.
 *
 * Run: bun scripts/audit-fork-strings.ts
 */

import { execFileSync } from "child_process";
import { FORK_ONLY_STRINGS } from "../src/shorthand/branding";

const upstreamEn = JSON.parse(
  execFileSync(
    "git",
    ["show", "upstream/main:src/i18n/locales/en/translation.json"],
    { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
  ),
);

const get = (obj: unknown, dotted: string): unknown =>
  dotted
    .split(".")
    .reduce<unknown>(
      (node, key) =>
        node && typeof node === "object"
          ? (node as Record<string, unknown>)[key]
          : undefined,
      obj,
    );

const normalise = (s: string) => s.toLowerCase().replace(/handy/g, "shorthand");

export const classification = {
  /** No upstream equivalent — genuinely ours. */
  forkOnly: [] as string[],
  /** Same words, different capitalisation. An English preference only. */
  caseOnly: [] as string[],
  /** Same but for the brand name — the substitution already handles it. */
  brandOnly: [] as string[],
  /** Genuinely different wording: the fork's own terminology. */
  semantic: [] as string[],
};

for (const [key, forkValue] of Object.entries(FORK_ONLY_STRINGS)) {
  const upstreamValue = get(upstreamEn, key);
  if (typeof upstreamValue !== "string") {
    classification.forkOnly.push(key);
  } else if (normalise(upstreamValue) !== normalise(forkValue)) {
    classification.semantic.push(key);
  } else if (upstreamValue.toLowerCase() === forkValue.toLowerCase()) {
    classification.caseOnly.push(key);
  } else {
    classification.brandOnly.push(key);
  }
}

if (import.meta.main) {
  for (const [name, keys] of Object.entries(classification)) {
    console.log(`\n--- ${name}: ${keys.length} ---`);
    for (const key of keys) console.log(`  ${key}`);
  }
}
