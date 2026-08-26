# Fork-Only Translation Catalogues Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the fork's own UI strings out of a hardcoded TypeScript object into real per-locale JSON catalogues, so Shorthand's fork-only strings can be translated.

**Architecture:** `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts` is a flat `Record<string, string>` with no locale dimension — it cannot express a translation. This plan moves those strings to `src/shorthand/locales/<lang>.json`, makes `applyBranding()` merge the English catalogue as a base with the active locale's overrides on top, and adds a key-parity gate. The merge happens at exactly the point it happens today — *after* brand substitution — so fork-only strings stay immune to the Handy→Shorthand rename. Every step is protected by a golden hash of the current output: the migration is correct only if all 24 locales render byte-identically to before.

**Tech Stack:** TypeScript, Bun (runtime + built-in test runner), Vite (build-time transform plugin), i18next.

**Spec:** `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md` — Decision 2 and Decision 2a. This plan implements Phase 0b only.

**Execution model:** subagent-driven, with implementation and review kept **independent**. A fresh implementation agent per batch receives the plan and its own tasks only. A separate Codex review agent then receives the resulting diff and the task text — never the implementer's reasoning or transcript — so the review is a genuine second read rather than a confirmation of the first.

Review batches:

| Batch | Tasks | Why grouped |
| --- | --- | --- |
| 1 | Tasks 1–2 | The golden test exists to protect the migration; reviewing the safety net without the thing it catches proves little. |
| 2 | Task 3 | The only real logic change, including the `import.meta.glob`-vs-Bun fork. Reviewed alone. |
| 3 | Tasks 4–5 | New guard script plus the docs that tell people to run it. |

## Global Constraints

- **Never write to `src/i18n/locales/`.** Upstream's 24 catalogues stay byte-identical to `cjpais/Handy` so `git merge upstream/main` never conflicts on them. All fork strings live under `src/shorthand/`.
- **Zero new dependencies.** `docs/FRONTEND_TESTING.md` records that vitest/jest were rejected because they add devDependencies to upstream's `package.json` and `bun.lock` — permanent merge-conflict surface. `bun test` is built into the Bun binary already required by `AGENTS.md`, so it costs nothing. Do not add any package to `dependencies` or `devDependencies`.
- **Only one upstream file may be edited:** `package.json`, and only to add script lines. Keep the edit small and local — no reordering, no reformatting (`AGENTS.md` § "Keep the diff mergeable").
- **Merge order is load-bearing.** Brand substitution runs first, fork-only strings merge on top (`branding.ts:15-17`). That is why fork strings may contain the word "Handy" and mean it. Never reverse this.
- **Fork strings use flat dotted keys** (`"settings.modes.heading"`), not nested objects — matching `FORK_ONLY_STRINGS` today. `setByPath` expands them at merge time. i18next accepts both; flat keeps this migration mechanical and diff-reviewable.
- **`applyBranding()` must stay pure.** Its input is never mutated — the same function backs both the Vite plugin and `scripts/check-branding.ts` (`branding.ts:236-238`).
- Run before every commit: `bun run lint`, `bun run format`, `bun run check:branding`, `bun run check:translations`.

---

### Task 1: Golden hash of current branding output

Locks in today's behaviour before anything changes. Every later task must keep this test green — that is the entire safety argument for the migration.

**Files:**
- Create: `src/shorthand/branding.golden.json`
- Create: `src/shorthand/branding.test.ts`
- Create: `scripts/write-branding-golden.ts`
- Modify: `package.json` (add `test:unit` and `golden:branding` scripts)

**Interfaces:**
- Consumes: `applyBranding(translation, locale)` from `src/shorthand/branding.ts` — existing, unchanged.
- Produces: `src/shorthand/branding.golden.json`, a `Record<string, string>` mapping locale name → SHA-256 hex of the rendered catalogue. Tasks 2 and 3 rely on this file existing and on `bun run test:unit` running it.

- [ ] **Step 1: Write the generator script**

A hash per locale, not a full snapshot — 24 lines instead of ~10,000, and equally decisive about any change.

Create `scripts/write-branding-golden.ts`:

