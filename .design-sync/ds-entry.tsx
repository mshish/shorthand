/**
 * Design-sync entry. Fork-only, not imported by the app.
 *
 * The converter bundles from a package `dist/`; this repo is an application and
 * ships none, and its default fallback (`export *` over every .tsx under src/)
 * would drag the Tauri shell, the Zustand stores and the i18n runtime into the
 * bundle. This file is the deliberate export surface instead: the brand marks
 * and the one primitive a marketing page actually reaches for.
 */
export { default as ShorthandMark } from "../src/shorthand/brand/ShorthandMark";
export { ShorthandWordmark } from "../src/shorthand/brand/ShorthandWordmark";
export { Button } from "../src/components/ui/Button";
