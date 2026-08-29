import { useMemo } from "react";
import type { ModelInfo } from "@/bindings";
import { useSettings } from "@/hooks/useSettings";

/**
 * Single source of truth for catalog-model visibility.
 *
 * Shorthand's pipeline consumes `--follow-stream json` and needs `partial`
 * events; a non-streaming model emits only one final transcript, so live
 * enhancement never fires — see `shorthand-core/docs/DESIGN.md`. The catalog
 * ships far more non-streaming models than streaming ones, and a user who
 * onboards onto one gets a product that silently does nothing. This module
 * hides those models from the two pickers that show the full/undownloaded
 * catalog, riding the same `show_all_settings` escape hatch documented in
 * `visibility.ts` rather than adding a new setting: "hide models Shorthand
 * can't use" is the same simplified-vs-everything question about a
 * different list.
 *
 * `ModelsSettings.tsx` used to run its full model list through
 * `useVisibleModels` before applying its own, independent `filterStreaming`
 * chip — which meant this hatch-driven predicate could hide a non-streaming
 * model from *Available to Download* that the chip's "off" state was
 * supposed to reveal. The two mechanisms disagreed about who owns
 * visibility. `ModelsSettings.tsx` no longer calls this hook: the chip now
 * governs both of its sections on its own, via
 * `src/shorthand/streamingModelFilter.ts`. This hook remains the hatch-driven
 * guard for `Onboarding.tsx`, which has no chip and therefore no other way to
 * recover a hidden on-disk, in-progress, or custom model.
 */

/**
 * Whether a catalog model should be shown.
 *
 * When `showAllSettings` is true, everything is shown — that's the hatch
 * upstream users and power users reach for.
 *
 * Otherwise, a model is hidden unless it streams, or is already on disk,
 * mid-download, or custom. That exemption is not about letting users keep
 * models they already have — it's forced by how `supports_streaming` can be
 * populated. Catalog-listed models and their alternate quants get their caps
 * from the catalog descriptor (`render_model_info` / `to_model_info_for_file`
 * reading `self.caps.supports_streaming`, `managers/model.rs:249`), and local
 * discovery deliberately skips the header probe for anything catalog-listed
 * because the catalog is authoritative (`model.rs:1765-1768`) — so a
 * catalog match's flag is never an unprobed guess. The genuinely uncertain
 * case is a non-catalog GGUF found in the local HF cache: its `local_caps`
 * reads `probe.supports_streaming.unwrap_or(false)` (`model.rs:360-364`), so
 * an absent `stt.capability.streaming` header key reads as non-streaming
 * until the model has been loaded once and reconciled by
 * `set_runtime_capabilities`. `false` therefore conflates "confirmed
 * non-streaming" with "unknown until loaded" for that one path — a model
 * already on disk, or in the middle of downloading, must never be hidden by
 * a flag we may be wrong about, or it becomes unloadable and so never gets
 * corrected.
 */
export function isModelVisible(
  model: ModelInfo,
  showAllSettings: boolean,
): boolean {
  if (showAllSettings) {
    return true;
  }

  return (
    model.supports_streaming ||
    model.is_downloaded ||
    model.is_downloading ||
    model.is_custom
  );
}

/**
 * Filters a model list down to the ones `isModelVisible` allows, reactive to
 * `show_all_settings` so toggling the hatch in About updates open pickers
 * without a restart.
 */
export function useVisibleModels(models: ModelInfo[]): ModelInfo[] {
  const { getSetting } = useSettings();
  const showAllSettings = getSetting("show_all_settings") ?? false;

  return useMemo(
    () => models.filter((model) => isModelVisible(model, showAllSettings)),
    [models, showAllSettings],
  );
}