```ts
/**
 * Fork-only. Regenerates the golden hashes in
 * `src/shorthand/branding.golden.json`.
 *
 * Run this ONLY when a fork-only string or an upstream catalogue has
 * deliberately changed. Running it to make a failing test pass defeats the
 * point: the test exists to prove a refactor changed nothing.
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
  if (Array.isArray(value)) {
    return `[${value.map(stableStringify).join(",")}]`;
  }
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
  console.log(
    `Wrote ${Object.keys(golden).length} golden hashes to ${path.relative(process.cwd(), GOLDEN)}`,
  );
}
```

- [ ] **Step 2: Write the failing test**

Create `src/shorthand/branding.test.ts`:

```ts
/**
 * Fork-only. Proves that refactoring the fork-string mechanism does not change
 * a single rendered string in any locale.
 *
 * `bun test` is the runner deliberately: it ships inside the Bun binary this
 * repo already requires, so unlike vitest it adds nothing to upstream's
 * package.json or bun.lock. See docs/FRONTEND_TESTING.md.
 */

import { describe, expect, test } from "bun:test";
import golden from "./branding.golden.json";
import { hashLocale, localeNames } from "../../scripts/write-branding-golden";

describe("applyBranding", () => {
  test("every upstream locale is present in the golden file", () => {
    expect(localeNames().sort()).toEqual(Object.keys(golden).sort());
  });

  for (const locale of localeNames()) {
    test(`${locale} renders byte-identically to the golden hash`, () => {
      expect(hashLocale(locale)).toBe(
        (golden as Record<string, string>)[locale],
      );
    });
  }
});
```

- [ ] **Step 3: Add the scripts to package.json**

Insert these two lines into `"scripts"` immediately after the existing `"check:settings"` line. Change nothing else in the file.

```json
    "test:unit": "bun test src/shorthand",
    "golden:branding": "bun scripts/write-branding-golden.ts",
```

`bun test` is scoped to `src/shorthand` on purpose: unscoped, it would also try to run the Playwright specs under `tests/`, which need a browser and a dev server.

- [ ] **Step 4: Run the test to verify it fails**

Run: `bun run test:unit`
Expected: FAIL — `branding.golden.json` does not exist yet, so the import cannot resolve.

- [ ] **Step 5: Generate the golden file**

Run: `bun run golden:branding`
Expected: `Wrote 24 golden hashes to src/shorthand/branding.golden.json`

- [ ] **Step 6: Run the test to verify it passes**

Run: `bun run test:unit`
Expected: PASS — 25 tests (one presence check, 24 locale hashes).

- [ ] **Step 7: Prove the test can actually fail**

A golden test that cannot fail is worse than none. Temporarily add a key to `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts`:

```ts
  "settings.modes.heading": "How each mode behaves TEMPORARY",
```

Run: `bun run test:unit`
Expected: FAIL on `en renders byte-identically to the golden hash` (and only on `en`).

Now revert that edit and re-run:

Run: `bun run test:unit`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/shorthand/branding.test.ts src/shorthand/branding.golden.json scripts/write-branding-golden.ts package.json
git commit -m "test: pin branding output with golden hashes before catalogue migration

