/**
 * Compiles `.design-sync/tailwind-entry.css` into the stylesheet the design-sync
 * converter ships as `cssEntry`. Run from the repo root:
 *
 *   node .design-sync/build-css.mjs
 *
 * Two things happen here that a bare `tailwindcss -i -o` does not do.
 *
 * 1. Tailwind inlines `@import`ed stylesheets but does not rebase the `url()`s
 *    inside them, so the Fontsource `@font-face` rules come out still pointing
 *    at `./files/*.woff2` — a path that only resolved from inside each font
 *    package. This copies those exact files next to the output so the emitted
 *    paths resolve again, which is what lets the converter find and ship them.
 *    Rewriting the URLs to absolute node_modules paths was the alternative and
 *    is worse: it bakes this machine's layout into a committed artifact.
 *
 * 2. It pins the compile to the same Tailwind the app builds with. A newer
 *    Tailwind renames utilities between minors, and a sheet compiled with a
 *    vocabulary the app's own build does not have is a sheet whose classes
 *    cannot be pasted back into the app.
 */
import { execFileSync } from "node:child_process";
import {
  cpSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");
const ENTRY = join(HERE, "tailwind-entry.css");
const OUT_DIR = join(HERE, "build");
const OUT = join(OUT_DIR, "shorthand-ds.css");
const FILES_DIR = join(OUT_DIR, "files");
const CLI = join(
  ROOT,
  ".ds-sync",
  "node_modules",
  "@tailwindcss",
  "cli",
  "dist",
  "index.mjs",
);

// The font packages the brand stylesheet imports. Kept as roots to search
// rather than a filename list, because the exact subset Fontsource emits
// (which scripts, which axes) changes with the package version.
const FONT_ROOTS = [
  join(ROOT, "node_modules", "@fontsource-variable"),
  join(ROOT, "node_modules", "@fontsource"),
];

mkdirSync(OUT_DIR, { recursive: true });
execFileSync(process.execPath, [CLI, "-i", ENTRY, "-o", OUT], {
  stdio: "inherit",
});

const css = readFileSync(OUT, "utf8");
const wanted = [
  ...new Set(
    [...css.matchAll(/url\(\.\/files\/([^)"']+)\)/g)].map((m) => m[1]),
  ),
];

function findFile(root, name) {
  if (!safeStat(root)?.isDirectory()) return null;
  for (const e of readdirSync(root, { withFileTypes: true })) {
    const p = join(root, e.name);
    if (e.isDirectory()) {
      const hit = findFile(p, name);
      if (hit) return hit;
    } else if (e.name === name) {
      return p;
    }
  }
  return null;
}

function safeStat(p) {
  try {
    return statSync(p);
  } catch {
    return null;
  }
}

// Rebuilt from scratch each run: a font dropped from the sheet must not linger
// here and get shipped as a file nothing references.
rmSync(FILES_DIR, { recursive: true, force: true });
mkdirSync(FILES_DIR, { recursive: true });

const missing = [];
for (const name of wanted) {
  const src = FONT_ROOTS.map((r) => findFile(r, name)).find(Boolean);
  if (!src) {
    missing.push(name);
    continue;
  }
  cpSync(src, join(FILES_DIR, name));
}

console.error(
  `css: ${(css.length / 1024).toFixed(0)}KB, fonts: ${wanted.length - missing.length}/${wanted.length}`,
);
if (missing.length) {
  console.error(
    `! font files not found in node_modules: ${missing.join(", ")}`,
  );
  process.exit(1);
}
