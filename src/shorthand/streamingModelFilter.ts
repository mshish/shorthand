/**
 * Fork-only, import-free. The streaming-chip decisions for
 * `ModelsSettings.tsx`.
 *
 * `ModelsSettings.tsx` used to run its full model list through
 * `useVisibleModels` (see `modelVisibility.ts`) *before* applying its own
 * `filterStreaming` chip. That hook already hides non-streaming models
 * whenever `show_all_settings` is off, so the chip's "off" state could never
 * reveal a non-streaming model in *Available to Download* — the hatch-driven
 * predicate had already removed it upstream of the chip. The two mechanisms
 * disagreed about who owns visibility. The fix is one decision for the whole
 * page: the chip governs both the Downloaded and Available-to-Download
 * sections, and `show_all_settings` only seeds the chip's untouched default.
 *
 * Kept here, rather than in `modelVisibility.ts`, because that module stays
 * the hatch-driven guard for `Onboarding.tsx`, which has no chip and so has
 * no equivalent "explicit override" state to track.
 */

export interface StreamingFilterModel {
  id: string;
  is_downloaded: boolean;
  is_downloading: boolean;
  is_custom: boolean;
}

/**
 * `null` means the user has not touched the chip in this mounted page. The
 * default follows the existing show-all-settings hatch; an explicit click
 * wins until the page unmounts.
 */
export function resolveStreamingFilter(
  override: boolean | null,
  showAllSettings: boolean,
): boolean {
  return override ?? !showAllSettings;
}

/**
 * Models the chip must not hide even when capability data says false.
 * The current model and an in-progress download must remain operable. Custom
 * models are exempt because the app cannot know a user-supplied model's
 * streaming capability from a catalog it is not in. `is_downloaded` alone is
 * deliberately not an exemption: chip off can reveal an ordinary downloaded
 * model, while exempting every downloaded model would make the chip useless
 * for the entire Downloaded Models section.
 */
export function isStreamingFilterExempt(
  model: StreamingFilterModel,
  currentModel: string | null,
): boolean {
  return model.id === currentModel || model.is_downloading || model.is_custom;
}