The fork-string mechanism is about to move from a TypeScript object to
JSON catalogues. These hashes are what make that refactor provably
behaviour-preserving rather than merely plausible."
```

---

### Task 2: Move the English strings into a catalogue file

Pure relocation. `applyBranding` still merges exactly one set of strings; they just live in JSON now. The golden test proves nothing moved.

**Files:**
- Create: `src/shorthand/locales/en.json`
- Modify: `src/shorthand/branding.ts:32-167` (replace the inline object with a JSON import)
- Test: `src/shorthand/branding.test.ts` (existing, unchanged — it must simply still pass)

**Interfaces:**
- Consumes: nothing new.
- Produces: `src/shorthand/locales/en.json`, a flat `Record<string, string>`. `FORK_ONLY_STRINGS` remains an exported `Record<string, string>` with an unchanged shape and unchanged contents, so `scripts/check-branding.ts:23` keeps working without edits. Task 3 replaces how it is populated, not its type.

- [ ] **Step 1: Extract the catalogue mechanically**

Do **not** hand-copy ~80 string pairs. Transcription errors in quoted prose are exactly what a human misses in review, and the values contain apostrophes, em-dashes and embedded quotes. Generate the file from the object that is already in memory.

Run this one-off from the repo root:

```bash
bun -e '
import { FORK_ONLY_STRINGS } from "./src/shorthand/branding";
import fs from "fs";
fs.mkdirSync("src/shorthand/locales", { recursive: true });
fs.writeFileSync(
  "src/shorthand/locales/en.json",
  JSON.stringify(FORK_ONLY_STRINGS, null, 2) + "\n",
);
console.log(Object.keys(FORK_ONLY_STRINGS).length + " keys written");
'
```

Expected: `80 keys written` (or whatever the current count is — note it, and check it against the key count `check:fork-translations` reports in Task 4).

Verify the round trip before going further:

```bash
bun -e '
import { FORK_ONLY_STRINGS } from "./src/shorthand/branding";
import written from "./src/shorthand/locales/en.json";
const a = JSON.stringify(FORK_ONLY_STRINGS);
const b = JSON.stringify(written);
console.log(a === b ? "identical" : "MISMATCH");
'
```

Expected: `identical`.

The explanatory comments in `branding.ts` (why "Meetings" was renamed, why sentence case is all-or-nothing, why two strings deliberately say "Handy") do **not** move — JSON cannot hold them, and they explain the *policy*, which stays in `branding.ts`. Step 2 keeps them.

- [ ] **Step 2: Replace the inline object with an import**

In `src/shorthand/branding.ts`, delete the object literal at lines 32-167 and put this in its place. Keep the surrounding doc comments; fold the ones that explained individual entries into the block comment below so the reasoning is not lost.

```ts
import forkEn from "./locales/en.json";

/**
 * Strings that exist only in this fork. Merged in after substitution, so they
 * are authoritative and immune to the rename.
 *
 * They live in `./locales/*.json` rather than inline here so they can be
 * translated: this object had no locale dimension at all, and a TypeScript
 * object is not something a translator can work with. See
 * `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md` § 2a.
 *
 * Flat dotted keys, matching what this object held before the move.
 * `setByPath` expands them; i18next accepts either shape.
 *
 * These never reach `src/i18n/locales/`, so `check:translations` — which
 * compares key parity between `en` and the other 23 upstream catalogues on
 * disk — never sees them and cannot fail on them. `check:fork-translations`
 * is their equivalent gate.
 *
 * Policy notes that used to sit beside individual entries:
 * - Upstream labels settings in Title Case; the fork's copy rule is sentence
 *   case, applied all-or-nothing. Half-converted reads as a bug in a way that
 *   uniformly Title Case did not. Acronyms and proper nouns keep their
 *   capitals: API, URL, ONNX, English, Handy, Beta, What's New.
 * - `settings.about.showAllSettings.*` say "Handy" on purpose: they name the
 *   upstream project. The merge order below is what protects them.
 * - The shortcut rows end in `.name`, not `.label`/`.title`, which is why an
 *   early sweep missed them.
 */
export const FORK_ONLY_STRINGS: Record<string, string> = forkEn;
```

- [ ] **Step 3: Run the golden test**

Run: `bun run test:unit`
Expected: PASS, all 25 tests. A failure here means a value was altered in transit — diff `en.json` against `git show HEAD:src/shorthand/branding.ts` rather than regenerating the golden file.

- [ ] **Step 4: Run the existing guards**

Run: `bun run check:branding && bun run check:translations && bun run lint`
Expected: all pass. `check:branding` still imports `FORK_ONLY_STRINGS` and must be untouched by this task.

- [ ] **Step 5: Verify the build still transforms catalogues**

Run: `bun run build`
Expected: completes. TypeScript resolves the JSON import (`resolveJsonModule` is implied by Vite's defaults; if `tsc` objects, that is a real finding — report it rather than adding a `// @ts-expect-error`).

- [ ] **Step 6: Commit**

```bash
git add src/shorthand/locales/en.json src/shorthand/branding.ts
git commit -m "refactor: move fork-only strings into a JSON catalogue

Behaviour-identical — the golden hashes are unchanged. This is the step
that makes the strings translatable at all; a Record<string, string> had
nowhere to put a second language."
```

---

### Task 3: Make the merge locale-aware

Adds the locale dimension. With only `en.json` present the behaviour is unchanged, so the golden test still guards the refactor.

**Files:**
- Modify: `src/shorthand/branding.ts` (add `forkStringsFor`, use it in `applyBranding`)
- Test: `src/shorthand/branding.test.ts` (add locale-merge cases)

