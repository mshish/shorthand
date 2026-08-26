/**
 * Fork-only. Fails if any settings control has become unreachable.
 *
 * The redesign stopped registering upstream's General, Models, Advanced and
 * Post-processing screens: their rows live in the fork's own sections now,
 * either visible by default or behind the Advanced switch. That is only safe if
 * every row genuinely made the move, and "I checked the list twice" is not a
 * guarantee — the previous revision of the design silently dropped two settings
 * (`KeyboardImplementationSelector` and `LazyStreamClose`) and nobody noticed
 * until a reviewer diffed the inventory by hand.
 *
 * So: walk the import graph from the registered sections, and assert every
 * settings component is reachable from it.
 *
 * A static check rather than a Playwright spec, deliberately. The question
 * — "is this file reachable from a registered section?" — is answerable from
 * the source, and answering it in a browser would mean rendering every section
 * against a Tauri backend that does not exist in CI. This follows the same
 * shape as `check-branding.ts` and `check-translations.ts`.
 *
 * Two limits, both found by trying to make the check fail on purpose rather
 * than by reasoning about it — which is the only way to learn that a check
 * cannot fail:
 *
 *   1. It proves reachability, not that a row renders. A component behind an
 *      `enabled` predicate that is never true would still pass.
 *   2. Debug is a registered section, so anything the Debug screen renders is
 *      reachable through it. Deleting `AlwaysOnMicrophone` from Audio does not
 *      trip this check, because Debug renders it too. That is correct — the
 *      setting really is still reachable — but it means this cannot detect a
 *      row quietly demoted from the main tree to debug-only.
 *
 * Verified to actually fail: removing `SystemAudioCapture` from AudioSettings,
 * which nothing else renders, is reported. A check that cannot fail is worth
 * nothing, so if you change the resolution logic, re-run that experiment.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, dirname, resolve, relative } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const SRC = join(ROOT, "src");

/** Where the walk starts: everything the sidebar can render. */
const ENTRY_POINTS = [
  join(SRC, "shorthand/sections.ts"),
  join(SRC, "components/Sidebar.tsx"),
];

/**
 * Directories that contain settings screens or controls, all of which must be
 * reachable. `src/shorthand` also contains brand assets, settings-only UI
 * primitives, and superseded section containers, so treating every TSX file in
 * that tree as a settings component produces false positives when one of those
 * non-setting components is unused.
 */
const SETTINGS_COMPONENT_DIRS = [
  join(SRC, "components/settings"),
  join(SRC, "shorthand/settings"),
  join(SRC, "shorthand/dictation"),
  join(SRC, "shorthand/assisted-notes"),
];

/** All source directories whose settings translation keys must resolve. */
const SETTINGS_SOURCE_DIRS = [
  join(SRC, "components/settings"),
  join(SRC, "shorthand"),
];

/**
 * Files that deliberately render nowhere. Every entry needs a reason, and the
 * reason has to be one of three kinds — a composite screen the fork replaced, a
 * pure re-export, or dead code. "It seemed fine" is not a category.
 *
 * Keeping these files rather than deleting them is deliberate: deleting a file
 * upstream still maintains turns every future edit to it into a delete/modify
 * conflict, which is the expensive kind.
 */
const ALLOWED_UNREACHABLE: Record<string, string> = {
  // Composite upstream screens the fork replaced wholesale. Their rows are all
  // reachable; only these container components are not.
  "components/settings/general/GeneralSettings.tsx":
    "replaced by shorthand/settings/{Modes,Audio,App}Settings",
  "components/settings/advanced/AdvancedSettings.tsx":
    "replaced by the Advanced switch, which reveals these rows in place",
  "components/settings/about/AboutSettings.tsx":
    "replaced by shorthand/settings/AboutSettings",

  // `post-processing/PostProcessingSettings.tsx` is deliberately absent from
  // this list. The screen component in it is not rendered, but the file is
  // still reached: `PostProcessingSettingsApi` and `PostProcessingSettingsPrompts`
  // are defined inside it and re-exported, and AICleanupSettings renders both.
  // This check is file-level, so it reports the file as reachable — which is
  // true, and the honest limit of the granularity.

  // Renders both the per-mode overlay style and the shared overlay position,
  // which the redesign needs in two different sections. ui/OverlayRows supplies
  // them separately; rendering this as well would put a second input on screen
  // bound to the same shared field.
  "components/settings/ShowOverlay.tsx":
    "superseded by ui/OverlayRows, which splits style from the shared position",

  // Renders both overlay rows together; the fork needs them in two sections, so
  // ui/OverlayRows supplies them separately. Kept because it is the surface the
  // dictation design documented.
  "shorthand/dictation/DictationShowOverlay.tsx":
    "superseded by ui/OverlayRows, which splits style from the shared position",

  // Dead code, and dead before this redesign: defined, exported from nowhere,
  // rendered nowhere. It also hardcodes English strings and Windows-only
  // %APPDATA% paths, so making it reachable would be a regression, not a fix.
  "components/settings/debug/DebugPaths.tsx": "dead code, and was before this",
};

