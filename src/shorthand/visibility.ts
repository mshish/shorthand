/**
 * Single source of truth for settings-sidebar section visibility.
 *
 * Handy (upstream) ships every section. Shorthand adds a `show_all_settings`
 * escape hatch that swaps between two modes:
 *
 * - `false` (simplified, the default/product mode): a handful of upstream
 *   sections are hidden in favour of fork-only replacements.
 * - `true` (the hatch): upstream's sections are shown exactly as upstream
 *   intends, and the fork-only replacement sections are hidden instead, so
 *   the two versions of the same settings never both appear at once.
 *
 * Section ids are typed as `string`, not `SidebarSection`
 * (`keyof typeof SECTIONS_CONFIG` in `src/components/Sidebar.tsx`), because
 * the fork-only ids (`capture`, `transcription`, `app`) are named here before
 * they are registered in `SECTIONS_CONFIG`. Typing against `SidebarSection`
 * would make this module fail to compile until those sections exist.
 */

/** Upstream section ids hidden when `show_all_settings` is false. */
export const SIMPLIFIED_MODE_HIDDEN_SECTIONS: ReadonlySet<string> = new Set([
  "general",
  "models",
  "advanced",
  "postprocessing",
]);

/**
 * Fork-only section ids hidden when `show_all_settings` is true. These
 * sections are not registered in `SECTIONS_CONFIG` yet, so naming them here
 * has no effect until they are added.
 */
export const FORK_ONLY_SECTIONS: ReadonlySet<string> = new Set([
  "capture",
  "transcription",
  "app",
]);

/**
 * Whether a section should be visible for a given `show_all_settings` value.
 *
 * This only encodes the simplified-mode/escape-hatch split above; it does
 * not know about a section's own `enabled` predicate in `SECTIONS_CONFIG`
 * (e.g. `postprocessing` gating on `post_process_enabled`, `debug` gating on
 * `debug_mode`). Callers must compose both — see `getVisibleSectionIds`.
 */
export function isSectionVisible(
  sectionId: string,
  showAllSettings: boolean,
): boolean {
  return showAllSettings
    ? !FORK_ONLY_SECTIONS.has(sectionId)
    : !SIMPLIFIED_MODE_HIDDEN_SECTIONS.has(sectionId);
}

interface VisibilitySectionConfig {
  enabled: (settings: any) => boolean;
}

/**
 * Section ids from a `SECTIONS_CONFIG`-shaped object that are currently
 * visible, in declaration order: sections that pass both their own `enabled`
 * predicate and `isSectionVisible`.
 *
 * Shared by `Sidebar.tsx` (to build the rendered section list) and
 * `App.tsx` (to resolve the initial/fallback section) so both apply
 * identical rules and never disagree about what's on screen.
 */
export function getVisibleSectionIds<
  T extends Record<string, VisibilitySectionConfig>,
>(sectionsConfig: T, settings: any): string[] {
  const showAllSettings = settings?.show_all_settings ?? false;

  return Object.keys(sectionsConfig).filter(
    (id) =>
      sectionsConfig[id].enabled(settings) &&
      isSectionVisible(id, showAllSettings),
  );
}