**Interfaces:**
- Consumes: `src/shorthand/locales/en.json` from Task 2.
- Produces: `forkStringsFor(locale: string): Record<string, string>` — returns the English catalogue merged under any same-named locale catalogue. Exported for `scripts/check-fork-translations.ts` in Task 4. `FORK_ONLY_STRINGS` stays exported and English-only, because `check-branding.ts` uses it to ask "is this key deliberately ours?", which is a locale-independent question.

- [ ] **Step 1: Write the failing tests**

Append to `src/shorthand/branding.test.ts`:

```ts
import { forkStringsFor } from "./branding";

describe("forkStringsFor", () => {
  test("returns the English catalogue for en", () => {
    expect(forkStringsFor("en")["sidebar.modes"]).toBe("Modes");
  });

  test("falls back to English for a locale with no catalogue", () => {
    expect(forkStringsFor("de")["sidebar.modes"]).toBe("Modes");
  });

  test("every locale gets the full English key set", () => {
    const en = Object.keys(forkStringsFor("en")).sort();
    expect(Object.keys(forkStringsFor("ja")).sort()).toEqual(en);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test:unit`
Expected: FAIL — `forkStringsFor is not a function` / import cannot be resolved.

- [ ] **Step 3: Implement the locale-aware lookup**

In `src/shorthand/branding.ts`, after the `FORK_ONLY_STRINGS` export, add:

```ts
/**
 * Every fork-only catalogue on disk, keyed by locale.
 *
 * `import.meta.glob` with `eager` is Vite's documented way to pull a directory
 * of JSON in at build time. It resolves statically, so adding `de.json`
 * requires no registration step — dropping the file in is the whole change.
 */
const FORK_CATALOGUES: Record<string, Record<string, string>> = Object.
  fromEntries(
    Object.entries(
      import.meta.glob<{ default: Record<string, string> }>(
        "./locales/*.json",
        { eager: true },
      ),
    ).map(([filePath, module]) => [
      // "./locales/de.json" -> "de"
      filePath.slice("./locales/".length, -".json".length),
      module.default,
    ]),
  );

/**
 * The fork's strings for one locale: English as the base, that locale's own
 * catalogue layered on top.
 *
 * English is the base rather than a lookup-time fallback so that every locale
 * receives a complete key set. A partially-translated catalogue then renders
 * its translated keys and leaves the rest in English, instead of rendering a
 * raw key path where a string should be.
 */
export function forkStringsFor(locale: string): Record<string, string> {
  const base = FORK_CATALOGUES["en"] ?? {};
  const override = FORK_CATALOGUES[locale];
  return override ? { ...base, ...override } : { ...base };
}
```

Then change the merge loop at the end of `applyBranding` from:

```ts
  for (const [path, value] of Object.entries(FORK_ONLY_STRINGS)) {
```

to:

```ts
  for (const [path, value] of Object.entries(forkStringsFor(locale))) {
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `bun run test:unit`
Expected: PASS — the 25 golden tests plus 3 new ones. The golden hashes must be unchanged: with only `en.json` on disk, `forkStringsFor` returns exactly what `FORK_ONLY_STRINGS` returned.

- [ ] **Step 5: Confirm `import.meta.glob` resolves under Bun**

`import.meta.glob` is a Vite transform, not a runtime API, so it does not exist when `check-branding.ts` runs under plain Bun.

Run: `bun run check:branding`

If it fails with `import.meta.glob is not a function`, that is expected and must be fixed now, not worked around. Replace the `FORK_CATALOGUES` initialiser with a runtime directory read, which works identically in both environments:

```ts
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const LOCALES = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "locales",
);

const FORK_CATALOGUES: Record<string, Record<string, string>> =
  Object.fromEntries(
    fs
      .readdirSync(LOCALES)
      .filter((file) => file.endsWith(".json"))
      .map((file) => [
        file.slice(0, -".json".length),
        JSON.parse(fs.readFileSync(path.join(LOCALES, file), "utf8")),
      ]),
  );