/** Not components: types, hooks, barrels, helpers. */
function isComponentFile(path: string): boolean {
  if (!path.endsWith(".tsx")) return false;
  const name = path.split(/[\\/]/).pop()!;
  return name !== "index.tsx";
}

function walkDir(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walkDir(full));
    else out.push(full);
  }
  return out;
}

/**
 * Captures the specifier clause as well as the module, because barrels make
 * naive import-following useless. `Sidebar.tsx` imports one name from
 * `./settings`, but that barrel re-exports every screen in the directory —
 * follow the barrel wholesale and `GeneralSettings`, `AdvancedSettings` and
 * `PostProcessingSettings` all look reachable, which is exactly the claim this
 * script exists to disprove.
 *
 * Group 1 is the clause (`{ A, B }`, `Foo`, `* as Ns`) or undefined for a
 * side-effect import; group 2 is the module specifier.
 */
const IMPORT_RE =
  /import\s+(?:type\s+)?(?:([\w*{}\s,]+?)\s+from\s+)?["']([^"']+)["']/g;

const BARREL_RE = /export\s*\{([^}]*)\}\s*from\s*["']([^"']+)["']/g;

/** `A`, `A as B`, `default as C` → the name as the *importer* sees it. */
function exportedNames(clause: string): string[] {
  return clause
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const as = part.split(/\s+as\s+/);
      return (as[1] ?? as[0]).trim();
    });
}

function resolveImport(spec: string, fromFile: string): string | null {
  let base: string;
  if (spec.startsWith("@/")) base = join(SRC, spec.slice(2));
  else if (spec.startsWith(".")) base = resolve(dirname(fromFile), spec);
  else return null; // a package, not ours

  for (const candidate of [
    base,
    `${base}.tsx`,
    `${base}.ts`,
    join(base, "index.tsx"),
    join(base, "index.ts"),
  ]) {
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {
      /* not this one */
    }
  }
  return null;
}

function isBarrel(path: string): boolean {
  return /[\\/]index\.tsx?$/.test(path);
}

/**
 * The modules a barrel re-exports the given names from.
 *
 * If the clause is a namespace import or the names cannot be matched, fall back
 * to the whole barrel — better to under-report a missing component than to
 * claim one is unreachable because the parser gave up.
 */
function throughBarrel(barrel: string, names: string[] | null): string[] {
  const source = readFileSync(barrel, "utf8");
  if (names === null) {
    return [...source.matchAll(BARREL_RE)]
      .map((m) => resolveImport(m[2], barrel))
      .filter((p): p is string => p !== null);
  }

  const wanted = new Set(names);
  const out: string[] = [];
  for (const match of source.matchAll(BARREL_RE)) {
    if (exportedNames(match[1]).some((name) => wanted.has(name))) {
      const target = resolveImport(match[2], barrel);
      if (target) out.push(target);
    }
  }
  return out;
}

/** Every local file reachable from the entry points, transitively. */
function reachableFrom(entries: string[]): Set<string> {
  const seen = new Set<string>();
  const queue = [...entries];
  while (queue.length) {
    const file = queue.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);

    let source: string;
    try {
      source = readFileSync(file, "utf8");
    } catch {
      continue;
    }

    for (const match of source.matchAll(IMPORT_RE)) {
      const target = resolveImport(match[2], file);
      if (!target) continue;

      if (isBarrel(target)) {
        const clause = match[1]?.trim();
        const names =
          clause && clause.startsWith("{")
            ? exportedNames(clause.slice(1, -1))
            : null;
        for (const onward of throughBarrel(target, names)) {
          if (!seen.has(onward)) queue.push(onward);
        }
        continue;
      }

      if (!seen.has(target)) queue.push(target);
    }
  }
  return seen;
}

