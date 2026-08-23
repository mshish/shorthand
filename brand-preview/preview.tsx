/*
 * Entry point for the settings-window preview — NOT part of the app bundle.
 *
 * This file does two things and deliberately nothing else: install the fake
 * Tauri IPC layer, then load the app.
 *
 * The order is the whole point, and it is why the render lives in `./app`
 * rather than here. ES module imports are hoisted and evaluated before any
 * statement in this file runs, so an ordinary `import { Sidebar } from ...`
 * alongside `import "./mock-tauri"` would give no guarantee that the mock is
 * installed first — and `@tauri-apps/plugin-os` reads
 * `window.__TAURI_OS_PLUGIN_INTERNALS__` during module evaluation of anything
 * that calls `type()` at the top level. A dynamic `import()` is evaluated when
 * it is awaited, not when the module is parsed, which makes the ordering a fact
 * rather than a hope.
 *
 * Run:  bun x vite dev --port 5199
 *       http://localhost:5199/brand-preview/index.html
 *       node brand-preview/shot.mjs
 *
 * Do not add this directory to .gitignore: Tailwind v4 skips gitignored files
 * when scanning for class names, so any class used only here would silently
 * fail to compile.
 */

import "./mock-tauri";

await import("./app");
