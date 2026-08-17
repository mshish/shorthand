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
 */

/**
 * Whether a catalog model should be shown.
 *
 * When `showAllSettings` is true, everything is shown — that's the hatch
 * upstream users and power users reach for.
 *
 * Otherwise, a model is hidden unless it streams, or is already on disk,
 * mid-download, or custom. That exemption is not about letting users keep
 * models they already have — it's forced by how `supports_streaming` is
 * populated: it's `Option<bool>` in `CapabilityProbe` but collapses via
 * `unwrap_or(false)`, so a local GGUF whose header omits the
 * `stt.capability.streaming` key (notably parakeet) reads as non-streaming
 * until it has been loaded once and reconciled by
 * `set_runtime_capabilities`. `false` therefore conflates "confirmed
 * non-streaming" with "unknown until loaded" — a model already on disk, or
 * in the middle of downloading, must never be hidden by a flag we may be
 * wrong about, or it becomes unloadable and so never gets corrected.
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