const reachable = reachableFrom(ENTRY_POINTS);

const required = SETTINGS_COMPONENT_DIRS.flatMap(walkDir).filter(isComponentFile);

const missing: string[] = [];
const staleAllowances: string[] = [];

for (const file of required) {
  const rel = relative(SRC, file).replace(/\\/g, "/");
  const allowed = rel in ALLOWED_UNREACHABLE;

  if (reachable.has(file)) {
    // Reachable and allow-listed means the allowance has outlived its reason.
    if (allowed) staleAllowances.push(rel);
  } else if (!allowed) {
    missing.push(rel);
  }
}

/**
 * Every `t("…")` key the settings tree uses must actually resolve.
 *
 * Added after shipping one that did not. An edit to `FORK_ONLY_STRINGS` dropped
 * two entries, and the sidebar footer rendered the literal string
 * `settings.advanced.switch.label` to the user. Nothing failed: not tsc, not
 * ESLint, not `check:translations` — which compares key parity *between
 * locales* and so cannot see a key that is missing from all of them. It was
 * caught by looking at a screenshot, which is not a guarantee.
 *
 * Only literal keys are checked. A computed key (`t(\`x.${y}\`)`) is invisible
 * here, and that is a reason to prefer literals in this tree — shortcut row
 * labels are built from a binding id and so are not seen by this at all, which
 * is how two Title Case labels survived a sweep of every other one.
 */
const literalKeys = new Set<string>();
for (const file of SETTINGS_SOURCE_DIRS.flatMap(walkDir)) {
  if (!/\.tsx?$/.test(file)) continue;
  for (const match of readFileSync(file, "utf8").matchAll(
    /\bt\(\s*"([a-zA-Z0-9_.]+)"/g,
  )) {
    literalKeys.add(match[1]);
  }
}

const { applyBranding } = await import("../src/shorthand/branding");
const enCatalogue = JSON.parse(
  readFileSync(join(SRC, "i18n/locales/en/translation.json"), "utf8"),
);
const { translation } = applyBranding(enCatalogue, "en");

const unresolved = [...literalKeys]
  .filter((key) => {
    const value = key
      .split(".")
      .reduce<any>(
        (node, part) => (node == null ? node : node[part]),
        translation,
      );
    return typeof value !== "string";
  })
  .sort();

let failed = false;

if (unresolved.length) {
  failed = true;
  console.error(
    `\n[31m✗ ${unresolved.length} translation key(s) used by the settings tree do not resolve:[0m\n`,
  );
  for (const key of unresolved) console.error(`    ${key}`);
  console.error(
    "\n  These render to the user as the raw key. Add them to FORK_ONLY_STRINGS\n" +
      "  in src/shorthand/branding.ts, or correct the call site.\n",
  );
}

if (missing.length) {
  failed = true;
  console.error(
    `\n[31m✗ ${missing.length} settings component(s) are no longer reachable from any registered section:[0m\n`,
  );
  for (const file of missing) console.error(`    ${file}`);
  console.error(
    "\n  Either render it somewhere in src/shorthand/settings/, or add it to\n" +
      "  ALLOWED_UNREACHABLE in this script with a reason. Do not add it to the\n" +
      "  allow-list just to make this pass — a setting a user could reach before\n" +
      "  and cannot reach now is a regression, not a tidy-up.\n",
  );
}

if (staleAllowances.length) {
  failed = true;
  console.error(
    `\n[31m✗ ${staleAllowances.length} allow-list entr(y/ies) are stale — these ARE reachable now:[0m\n`,
  );
  for (const file of staleAllowances) console.error(`    ${file}`);
  console.error(
    "\n  Remove them from ALLOWED_UNREACHABLE so the list keeps meaning something.\n",
  );
}

if (failed) process.exit(1);

console.log(
  `[32m\n✓ All ${required.length - Object.keys(ALLOWED_UNREACHABLE).length} settings components are reachable ` +
    `(${Object.keys(ALLOWED_UNREACHABLE).length} deliberately unreachable, each with a recorded reason).[0m`,
);
