# Fork-Only Translation Catalogues Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the fork's hardcoded UI strings into the two things they
actually are — translatable fork content, and English copy preferences — so
fork strings can be translated, so 43 of them stop overwriting real
translations in 23 languages, and so the fork's 24 locale catalogues stop
drifting from upstream's. A second audit, run while reviewing this plan, found
that 32 fork-only keys (dictation mode, system-audio capture) had already been
written directly into all 24 locale files instead of through
`FORK_ONLY_STRINGS` — 1568 inserted lines of exactly the defect this plan
exists to prevent. Restoring byte-identity with upstream is now Task 2 of this
plan, ahead of the original split.

**Architecture:** `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts` is one
flat `Record<string, string>` merged into *every* locale. Two audits motivate
this plan, run in opposite directions. The first, against upstream's English
catalogue, classifies the keys already inside `FORK_ONLY_STRINGS` and shows
they hold two unrelated things: strings genuinely fork-only, and strings that
differ from upstream **only in English capitalisation** (Title Case → sentence
case). Because the merge is locale-independent, the capitalisation ones
replace each locale's real translation with English. The second audit checks
the opposite direction — every key present in `src/i18n/locales/` but absent
from `upstream/main` — and found 32 more fork-only keys that never went
through `FORK_ONLY_STRINGS` at all: written straight into all 24 locale files,
in English, by a later feature branch, after an earlier commit had already
established the "never edit the locale files" rule this plan depends on. Task
2 moves that content into the mechanism first, so by the time Task 3 splits
`FORK_ONLY_STRINGS` it is classifying 108 keys, not 81. From there:
`src/shorthand/locales/<lang>.json` holds translatable fork strings merged
into all locales; `src/shorthand/english-copy.json` holds the casing
preferences and merges into `en` alone.

**Tech Stack:** TypeScript, Bun (runtime + built-in test runner), Vite
(build-time transform plugin), i18next.

**Spec:** `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md` —
Decision 2 and Decision 2a. This plan implements Phase 0b only.

**Execution model:** subagent-driven, with implementation and review kept
**independent**. A fresh implementation agent per batch receives the plan and
its own tasks only. A separate Codex review agent then receives the resulting
diff and the task text — never the implementer's reasoning or transcript — so
the review is a genuine second read rather than a confirmation of the first.

| Batch | Tasks | Why grouped |
| --- | --- | --- |
| 1 | Tasks 1–3 | Baseline, then two pure reorganisations — restoring byte-identity with upstream, then splitting the strings into two files. Neither may change a rendered string, with one named, temporary exception in Task 2 that Task 4 closes out. |
| 2 | Task 4 | The only lasting behaviour change in the plan: 23 locales get their translations back, and one shortcut description gets its fork wording back, in English only this time. Reviewed alone. |
| 3 | Tasks 5–7 | Locale-aware lookup, its parity gate, and the docs that direct contributors to it. |

## Global Constraints

- **Never write to `src/i18n/locales/`.** Upstream's 24 catalogues must stay
  byte-identical to `cjpais/Handy` so `git merge upstream/main` never
  conflicts on them. **This is not true today.** The audit below found 1568
  lines of fork-only content already written directly into all 24 files; Task
  2 restores byte-identity, and the `check:locale-drift` gate it adds keeps it
  that way going forward. All fork strings live under `src/shorthand/`.
- **Zero new dependencies.** `docs/FRONTEND_TESTING.md` records that
  vitest/jest were rejected because they add devDependencies to upstream's
  `package.json` and `bun.lock` — permanent merge-conflict surface. `bun test`
  is built into the Bun binary `AGENTS.md` already requires, so it costs
  nothing. Do not add any package to `dependencies` or `devDependencies`.
- **Only one upstream file may be edited:** `package.json`, and only to add
  script lines. Keep the edit small and local (`AGENTS.md` § "Keep the diff
  mergeable"). (`src/i18n/locales/` is also technically "upstream" but has its
  own, stricter rule above: edits there are not additive script lines, they
  are the defect.)
- **Merge order is load-bearing.** Brand substitution runs first, fork strings
  merge on top. That is why a fork string may contain the word "Handy" and
  mean it. Never reverse this.
- **A string that matches upstream apart from Handy/Shorthand must use
  upstream's**, so its existing translations survive. That is the reason for
  the `english-copy.json` split. **A string that upstream does not have at all
  must never be written into the locale files themselves** — that is the
  reason for Task 2. Both are the same underlying rule: fork content belongs
  in `src/shorthand/`, never in `src/i18n/locales/`.
- **Translation process is upstream's process**, unchanged: contributors fork,
  copy the English file, translate values, open a PR
  (`CONTRIBUTING_TRANSLATIONS.md`). No translation platform, no machine
  translation, no new tooling.
- **Flat dotted keys** (`"settings.modes.heading"`) in fork files, matching
  what `FORK_ONLY_STRINGS` holds today. `setByPath` expands them.
- **`applyBranding()` must stay pure** — its input is never mutated, so the
  same function backs the Vite plugin and `scripts/check-branding.ts`.
- Run before every commit: `bun run lint`, `bun run format`, `bun run
  check:branding`, `bun run check:locale-drift`, `bun run check:translations`,
  `bun run check:fork-translations` (the last two gates come from this plan;
  add them to your local habit as each lands).

## The audit that motivates this plan

Two audits, run in opposite directions, because a single-direction audit
missed a defect the review of this plan caught.

### Direction A — is everything already in `FORK_ONLY_STRINGS` genuinely ours?

Run against `upstream/main`'s `src/i18n/locales/en/translation.json`,
classifying all 81 keys currently in `FORK_ONLY_STRINGS`:

| Category | Count | Disposition |
| --- | --- | --- |
| Key absent upstream — genuinely fork-only | 35 | → `locales/en.json`, translatable |
| Differs only by English capitalisation | 43 | → `english-copy.json`, `en` only |
| Deliberate semantic rename | 3 | → `locales/en.json`, translatable |
| Differs only by brand name | 0 | — |