```

Note this makes `branding.ts` depend on `node:fs`, which is fine for the build plugin and the guard scripts but would break if `branding.ts` were ever imported into browser code. It is not today — its only importers are `vite-branding-plugin.ts` (build-time) and `check-branding.ts` (Bun). Record that constraint in the file's header comment.

Run: `bun run check:branding && bun run test:unit && bun run build`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/shorthand/branding.ts src/shorthand/branding.test.ts
git commit -m "feat: give fork-only strings a locale dimension

English is the merge base rather than a lookup-time fallback, so a
partially-translated catalogue renders English for its untranslated keys
instead of a raw key path."
```

---

### Task 4: Key-parity gate for fork catalogues

Without this, a `de.json` missing a key fails silently to English and nobody finds out. This is the fork-side equivalent of `check:translations`.

**Files:**
- Create: `scripts/check-fork-translations.ts`
- Create: `src/shorthand/locales/README.md`
- Modify: `package.json` (add `check:fork-translations`)

**Interfaces:**
- Consumes: `forkStringsFor` is *not* used here — the gate must compare raw files on disk, since `forkStringsFor` merges English in and would mask every missing key.
- Produces: `bun run check:fork-translations`, exiting non-zero on any key mismatch.

- [ ] **Step 1: Write the gate**

Create `scripts/check-fork-translations.ts`:

