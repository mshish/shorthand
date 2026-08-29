/**
 * Single source of truth for settings-sidebar section visibility.
 *
 * This used to encode a swap between two whole section trees: `show_all_settings`
 * hid a set of upstream sections in favour of fork-only replacements, or the
 * reverse. That is gone. The two trees shared no vocabulary, so turning the
 * escape hatch on moved you into what felt like a different application, and
 * finding the setting you had just been looking at meant starting over.
 *
 * `show_all_settings` now means "reveal more rows in place" and is read by
 * `useAdvanced` / `AdvancedOnly` at the row level instead. Sections no longer
 * appear and disappear with it, so all that is left here is each section's own
 * `enabled` predicate — and only `debug` still uses one, gating on `debug_mode`.
 *
 * AI cleanup used to have one too, gating on post-processing being on for
 * either mode. It was removed: the section holds the prompt library, so gating
 * it hid the instructions behind the feature that runs them. See the note in
 * `sections.ts`. A section that vanishes is a poor way to say "not configured
 * yet", and a good way to say "this app cannot do that".
 *
 * The fork owns settings presentation outright as a result: upstream's General,
 * Advanced, Models and Post-processing screens are never registered. Their files
 * stay in the tree, untouched and unregistered, because deleting a file upstream
 * still maintains turns every future edit into a delete/modify conflict — the
 * expensive kind.
 */

interface VisibilitySectionConfig {
  enabled: (settings: any) => boolean;
}

/**
 * Section ids from a `SECTIONS_CONFIG`-shaped object that are currently
 * visible, in declaration order.
 *
 * Shared by `Sidebar.tsx` (to build the rendered section list) and
 * `useVisibleSection` (to resolve the initial/fallback section) so both apply
 * identical rules and never disagree about what is on screen.
 */
export function getVisibleSectionIds<
  T extends Record<string, VisibilitySectionConfig>,
>(sectionsConfig: T, settings: any): string[] {
  return Object.keys(sectionsConfig).filter((id) =>
    sectionsConfig[id].enabled(settings),
  );
}