The 3 semantic renames are `settings.debug.postProcessingToggle.label` ("Post
Processing" → "AI cleanup"), `settings.general.shortcut.bindings.transcribe.name`
("Transcribe Shortcut" → "Capture shortcut"), and
`settings.general.shortcut.bindings.transcribe_with_post_process.name`
("Post-Processing Hotkey" → "AI cleanup shortcut"). These are the fork's chosen
terminology, so they belong with the translatable strings and show English
until someone translates them.

**A bug this audit surfaced:** `settings.general.shortcut.title` is overridden
to `"Handy shortcuts"`. Fork strings bypass brand substitution, so that renders
literally "Handy shortcuts" in the Shorthand UI — the override reintroduces
the name the mechanism exists to remove. Upstream's "Handy Shortcuts" would
have been substituted correctly had it been left alone. Task 4 fixes it to
"Shorthand shortcuts".

Direction A only asks whether the 81 keys *already inside* `FORK_ONLY_STRINGS`
are genuinely ours. It cannot see fork content that never entered the
mechanism in the first place.

### Direction B — is there fork content that never went through `FORK_ONLY_STRINGS` at all?

It had. Diffing every key in the fork's `src/i18n/locales/` tree against the
matching path in `upstream/main` — not `FORK_ONLY_STRINGS`, the files
themselves — finds what `git diff --stat upstream/main -- src/i18n/locales`
already shows at a glance: **24 files changed, 1568 insertions(+), 80
deletions(-)**. Two distinct defects, found by two different comparisons,
produce that number.

**B1 — 32 keys present in the fork's catalogues, absent from upstream's,
written directly into all 24 locale files.** Dictation mode and system-audio
capture, added by a later feature branch (`16f1d38`, `3e65783`) after an
earlier commit (`9d1852f`, "rebrand the UI at build time instead of editing
locale files") had already established the rule this violates. Every one of
the 24 files carries byte-identical English text for all 32 keys — verified by
comparing all 24 locale files against each other, not just against upstream —
because nothing translated them: they were never reachable by the fork-string
mechanism, so no translation process ever saw them.

| Subtree | Count |
| --- | --- |
| `settings.dictation.*` | 13 |
| `settings.advanced.*` (`followStream`, `systemAudio`, `systemAudioDevice`) | 9 |
| `settings.general.shortcut.bindings.dictate*` | 4 |
| `settings.history.source.*` | 2 |
| `transcript.*` / `sidebar.dictation` | 4 |

5 of the 32 already have a live entry in `FORK_ONLY_STRINGS`
(`settings.advanced.systemAudio.label`, `settings.advanced.systemAudioDevice.title`,
`settings.dictation.enable.label`, `settings.dictation.privacy.saveRecordings.label`,
`settings.dictation.privacy.saveTranscripts.label`): their copy sitting in the
locale files is already dead, because the merge overlay wins regardless of
what the raw file says. The other **27 render directly from the raw locale
file**, with nothing covering them, in every language.

**B2 — a value edit to a key upstream also has**, found by comparing values on
shared keys rather than just key presence.
`settings.general.shortcut.bindings.transcribe.name` was overwritten to the
English "Capture Shortcut" directly in all 24 files — also already dead, since
the same key is a live `FORK_ONLY_STRINGS` entry ("Capture shortcut", lower
case) that wins the merge regardless of the raw file. Its sibling
`.description`, however, was rewritten in `en` only — upstream says "The
keyboard shortcut to record and transcribe your voice," the fork's raw `en`
file says "...a meeting or note" — and nothing covers it. It is the one piece
of this drift with no dead-duplicate shortcut, and Task 2 handles it as a
named exception (Task 2, Step 5).

**Found by the same value comparison, and explicitly not fixed by this
plan:** `src/i18n/locales/tr/translation.json` differs from upstream on 8
further keys, under `settings.advanced.acceleration.transcribe.*`,
`settings.advanced.overlay.style.*`, `settings.advanced.overlay.position.*`
and `settings.about.acknowledgments.ggml.*`. Unlike B1 and B2, this is not
fork content squatting outside the mechanism — both the fork's and upstream's
values are Turkish. It reads as a translation that fell behind an upstream key
rename (`acceleration.whisper` → `acceleration.transcribe`, and a further
rewording upstream in commit `8abb802`, "make the Turkish translation
internally consistent") and was never caught up. Deciding whether the fork's
current Turkish wording is a deliberate improvement or a regression is a
translation-quality judgement, not a fork-string relocation, and this plan
does not make it. See "Deliberately not in this plan."

Both audits matter and neither substitutes for the other: Direction A finds
content inside the mechanism that shouldn't be shaped the way it is; Direction
B finds content that isn't in the mechanism and should be. `scripts/audit-fork-strings.ts`
(Task 3) keeps asking Direction A's question on every future addition;
`scripts/check-locale-drift.ts` (Task 2) keeps asking Direction B's, so this
category of regression cannot recur silently a second time.

---

### Task 1: Golden hash of current branding output

Locks in today's behaviour before anything changes. Task 2 must keep this
green, with one named, temporary, single-field exception for `en` (Step 5 of
that task explains why, and Task 4 closes it out). Task 3 must keep it green
without exception. Task 4 changes it deliberately, for 23 locales plus that
one `en` field, and regenerates it with a reviewed diff.

**Files:**
- Create: `src/shorthand/branding.golden.json`
- Create: `src/shorthand/branding.test.ts`
- Create: `scripts/write-branding-golden.ts`
- Modify: `package.json` (add `test:unit` and `golden:branding` scripts)

**Interfaces:**
- Consumes: `applyBranding(translation, locale)` from `src/shorthand/branding.ts` — existing, unchanged.
- Produces: `hashLocale(locale: string): string` and `localeNames(): string[]`, both exported from `scripts/write-branding-golden.ts` and imported by the test. `src/shorthand/branding.golden.json` is a `Record<string, string>` of locale → SHA-256 hex.

- [ ] **Step 1: Write the generator script**

Create `scripts/write-branding-golden.ts`:

```ts
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
```

- [ ] **Step 2: Write the failing test**

Create `src/shorthand/branding.test.ts`:

```ts
/**
 * Fork-only. Proves that reorganising the fork-string mechanism does not
 * change a rendered string in any locale — except where a task deliberately
 * changes one and regenerates the golden file.
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

Insert into `"scripts"` immediately after the existing `"check:settings"`
line. Change nothing else in the file.

```json
    "test:unit": "bun test src/shorthand",
    "golden:branding": "bun scripts/write-branding-golden.ts",
```

`bun test` is scoped to `src/shorthand` on purpose: unscoped, it would also
try to run the Playwright specs under `tests/`, which need a browser and a dev
server, and would pick up `src/components/update-checker/portableInstaller.test.ts`,
which is a plain assertion script rather than a `bun:test` suite.

- [ ] **Step 4: Run the test to verify it fails**

Run: `bun run test:unit`
Expected: FAIL — `branding.golden.json` does not exist, so the import cannot resolve.

- [ ] **Step 5: Generate the golden file**

Run: `bun run golden:branding`
Expected: `Wrote 24 golden hashes.`

- [ ] **Step 6: Run the test to verify it passes**

Run: `bun run test:unit`
Expected: PASS — 25 tests.

- [ ] **Step 7: Prove the test can actually fail**

Temporarily change a value in `FORK_ONLY_STRINGS` (`src/shorthand/branding.ts`), e.g. `"sidebar.modes": "Modes TEMPORARY"`.

Run: `bun run test:unit`
Expected: FAIL on every locale hash — `FORK_ONLY_STRINGS` merges into all 24 today, which is precisely the problem this plan fixes.

Revert the edit and re-run.
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/shorthand/branding.test.ts src/shorthand/branding.golden.json scripts/write-branding-golden.ts package.json
git commit -m "test: pin branding output with golden hashes before the catalogue split

Step 7 of this task demonstrates the defect being fixed: changing one
fork string invalidates all 24 locale hashes, because fork strings are
merged into every locale regardless of language."
```

---

### Task 2: Restore byte-identity with upstream

Direction B of the audit above found 32 keys — 34, counting the shared-key
value drift in B2 — sitting directly in `src/i18n/locales/`, in violation of
the Global Constraint every later task in this plan depends on. This task
removes them, gives the 27 genuinely uncovered keys a home in the mechanism
Task 3 is about to reorganise, and adds a permanent gate so this cannot
recur silently a second time.

This is a reorganisation, not a rewrite: every value it moves is copied
verbatim, with **one** named, temporary exception (Step 5) that Task 4 closes
out. Do not use this task to also apply the sentence-case convention to the 27
migrated strings — that is a separate editorial decision this task does not
make, and making it here would hide a real rendered-output change inside a
task whose entire point is to have none.

**Files:**
- Create: `scripts/check-locale-drift.ts`
- Modify: `src/shorthand/branding.ts` (add 27 entries to `FORK_ONLY_STRINGS`; see Step 3)
- Modify: all 24 `src/i18n/locales/<lang>/translation.json` (delete the 32 fork-only keys and the dead `transcribe.name` duplicate; fix `transcribe.description` — see Step 5)
- Modify: `package.json` (add `check:locale-drift`)
- Modify: `src/shorthand/branding.golden.json` (one line — `en` only, temporarily — see Step 5)
- Modify: `src/shorthand/branding.test.ts` (note the temporary exception so Task 4 knows to remove it)

**Interfaces:**
- Consumes: `upstream/main` via `git show`, the same pattern `scripts/audit-fork-strings.ts` (Task 3) already uses.
- Produces: `bun run check:locale-drift`, exiting non-zero on any key present
  in a locale file but absent from `upstream/main`'s matching file. `--fix`
  removes exactly those keys and nothing else — it does not touch keys that
  exist in both fork and upstream with differing *values* (see Step 7 for why
  that is a separate, non-blocking check).

- [ ] **Step 1: Write the drift checker**

Create `scripts/check-locale-drift.ts`:

```ts
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
```

- [ ] **Step 2: Run it and confirm it reproduces the audit**

Run: `bun scripts/check-locale-drift.ts`
Expected: FAIL. All 24 locales report the same 32 keys from Direction B1,
plus `settings.general.shortcut.bindings.transcribe.name` (present in all 24,
absent upstream by value — wait, it *is* present upstream, just with a
different value, so this key-presence-only script will **not** report it).
That is expected and correct given this script's scope (Step 7 explains why
value drift is a separate, non-blocking check) — do not widen this script to
catch it; handle `transcribe.name` and `.description` by hand in Steps 4–5.

- [ ] **Step 3: Add the 27 uncovered keys to `FORK_ONLY_STRINGS`, verbatim**

In `src/shorthand/branding.ts`, add this block to the `FORK_ONLY_STRINGS`
object literal. Task 3 classifies and relocates it, along with everything
else already there, in the next task — do not recase or reword anything here.

```ts
  // ---- Migrated from src/i18n/locales/, where they were written directly
  // instead of through this file — see "The audit that motivates this plan",
  // Direction B1. Every locale carried the identical English text, so moving
  // these here changes nothing anyone sees; it only makes them reachable by
  // translation and gets them out of files that must match upstream.
  "settings.advanced.followStream.description":
    "Allow local tools to follow live transcript events by running `handy --follow-stream`.",
  "settings.advanced.followStream.label": "Follow Live Transcript Output",
  "settings.advanced.systemAudio.description":
    "Capture Windows system output as a separate speaker-labelled transcription alongside the microphone.",
  "settings.advanced.systemAudio.muteConflict":
    "Turn off Capture System Audio before enabling Mute While Recording.",
  "settings.advanced.systemAudio.streamingRequired":
    "System audio capture requires a model that supports streaming.",
  "settings.advanced.systemAudioDevice.default": "Default",
  "settings.advanced.systemAudioDevice.description":
    "Choose the output device to capture. Default follows the current Windows default output.",
  "settings.dictation.enable.description":
    "Turn on a separate dictation mode, with its own shortcut, that pastes text into whatever window has focus.",
  "settings.dictation.enable.shortcutConflict":
    "Could not enable dictation: one of its shortcuts is already in use, either by another Shorthand shortcut or another application. Choose a different shortcut below and try again.",
  "settings.dictation.footer":
    "Microphone, model and language come from the Capture and Transcription sections.",
  "settings.dictation.groups.aiCleanup": "AI Cleanup",
  "settings.dictation.groups.privacy": "Privacy",
  "settings.dictation.groups.shortcut": "Shortcut",
  "settings.dictation.overlayPosition.sharedDescription":
    "Where the overlay appears on screen. Shared with meeting mode — this isn't a per-mode setting.",
  "settings.dictation.postProcessing.hint":
    "Configure providers, API keys and models in the Post-Processing section.",
  "settings.dictation.privacy.saveRecordings.description":
    "Keep the audio recording for each dictation.",
  "settings.dictation.privacy.saveTranscripts.description":
    "Keep the transcript text for each dictation.",
  "settings.general.shortcut.bindings.dictate.description":
    "The keyboard shortcut to start and stop dictation.",
  "settings.general.shortcut.bindings.dictate.name": "Dictation Shortcut",
  "settings.general.shortcut.bindings.dictate_with_post_process.description":
    "Optional: a dedicated hotkey that always applies AI cleanup to your dictation.",
  "settings.general.shortcut.bindings.dictate_with_post_process.name":
    "Dictation AI Cleanup Hotkey",
  "settings.history.source.dictation": "Dictation",
  "settings.history.source.meeting": "Meeting",
  "sidebar.dictation": "Dictation",
  "transcript.retryUnavailable":
    "Retry is unavailable for transcripts containing system audio.",
  "transcript.speakerMic": "You",
  "transcript.speakerSystem": "System",
```

The other 5 of the 32 (`settings.advanced.systemAudio.label`,
`settings.advanced.systemAudioDevice.title`, `settings.dictation.enable.label`,
`settings.dictation.privacy.saveRecordings.label`,
`settings.dictation.privacy.saveTranscripts.label`) already have an entry in
`FORK_ONLY_STRINGS` — do not add or change them.

Run: `bun run test:unit`
Expected: still PASS. Adding entries to `FORK_ONLY_STRINGS` for keys that also
still exist (dead, unread by anything but the merge) in the raw locale files
changes nothing yet, because the merge overlay already wins.

- [ ] **Step 4: The dead `transcribe.name` duplicate needs no code change**

`settings.general.shortcut.bindings.transcribe.name` is already a
`FORK_ONLY_STRINGS` entry ("Capture shortcut") that wins the merge regardless
of what the raw locale file says. Its raw value ("Capture Shortcut", capital
S, written into all 24 files) has had no effect on rendered output since the
day that entry was added. Nothing to change in `branding.ts` for it — Step 6
removes the raw duplicate along with everything else.

- [ ] **Step 5: The one exception — `transcribe.description`**

`settings.general.shortcut.bindings.transcribe.description` is genuinely live
today, in `en` only: the raw file says "The keyboard shortcut to record and
transcribe a meeting or note," upstream says "...your voice," and nothing in
`FORK_ONLY_STRINGS` covers it. It cannot be fixed inside this task without
breaking this task's own hash-neutrality promise:

- Adding it to `FORK_ONLY_STRINGS` now would apply it to *every* locale — the
  locale-independent merge this whole plan exists to stop doesn't get gated to
  `en` until Task 4. Doing it here would recreate, for a 34th key, the exact
  defect Task 4 exists to fix for the other 43.
- Leaving the raw file's drifted value in place fails this task's own
  completion gate (Step 8).

So: Step 6 restores `en`'s raw value to upstream's ("...your voice"), same as
every other locale already has. This is a real, one-line, one-locale content
change — `en` will briefly show upstream's stock wording instead of the
fork's improved copy. **Task 4, Step 3** adds
`settings.general.shortcut.bindings.transcribe.description` to
`english-copy.json` with the fork's wording, which — once Task 4's `en`-only
gate exists — restores it without leaking it to the other 23 locales, the way
adding it here would.

Net effect across Tasks 2 and 4 together: `en`'s final rendered value for this
key is unchanged from before either task ran. Only this task's own golden
hash for `en` moves, only by this one field, and only until Task 4 puts it
back. Add a one-line comment to that effect at the top of
`src/shorthand/branding.test.ts` so a reader mid-plan does not mistake it for
a mistake:

```ts
// NOTE (temporary, removed by Task 4): the `en` golden hash changes in Task 2
// by exactly one field — settings.general.shortcut.bindings.transcribe.description
// reverts to upstream's wording because Task 2 cannot gate a fix to `en` alone
// yet. Task 4 restores the fork's wording via english-copy.json and this note
// goes with it.
```

- [ ] **Step 6: Remove the drift from all 24 locale files**

Run: `bun scripts/check-locale-drift.ts --fix`

This deletes, from every locale file, every one of the 32 Direction-B1 keys
(pruning any parent object — such as the whole `settings.dictation` object —
that becomes empty as a result). It does **not** touch `transcribe.name` or
`.description`, because those exist upstream too and this script only removes
keys absent upstream (Step 1's design choice). Handle those two by hand:

- In **every** locale's `translation.json`, restore
  `settings.general.shortcut.bindings.transcribe.name` to whatever
  `git show upstream/main:src/i18n/locales/<lang>/translation.json` has for
  that key (each locale's own translated shortcut name — not English).
- In **`en/translation.json` only**, restore
  `settings.general.shortcut.bindings.transcribe.description` to upstream's
  "The keyboard shortcut to record and transcribe your voice." Leave every
  other locale's `.description` alone — none of the other 23 were touched by
  the original drift, so they already match upstream.

Do **not** touch `src/i18n/locales/tr/translation.json`'s other 8 differing
keys (`acceleration.transcribe.*`, `overlay.style.*`, `overlay.position.*`,
`about.acknowledgments.ggml.*`). Those are not part of this task's scope — see
"Deliberately not in this plan."

- [ ] **Step 7: Confirm the fix, and add the non-blocking value-drift notice**

Run: `bun scripts/check-locale-drift.ts`
Expected: PASS — `✓ 24 locale file(s) match upstream/main exactly.` This
script only checks key presence (Step 1), so it does not see — and must not
be made to fail on — the Turkish value drift.

Run: `git diff upstream/main -- src/i18n/locales`
Expected: **empty for every file except `tr/translation.json`**, which still
shows exactly the 8 pre-existing lines documented in "The audit that
motivates this plan," Direction B2. If any other file shows a difference,
something in Step 6 was applied to the wrong locale or the wrong key —
investigate before continuing.

This distinction — a hard-failing check for content absent upstream, and a
merely-visible `git diff` for translated-value differences on shared keys — is
deliberate, not a gap. A future contributor should be free to improve a
translation's wording directly in these files (that is normal per
`CONTRIBUTING_TRANSLATIONS.md`); this task's gate exists to stop fork content
being smuggled in as new keys, not to freeze every translated value forever.

- [ ] **Step 8: Regenerate the golden hash for `en`, and only `en`**

Run: `bun run test:unit`
Expected: FAIL, on exactly one test — `en renders byte-identically to the
golden hash` — because of Step 5's named exception. Every other locale's
hash, and every other `en` key, is unaffected.

Run: `bun run golden:branding && git diff src/shorthand/branding.golden.json`
Expected: exactly one line changes, the `en` hash. Confirm by spot check:

```bash
bun -e '
import fs from "fs";
import { applyBranding } from "./src/shorthand/branding";
const raw = JSON.parse(fs.readFileSync("src/i18n/locales/en/translation.json", "utf8"));
const { translation } = applyBranding(raw, "en");
console.log(translation.settings.general.shortcut.bindings.transcribe.description);
'
```

Expected: `The keyboard shortcut to record and transcribe your voice.` — the
temporary regression from Step 5, confirmed and about to be fixed forward in
Task 4.

Run: `bun run test:unit`
Expected: PASS, all 25 tests.

- [ ] **Step 9: Add the script to package.json**

Insert into `"scripts"` immediately after the existing `"check:branding"`
line (before `"check:locale-drift"` is not yet a thing to be before — this is
the line that creates it). Change nothing else in the file.

```json
    "check:locale-drift": "bun scripts/check-locale-drift.ts",
```

- [ ] **Step 10: Run every gate**

Run: `bun run test:unit && bun run check:locale-drift && bun run check:translations && bun run check:branding && bun run lint && bun run build`
Expected: all pass. `check:translations` must still pass — the removal was
uniform across all 24 locale files including `en`, so key parity between them
is unaffected.

- [ ] **Step 11: Commit**

```bash
git add src/shorthand/branding.ts src/i18n/locales scripts/check-locale-drift.ts package.json src/shorthand/branding.golden.json src/shorthand/branding.test.ts
git commit -m "fix: restore byte-identity between src/i18n/locales and upstream

An audit found 32 fork-only keys (dictation mode, system audio capture)
written directly into all 24 locale files instead of through
FORK_ONLY_STRINGS -- 1568 lines of exactly the defect this fork's
locale-file policy exists to prevent. 27 had no covering entry and
rendered untranslated English in every language; 5 were already
overridden and just dead weight. Also restores a value edit to
settings.general.shortcut.bindings.transcribe.{name,description} found
the same way.

check:locale-drift makes this a permanent, automated gate rather than
something that has to be noticed by reading a diff.

transcribe.description keeps its upstream wording in en for one commit;
Task 4 restores the fork's wording through english-copy.json, which by
then can apply to en without leaking to the other 23 locales."
```

---

### Task 3: Split the strings into two files

Pure reorganisation. Both files are still merged into every locale, so
nothing rendered changes and the golden test stays fully green (Task 2's
named exception is already resolved by the time this task starts — Task 2's
own golden-hash regeneration in its Step 8 covers it). Task 4 is what changes
behaviour.

**Files:**
- Create: `src/shorthand/locales/en.json` (65 translatable fork strings)
- Create: `src/shorthand/english-copy.json` (43 English casing preferences)
- Create: `scripts/audit-fork-strings.ts`
- Modify: `src/shorthand/branding.ts` (the `FORK_ONLY_STRINGS` object literal — 108 entries after Task 2, so do not rely on a specific line range; find it by name)

**Interfaces:**
- Consumes: nothing new.
- Produces: `FORK_ONLY_STRINGS` remains exported as `Record<string, string>`
  and remains the union of both files, so `scripts/check-branding.ts` keeps
  working unmodified. Task 4 changes how the two halves are applied, not this
  export's type.

- [ ] **Step 1: Write the audit script**

The split must be derived, not hand-sorted. Create `scripts/audit-fork-strings.ts`:

```ts
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
```

- [ ] **Step 2: Run the audit and confirm the counts**

Run: `bun scripts/audit-fork-strings.ts`
Expected: `forkOnly: 62`, `caseOnly: 43`, `brandOnly: 0`, `semantic: 3`,
totalling 108 — the original 81 plus the 27 migrated by Task 2, all of which
are absent upstream and so land in `forkOnly`.

If the counts differ, **stop and report** rather than proceeding. A different
split means `FORK_ONLY_STRINGS` changed since this plan was written (or Task
2 was not completed first), and the two output files would be wrong.

- [ ] **Step 3: Generate both files from the classification**

Do not hand-copy over a hundred string pairs — the values contain apostrophes,
em-dashes and embedded quotes, and transcription errors in quoted prose are
exactly what review misses.

Run from the repo root:

```bash
bun -e '
import { FORK_ONLY_STRINGS } from "./src/shorthand/branding";
import { classification } from "./scripts/audit-fork-strings";
import fs from "fs";
const pick = (keys) => Object.fromEntries(keys.map((k) => [k, FORK_ONLY_STRINGS[k]]));
fs.mkdirSync("src/shorthand/locales", { recursive: true });
fs.writeFileSync(
  "src/shorthand/locales/en.json",
  JSON.stringify(pick([...classification.forkOnly, ...classification.semantic].sort()), null, 2) + "\n",
);
fs.writeFileSync(
  "src/shorthand/english-copy.json",
  JSON.stringify(pick([...classification.caseOnly, ...classification.brandOnly].sort()), null, 2) + "\n",
);
console.log("locales/en.json: " + ([...classification.forkOnly, ...classification.semantic].length));
console.log("english-copy.json: " + ([...classification.caseOnly, ...classification.brandOnly].length));
'
```

Expected: `locales/en.json: 65`, `english-copy.json: 43`.

Verify the union round-trips:

```bash
bun -e '
import { FORK_ONLY_STRINGS } from "./src/shorthand/branding";
import fork from "./src/shorthand/locales/en.json";
import copy from "./src/shorthand/english-copy.json";
const merged = { ...fork, ...copy };
const a = JSON.stringify(FORK_ONLY_STRINGS, Object.keys(FORK_ONLY_STRINGS).sort());
const b = JSON.stringify(merged, Object.keys(FORK_ONLY_STRINGS).sort());
const overlap = Object.keys(fork).filter((k) => k in copy);
console.log(a === b ? "identical" : "MISMATCH");
console.log("overlapping keys (must be 0): " + overlap.length);
'
```

Expected: `identical`, `overlapping keys (must be 0): 0`.

(The replacer-array form is safe here because both objects are flat.)

- [ ] **Step 4: Replace the inline object with imports**

In `src/shorthand/branding.ts`, delete the `FORK_ONLY_STRINGS` object literal
and put this in its place. The policy comments explaining *why* stay in this
file — JSON cannot hold them.

```ts
import forkEn from "./locales/en.json";
import englishCopy from "./english-copy.json";

/**
 * Strings that exist only in this fork, in `./locales/*.json` so they can be
 * translated. Contributors add a language exactly as they would upstream —
 * see `./locales/README.md`.
 *
 * Separate from `english-copy.json` on purpose. An audit against upstream's
 * catalogue found 43 of the original 81 entries here differed from upstream
 * only in English capitalisation, and because this merge is locale-independent
 * each one replaced 23 real translations with an English string. Those live in
 * `english-copy.json` now and reach `en` alone. A second audit found 27 more
 * fork-only keys that had bypassed this file entirely, written straight into
 * the locale files instead (`docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md`,
 * Task 2) — those are folded in here too.
 *
 * Flat dotted keys; `setByPath` expands them. i18next accepts either shape.
 */
const FORK_STRINGS: Record<string, string> = forkEn;

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
  ...FORK_STRINGS,
  ...ENGLISH_COPY,
};
```

- [ ] **Step 5: Verify nothing rendered changed**

Run: `bun run test:unit`
Expected: PASS, all 25 tests. `applyBranding` still merges `FORK_ONLY_STRINGS`, which is still the same union.

Run: `bun run check:branding && bun run check:translations && bun run check:locale-drift && bun run lint && bun run build`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/shorthand/locales/en.json src/shorthand/english-copy.json scripts/audit-fork-strings.ts src/shorthand/branding.ts
git commit -m "refactor: separate translatable fork strings from English copy rules

Derived, not curated: audit-fork-strings.ts classifies each key against
upstream's catalogue. 65 are genuinely ours (38 from the original audit,
27 migrated out of the locale files by Task 2); 43 differ from upstream
only in English capitalisation. Behaviour is unchanged in this commit —
the golden hashes still match. Task 4 is what stops the 43 reaching other
locales."
```

---

### Task 4: Stop English copy rules reaching other locales

The behaviour change. 23 locales get their translations back, and one
shortcut description gets its fork wording back — in English only this time,
which is the fix, not a regression.

**Files:**
- Modify: `src/shorthand/branding.ts` (merge `ENGLISH_COPY` only for `en`)
- Modify: `src/shorthand/english-copy.json` (fix the "Handy shortcuts" bug; add `transcribe.description`)
- Modify: `src/shorthand/branding.test.ts` (assert the restoration; remove Task 2's temporary-exception note)
- Modify: `src/shorthand/branding.golden.json` (regenerate deliberately)

**Interfaces:**
- Consumes: `FORK_STRINGS` and `ENGLISH_COPY` from Task 3.
- Produces: no new export. `applyBranding` behaviour changes: for `locale !== "en"`, only `FORK_STRINGS` is merged.

- [ ] **Step 1: Write the failing tests**

Append to `src/shorthand/branding.test.ts`:

```ts
import fs from "fs";
import path from "path";
import { applyBranding } from "./branding";
import englishCopy from "./english-copy.json";

const read = (locale: string) =>
  JSON.parse(
    fs.readFileSync(
      path.join(
        import.meta.dir,
        "..",
        "i18n",
        "locales",
        locale,
        "translation.json",
      ),
      "utf8",
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

describe("English copy rules", () => {
  test("apply to English", () => {
    const { translation } = applyBranding(read("en"), "en");
    expect(get(translation, "theme.title")).toBe("Application theme");
  });

  test("do not overwrite a German translation", () => {
    const { translation } = applyBranding(read("de"), "de");
    expect(get(translation, "theme.title")).toBe("Anwendungsdesign");
    expect(get(translation, "appLanguage.title")).toBe("Anwendungssprache");
  });

  test("no locale but en receives any english-copy value", () => {
    for (const locale of localeNames()) {
      if (locale === "en") continue;
      const { translation } = applyBranding(read(locale), locale);
      for (const [key, englishValue] of Object.entries(englishCopy)) {
        const rendered = get(translation, key);
        const upstreamHadIt = get(read(locale), key) !== undefined;
        if (upstreamHadIt && rendered === englishValue) {
          throw new Error(
            `${locale}:${key} was overwritten with the English "${englishValue}"`,
          );
        }
      }
    }
  });

  test("no english-copy value reintroduces the upstream brand name", () => {
    for (const value of Object.values(englishCopy)) {
      expect(value).not.toMatch(/\bHandy\b/);
    }
  });

  test("the transcribe shortcut's fork description is restored for en only", () => {
    const en = applyBranding(read("en"), "en").translation;
    expect(
      get(en, "settings.general.shortcut.bindings.transcribe.description"),
    ).toBe("The keyboard shortcut to record and transcribe a meeting or note.");

    const de = applyBranding(read("de"), "de").translation;
    expect(
      get(de, "settings.general.shortcut.bindings.transcribe.description"),
    ).not.toBe("The keyboard shortcut to record and transcribe a meeting or note.");
  });
});
```

Remove the "NOTE (temporary, removed by Task 4)" comment Task 2 added at the
top of this file — its job is done once this task's changes land.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `bun run test:unit`
Expected: FAIL on `do not overwrite a German translation` (renders the English
"Application theme"), on `no locale but en receives any english-copy value`,
on the brand test (`settings.general.shortcut.title` is `"Handy shortcuts"`),
and on the new `transcribe` description test (`en` still shows upstream's
"...your voice" from Task 2's deliberate, temporary revert).

- [ ] **Step 3: Fix the brand bug, and restore the shortcut description**

In `src/shorthand/english-copy.json`, change:

```json
  "settings.general.shortcut.title": "Handy shortcuts",
```

to:

```json
  "settings.general.shortcut.title": "Shorthand shortcuts",
```

Upstream's value is "Handy Shortcuts"; substitution alone would render
"Shorthand Shortcuts", and the sentence-case rule makes it "Shorthand
shortcuts". The old value bypassed substitution and put the upstream brand
name back into the UI.

In the same file, add the entry Task 2 deferred:

```json
  "settings.general.shortcut.bindings.transcribe.description": "The keyboard shortcut to record and transcribe a meeting or note.",
```

This is a wording change, not a casing one, but it belongs here rather than in
`locales/en.json`: unlike the sibling `.name` field (a semantic rename,
intentionally shown in English until someone translates it, in every locale),
this description was never drifted anywhere but `en` — every other locale
still carries its own upstream-translated text, undisturbed, and should keep
it rather than being switched to English. `english-copy.json`'s "apply to
`en` only, leave everyone else exactly as they are" mechanics is what that
requires, even though its existing entries are all casing preferences.

- [ ] **Step 4: Gate the English-copy merge on locale**

In `src/shorthand/branding.ts`, change the merge loop at the end of `applyBranding` from:

```ts
  for (const [path, value] of Object.entries(FORK_ONLY_STRINGS)) {
    setByPath(rebranded, path, value);
  }
```

to:

```ts
  // Fork strings are content and belong in every locale; English copy rules
  // are an English typographic convention (or, in one case, a wording choice
  // not yet ported to other locales) and belong only in English. Merging
  // the latter everywhere is what replaced 23 locales' translations with
  // English strings.
  const overlay =
    locale === "en" ? { ...FORK_STRINGS, ...ENGLISH_COPY } : FORK_STRINGS;

  for (const [path, value] of Object.entries(overlay)) {
    setByPath(rebranded, path, value);
  }
```

- [ ] **Step 5: Run the tests**

Run: `bun run test:unit`
Expected: the five new/changed tests PASS. The 23 non-English golden hash
tests now FAIL — that is correct and expected; those locales render
differently now, which is the entire point of this task. `en` must **also**
fail, but only because Step 3 restores the fork wording Task 2 had to
temporarily revert — confirm with the spot check in Step 6 that this is the
only thing that moved for `en`.

If any other `en` key differs from Task 2's golden baseline, something beyond
the intended changes moved — investigate before continuing.

- [ ] **Step 6: Regenerate the golden file and review the diff**

Run: `bun run golden:branding && git diff src/shorthand/branding.golden.json`
Expected: 24 hashes change — the 23 non-English locales (the intended fix)
and `en`'s (reverting Task 2's Step 5 temporary regression on
`transcribe.description` back to the fork's wording; every other `en` key is
unchanged, so this is `en`'s hash returning to what it was *before Task 2
ever ran*, not a new departure).

Confirm the change is the intended one by spot-checking a locale:

```bash
bun -e '
import fs from "fs";
import { applyBranding } from "./src/shorthand/branding";
const raw = JSON.parse(fs.readFileSync("src/i18n/locales/ja/translation.json", "utf8"));
const { translation } = applyBranding(raw, "ja");
for (const k of ["theme.title", "appLanguage.title", "settings.models.title"])
  console.log(k + ": " + JSON.stringify(k.split(".").reduce((a, p) => a?.[p], translation)));
'
```

Expected: Japanese strings, not English ones.

- [ ] **Step 7: Run every gate**

Run: `bun run test:unit && bun run check:branding && bun run check:locale-drift && bun run check:translations && bun run lint && bun run build`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src/shorthand/branding.ts src/shorthand/english-copy.json src/shorthand/branding.test.ts src/shorthand/branding.golden.json
git commit -m "fix: stop English copy rules overwriting 23 locales' translations

43 fork entries differed from upstream only in English capitalisation, and
the merge was locale-independent, so a German user saw 'Application
language' where 'Anwendungssprache' already existed. Sentence case is an
English convention; German capitalises every noun.

Also fixes settings.general.shortcut.title, which read 'Handy shortcuts':
fork strings bypass brand substitution, so the override was putting the
upstream name back into the Shorthand UI.

And restores settings.general.shortcut.bindings.transcribe.description's
fork wording for en, gated correctly this time -- Task 2 had to revert it
to upstream's text temporarily because no en-only mechanism existed until
this commit."
```

---

### Task 5: Make the fork catalogue locale-aware

Lets a contributor add `de.json` and have it used. Behaviour is unchanged
until one exists.

**Files:**
- Modify: `src/shorthand/branding.ts` (add `forkStringsFor`)
- Modify: `src/shorthand/branding.test.ts`

**Interfaces:**
- Consumes: `src/shorthand/locales/en.json`.
- Produces: `forkStringsFor(locale: string): Record<string, string>` — the
  English catalogue with any same-named locale catalogue layered on top. Task
  6's gate does **not** use it.

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

- [ ] **Step 2: Run to verify failure**

Run: `bun run test:unit`
Expected: FAIL — `forkStringsFor` is not exported.

- [ ] **Step 3: Implement**

In `src/shorthand/branding.ts`, replace `const FORK_STRINGS: Record<string, string> = forkEn;` with:

```ts
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

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
const FORK_CATALOGUES: Record<string, Record<string, string>> =
  Object.fromEntries(
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
```

Remove the now-unused `import forkEn from "./locales/en.json";`, and update the two remaining references to `FORK_STRINGS`:

- in `FORK_ONLY_STRINGS`, use `...forkStringsFor("en")`
- in `applyBranding`'s overlay, use `forkStringsFor(locale)`:

```ts
  const forkStrings = forkStringsFor(locale);
  const overlay =
    locale === "en" ? { ...forkStrings, ...ENGLISH_COPY } : forkStrings;
```

- [ ] **Step 4: Run every gate**

Run: `bun run test:unit && bun run check:branding && bun run build`
Expected: all pass, golden hashes unchanged — with only `en.json` present, `forkStringsFor` returns exactly what `forkEn` did.

If `check:branding` fails with an `fs` or path error, the module is being loaded from an unexpected working directory; fix the path resolution rather than adding a fallback that silently returns `{}`.

- [ ] **Step 5: Commit**

```bash
git add src/shorthand/branding.ts src/shorthand/branding.test.ts
git commit -m "feat: give fork strings a locale dimension

English is the merge base rather than a lookup-time fallback, so a
partially-translated catalogue renders English for its untranslated keys
instead of a raw key path."
```

---

### Task 6: Key-parity gate for fork catalogues

Without it, a `de.json` missing a key falls back to English silently and nobody finds out.

**Files:**
- Create: `scripts/check-fork-translations.ts`
- Modify: `package.json`

**Interfaces:**
- Consumes: raw files in `src/shorthand/locales/`. Deliberately **not** `forkStringsFor`, which merges English in and would report every catalogue as complete.
- Produces: `bun run check:fork-translations`, exiting non-zero on any mismatch.

- [ ] **Step 1: Write the gate**

Create `scripts/check-fork-translations.ts`:

```ts
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
```

- [ ] **Step 2: Add the script to package.json**

Insert immediately after the `check:locale-drift` line (added by Task 2),
before `check:settings`. Changing nothing else keeps every task's insertion
anchored to a line that already exists by the time that task runs — Task 1
anchors to `check:settings`, Task 2 anchors to `check:branding`, this task
anchors to `check:locale-drift`, and none of the three collide.

```json
    "check:fork-translations": "bun scripts/check-fork-translations.ts",
```

- [ ] **Step 3: Run it**

Run: `bun run check:fork-translations`
Expected: PASS — `✓ 0 fork translation(s) match en (65 keys).` Only
`en.json` exists, so there is nothing to compare. Step 4 proves it is not
merely inert.

- [ ] **Step 4: Prove the gate catches a gap**

Create a deliberately incomplete `src/shorthand/locales/de.json`:

```json
{
  "sidebar.modes": "Modi"
}
```

Run: `bun run check:fork-translations`
Expected: FAIL, exit 1, reporting `de` missing 64 keys.

Confirm the merge still renders correctly with a partial catalogue:

Run: `bun run test:unit`
Expected: PASS — `de` picks up "Modi" for `sidebar.modes` and English for the rest. The golden hash for `de` will now fail; that is expected with a fixture in place.

Delete the fixture and re-run:

```bash
rm src/shorthand/locales/de.json
```

Run: `bun run check:fork-translations && bun run test:unit`
Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/check-fork-translations.ts package.json
git commit -m "feat: key-parity gate for fork translation catalogues

check:translations covers upstream's catalogues and structurally cannot
see these. A missing key falls back to English silently, so this fails
rather than warns."
```

---

### Task 7: Document the process for contributors

Translation stays upstream's process — fork, copy, translate, PR. The docs
have to say where the fork's files are, when *not* to add one, and — new in
this plan — that the byte-identity guarantee is real now, not aspirational.

**Files:**
- Create: `src/shorthand/locales/README.md`
- Modify: `AGENTS.md` (§ Internationalization)
- Modify: `CONTRIBUTING_TRANSLATIONS.md`
- Modify: `BRANDING.md`
- Modify: `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md`

- [ ] **Step 1: Write the contributor guide**

Create `src/shorthand/locales/README.md`:

```markdown
# Fork-only translation catalogues

Strings that exist only in Shorthand, not in upstream Handy.

Upstream's catalogues live in `src/i18n/locales/<lang>/translation.json`.
They are supposed to stay **byte-identical to upstream** so
`git merge upstream/main` never conflicts on them — and, as of this file
existing, a permanent gate (`bun run check:locale-drift`) checks that on every
commit. That gate did not always exist: an earlier feature branch wrote 32
fork-only keys directly into all 24 of those files before anyone noticed, and
nothing caught it until this catalogue split was built. Fork strings cannot go
there. They live here instead and are merged into the bundle at build time by
`src/shorthand/vite-branding-plugin.ts` — after the Handy→Shorthand
substitution, which is why a string here may say "Handy" and mean it.

## Adding a language

The process is upstream's, from [CONTRIBUTING_TRANSLATIONS.md](../../../CONTRIBUTING_TRANSLATIONS.md):

1. Copy `en.json` to `<lang>.json`, matching a locale directory name under
   `src/i18n/locales/`.
2. Translate the values. Leave the keys alone.
3. Run `bun run check:fork-translations`.
4. Open a pull request.

Translate this file **and** upstream's `src/i18n/locales/<lang>/translation.json`
— together they are the whole UI.

Every key in `en.json` must be present. An untranslated key renders in English
rather than failing, but the gate still requires it: silent English in an
otherwise translated UI is a bug nobody reports.

## Adding a *new* string — read this first

Before adding a key here, check whether upstream already has it:

```bash
bun scripts/audit-fork-strings.ts
```

If upstream has the same string and you only dislike its wording or
capitalisation, **do not add it here.** A fork string overrides that key in
every language, so an English preference silently replaces real translations
in all 23 of them. That happened to 43 keys before this directory existed.

- Purely an English capitalisation preference → `../english-copy.json`, which
  reaches English only.
- Genuinely new, or the fork's own terminology → here, and it needs
  translating like anything else.

**Never add a fork-only string directly to a file under `src/i18n/locales/`.**
That is the mistake `bun run check:locale-drift` exists to catch — it fails
the build on any key present there that upstream does not have. It happened
once, for 32 keys across all 24 locales, before the check existed.

Keys are flat and dotted (`"settings.modes.heading"`), unlike upstream's
nested catalogues. Both are valid i18next.
```

- [ ] **Step 2: Correct the i18n section in AGENTS.md**

`AGENTS.md` § Internationalization says to add new keys to `src/i18n/locales/en/translation.json`. For a fork-only string that is wrong and reintroduces the conflict surface the mechanism exists to avoid. Replace the "Adding new text" block with:

```markdown
**Adding new text:**

Which file depends on what kind of string it is.

- **A string upstream also has** — leave upstream's alone. Its translations
  already exist in 23 languages, and a fork override replaces them all with
  English. Run `bun scripts/audit-fork-strings.ts` if unsure.
- **An English capitalisation preference** — `src/shorthand/english-copy.json`.
  Applies to English only; sentence case is an English convention.
- **A genuinely fork-only string** — `src/shorthand/locales/en.json`. See
  [`src/shorthand/locales/README.md`](src/shorthand/locales/README.md). Never
  `src/i18n/locales/` directly — `bun run check:locale-drift` fails the build
  if a fork-only key ends up there, which happened once, silently, for 32
  keys across all 24 locales, before that gate existed.
- **A string being contributed upstream** —
  `src/i18n/locales/en/translation.json`, per
  [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

Then use it: `const { t } = useTranslation(); t('key.path')`

Gates: `bun run check:translations` (upstream's catalogues),
`bun run check:locale-drift` (fork content must not be among them),
`bun run check:fork-translations` (the fork's own catalogues),
`bun run check:branding` (the rename).
```

- [ ] **Step 3: Point translators at both files**

In `CONTRIBUTING_TRANSLATIONS.md`, add after the "File Structure" section:

```markdown
## Fork-only strings

Shorthand adds screens upstream Handy does not have. Their strings live in
`src/shorthand/locales/<lang>.json`, separately from the catalogues above, so
that upstream's files stay byte-identical and merges never conflict on them.

A complete translation covers both files. See
[`src/shorthand/locales/README.md`](src/shorthand/locales/README.md).
```

Also correct the stale "Currently Supported Languages" table, which lists 7 languages as complete while 24 locale directories exist.

- [ ] **Step 4: Update BRANDING.md**

Add after its description of `FORK_ONLY_STRINGS`:

```markdown
As of 2026-08-26 the strings are in `src/shorthand/locales/*.json` (translatable
fork content) and `src/shorthand/english-copy.json` (English casing rules,
merged into `en` only). `FORK_ONLY_STRINGS` remains exported as the union, for
`check-branding.ts`'s locale-independent question "is this key deliberately
ours?". Merge order is unchanged: substitution first, fork strings on top.

The same 2026-08-26 plan also found, and fixed, 32 fork-only keys that had
been written directly into `src/i18n/locales/` instead of through this
mechanism — the exact thing this file's "never edit the locale files" rule
exists to prevent. `bun run check:locale-drift` now fails the build if that
happens again.
```

- [ ] **Step 5: Mark the phase done in the spec**

In `docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md`, update the Phase 0b entry to reference this plan and record that Decision 2a is implemented, noting the 43-key regression it uncovered and fixed, and the separate 32-key byte-identity regression uncovered while implementing it.

- [ ] **Step 6: Note the knock-on for the small-fixes plan**

`docs/superpowers/plans/2026-08-26-shorthand-small-ui-fixes.md`, Task 1, drops
the `t()` call for `settings.advanced.systemAudioDevice.default` on the
reasoning that the key renders as the literal string `"Default"` in all 24
locales today, so routing it through translation would be a no-op. That
reasoning holds only *because* the key is presently untranslatable drift —
after this plan's Task 2, `settings.advanced.systemAudioDevice.default` lives
in `src/shorthand/locales/en.json` and genuinely can be translated per
locale. Do not edit that plan from here; leave a note in this plan's own
record (this step) that the small-fixes plan's Task 1 decision should be
revisited once this plan has landed, since the premise it relied on will no
longer be true.

- [ ] **Step 7: Verify the docs match reality**

Run every command the docs name:

Run: `bun scripts/audit-fork-strings.ts && bun run check:fork-translations && bun run check:translations && bun run check:locale-drift && bun run check:branding && bun run test:unit`
Expected: all pass. `check:locale-drift` passing here means the `git diff
upstream/main -- src/i18n/locales` gap is now empty except for the
pre-existing, out-of-scope Turkish drift documented in "The audit that
motivates this plan" and "Deliberately not in this plan" — confirm that
residual is still exactly those 8 lines and nothing more.

- [ ] **Step 8: Commit**

```bash
git add src/shorthand/locales/README.md AGENTS.md CONTRIBUTING_TRANSLATIONS.md BRANDING.md docs/superpowers/specs/2026-08-25-shorthand-umbrella-design.md docs/superpowers/plans/2026-08-26-fork-only-translation-catalogues.md
git commit -m "docs: say where fork strings live and when not to add one

AGENTS.md still sent people to src/i18n/locales/en/translation.json, and
nothing warned that overriding an upstream key replaces its translations in
every language, or that fork-only content could end up in those files at
all -- both mistakes this plan found already shipped, at 43 keys and 32
keys respectively."
```

---

## Done when

- 23 locales render their own translations for the 43 keys that previously showed English, verified by test rather than by inspection.
- `settings.general.shortcut.title` no longer says "Handy" in the Shorthand UI.
- `settings.general.shortcut.bindings.transcribe.description` reads the fork's wording in `en` and each other locale's own upstream translation everywhere else.
- Adding `de.json` is the whole cost of contributing German fork strings — no registration step.
- `bun run check:fork-translations` fails on an incomplete catalogue, and has been observed failing.
- `bun run check:locale-drift` fails on a key written directly into `src/i18n/locales/`, and has been observed failing (Task 2, Step 2).
- `git diff upstream/main -- src/i18n/locales` shows nothing except the pre-existing, out-of-scope Turkish translation lines documented above — not 1568 lines of fork content, and not zero, until that separate issue is resolved on its own.
- `bun scripts/audit-fork-strings.ts` reports `caseOnly: 0` when run against `locales/en.json`.
- `AGENTS.md` tells the next person which of the three files to use, and that `src/i18n/locales/` is never one of them.

## Deliberately not in this plan

- **Translating anything.** This builds the mechanism and fixes the regression. Contributions arrive by PR, upstream's process, unchanged.
- **A translation platform.** Weblate and Crowdin offer free open-source hosting and would fit this file layout, but they solve coordinating volunteers, and there are none yet. Revisit when there are.
- **Machine translation.** Explicitly out — the process is upstream's.
- **Nesting the keys** to match upstream's shape. Flat is what these strings already used; keeping it made the split reviewable.
- **Fixing the pre-existing Turkish translation drift** found while writing Task 2 — 8 keys under `settings.advanced.acceleration.transcribe.*`, `overlay.style.*`, `overlay.position.*` and `about.acknowledgments.ggml.*`, whose Turkish wording differs from upstream's, apparently stale since an upstream key rename. Both values are Turkish; this is not fork content bypassing the mechanism, it is a translation that may or may not need catching up, and deciding which is a translation-quality call this plan does not make. File it as its own follow-up.
- **Revisiting the small-fixes plan's `systemAudioDevice.default` decision.** Flagged in Task 7, Step 6, for whoever picks that plan up next — not resolved here.
- **The umbrella installer** (spec Phases 0, 1, 2, 3).