```ts
/**
 * Fork-only. Key parity across `src/shorthand/locales/*.json`.
 *
 * `check-translations.ts` covers upstream's catalogues under
 * `src/i18n/locales/`. It cannot see these: fork-only strings deliberately
 * never enter those files, so upstream merges never conflict on them. This is
 * their gate.
 *
 * Reads the raw files rather than calling `forkStringsFor`, which merges
 * English in as a base and would therefore report every catalogue as complete.
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

function load(locale: string): Record<string, string> {
  return JSON.parse(
    fs.readFileSync(path.join(LOCALES, `${locale}.json`), "utf8"),
  );
}

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
    console.log(colorize(`✓ ${locale}: all ${referenceKeys.size} keys`, "green"));
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
```

- [ ] **Step 2: Add the script to package.json**

Insert after the `check:branding` line, changing nothing else:

```json
    "check:fork-translations": "bun scripts/check-fork-translations.ts",
```

- [ ] **Step 3: Run it against the current state**

Run: `bun run check:fork-translations`
Expected: PASS with `✓ 0 fork translation(s) match en (N keys).` — only `en.json` exists, so there is nothing to compare yet. A gate that passes vacuously is correct here; Step 4 proves it is not merely inert.

- [ ] **Step 4: Prove the gate catches a real gap**

Create a deliberately incomplete `src/shorthand/locales/de.json`:

```json
{
  "sidebar.modes": "Modi"
}
```

Run: `bun run check:fork-translations`
Expected: FAIL, exit 1, reporting `de` missing every key except `sidebar.modes`.

Then confirm the merge still renders correctly with a partial catalogue:

Run: `bun run test:unit`
Expected: PASS — `de` picks up `"Modi"` for `sidebar.modes` and English for everything else. This is the fallback behaviour from Task 3 working end to end.

Now delete the fixture:

```bash
rm src/shorthand/locales/de.json
```

Run: `bun run check:fork-translations && bun run test:unit`
Expected: both PASS.

- [ ] **Step 5: Document the directory for future translators**

Create `src/shorthand/locales/README.md`:

```markdown
# Fork-only translation catalogues

Strings that exist only in Shorthand, not in upstream Handy.

Upstream's catalogues live in `src/i18n/locales/<lang>/translation.json` and
are kept **byte-identical to upstream** so `git merge upstream/main` never
conflicts on them. Fork strings therefore cannot go there. They live here
instead, and are merged into the bundle at build time by
`src/shorthand/vite-branding-plugin.ts` — after the Handy→Shorthand
substitution, which is why a string here may say "Handy" and mean it.

## Adding a language

1. Copy `en.json` to `<lang>.json`, matching a locale directory name under
   `src/i18n/locales/`.
2. Translate the values. Leave the keys alone.
3. Run `bun run check:fork-translations`.

Every key in `en.json` must be present. An untranslated key renders in
English rather than failing, but the gate still requires it: silent English
in a translated UI is a bug that nobody reports.

Keys are flat and dotted (`"settings.modes.heading"`), unlike upstream's
nested catalogues. Both are valid i18next; flat is what these strings used
before they were a file, and keeping the shape made the migration reviewable.
```

- [ ] **Step 6: Run every gate**

Run: `bun run lint && bun run format:check && bun run check:branding && bun run check:translations && bun run check:fork-translations && bun run test:unit && bun run build`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add scripts/check-fork-translations.ts src/shorthand/locales/README.md package.json
git commit -m "feat: key-parity gate for fork-only translation catalogues

check:translations covers upstream's catalogues and structurally cannot see
these. A missing key falls back to English silently, so this fails rather
than warns."
```

---

### Task 5: Record the mechanism where the next person will look

The reasoning lives in a spec that a future reader has no reason to open. `AGENTS.md` and `BRANDING.md` are where someone looks when adding a string.

**Files:**
- Modify: `AGENTS.md` (§ Internationalization)
- Modify: `BRANDING.md`
- Modify: `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md` (mark Phase 0b done)

**Interfaces:**
- Consumes: everything above.
- Produces: no code.

- [ ] **Step 1: Correct the i18n section in AGENTS.md**

`AGENTS.md` § Internationalization currently says to add new keys to `src/i18n/locales/en/translation.json`. For fork-only strings that is now wrong and would reintroduce the conflict surface the whole mechanism exists to avoid. Replace the "Adding new text" block with:

```markdown
**Adding new text:**

Which file depends on whether the string exists upstream.

- **A string upstream also has** — add the key to
  `src/i18n/locales/en/translation.json` and follow
  [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **A fork-only string** — add it to `src/shorthand/locales/en.json`. Upstream's
  24 catalogues stay byte-identical to `cjpais/Handy` so merges never conflict
  on them; fork strings are merged in at build time instead. See
  [`src/shorthand/locales/README.md`](src/shorthand/locales/README.md).

Then use it in the component: `const { t } = useTranslation(); t('key.path')`

Gates: `bun run check:translations` covers upstream's catalogues,
`bun run check:fork-translations` covers the fork's, `bun run check:branding`
covers the rename.
```

- [ ] **Step 2: Point BRANDING.md at the new location**

`BRANDING.md` describes `FORK_ONLY_STRINGS` as the home for fork-only strings. Add, after that description:

```markdown
As of 2026-08-26 the strings themselves live in `src/shorthand/locales/*.json`,
not inline in `branding.ts`. `FORK_ONLY_STRINGS` is still exported — it is the
English catalogue, and `check-branding.ts` uses it to answer the
locale-independent question "is this key deliberately ours?". The merge order
is unchanged: substitution first, fork strings on top.
```

- [ ] **Step 3: Mark the phase complete in the spec**

In `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md`, change the Phase 0b heading line to:

```markdown
**Phase 0b — fork-only catalogues. DONE 2026-08-26**, see
`docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md`.
```

- [ ] **Step 4: Verify the docs match reality**

Re-read each instruction you just wrote and run the command it names. A doc that tells someone to run `bun run check:fork-translations` is wrong if the script is not in `package.json`.

Run: `bun run check:fork-translations && bun run check:translations && bun run check:branding`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add AGENTS.md BRANDING.md docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md
git commit -m "docs: record where fork-only strings live now

AGENTS.md still sent people to src/i18n/locales/en/translation.json, which
for a fork-only string reintroduces exactly the conflict surface the
build-time merge exists to avoid."
```

---

## Done when

- `src/shorthand/locales/en.json` holds every fork-only string; `branding.ts` holds the policy and the merge, not the data.
- Adding `de.json` is the entire cost of adding German — no registration step.
- `bun run check:fork-translations` fails on an incomplete catalogue, and has been observed failing.
- The golden hashes are unchanged from before the migration, proving no rendered string moved.
- `AGENTS.md` tells the next person the right file.

## Deliberately not in this plan

- **Translating anything.** This builds the mechanism; the first real `de.json` is separate work.
- **Nesting the keys** to match upstream's catalogue shape. Flat keys are what these strings already used, and keeping the shape is what makes the migration reviewable. Revisit if translation tooling demands it.
- **A React unit harness.** `bun test` here covers pure functions only. `docs/FRONTEND_TESTING.md` recommends Playwright for component behaviour, and nothing in this plan renders a component.
- **The umbrella installer** (Phases 0, 1, 2, 3 of the spec). Phase 0b was separated precisely because it is independent and unblocked.
