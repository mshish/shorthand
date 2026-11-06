import { CURATED_ASR, picksFor, type CatalogModel } from "@/lib/models/catalog";
import type { DownloadState } from "@/lib/models/downloadState";

/**
 * Which catalog models Shorthand offers.
 *
 * Shorthand's pipeline consumes `--follow-stream json` and needs `partial`
 * events; a non-streaming model emits only one final transcript, so live
 * notes never appear — see `shorthand-core/docs/DESIGN.md`. The catalog ships
 * far more non-streaming models than streaming ones, and a user who onboards
 * onto one gets a product that silently does nothing. So the model table hides
 * those models, and the onboarding picks prefer streaming ones.
 *
 * Onboarding's full list hides them, and the `show_all_settings` escape hatch
 * documented in `visibility.ts` shows them again. Settings > Model does not
 * hide anything: it starts with the table's own "Live preview" filter pressed
 * (`DEFAULT_MODEL_FILTERS`), so the user sees why a model is missing and can
 * turn it off.
 */

/** Whether a model is on disk or being fetched, which are the states that count as held. */
export const holdsModel = (state: DownloadState): boolean =>
  state.state === "ready" ||
  state.state === "downloading" ||
  state.state === "checking";

/**
 * Whether the model table shows a model.
 *
 * With `showAllSettings` everything shows. Otherwise a model shows if it
 * streams, or if the user already holds it (`held`: on disk or downloading).
 *
 * The held exemption is not about keeping models the user has. A model's
 * `supports_streaming` is read from the catalog for catalog-listed models, but
 * for a non-catalog GGUF found in the local Hugging Face cache an absent
 * `stt.capability.streaming` header key reads as `false` until the model has
 * been loaded once (`managers/model.rs`, `local_caps`). So `false` can mean
 * "unknown until loaded", and a model on disk or mid-download must never be
 * hidden by a flag that may be wrong, or it can never be loaded and corrected.
 */
export function isTableModelVisible(
  entry: { streaming: boolean },
  showAllSettings: boolean,
  held: boolean,
): boolean {
  return showAllSettings || entry.streaming || held;
}

/** Settings > Model's filters at rest: Live preview pressed, no search. */
export const DEFAULT_MODEL_FILTERS = {
  query: "",
  preview: true,
  translate: false,
};

/**
 * Whether the Models page keeps a model under its pressed capability filters
 * (the search is applied separately).
 *
 * Translate is strict. Live preview never hides a model the user holds (on
 * disk or downloading, which includes the current model and custom ones), for
 * the reason `isTableModelVisible` gives: a non-catalog GGUF reads
 * `streaming: false` until it has been loaded once, so hiding it by that flag
 * could keep it from ever being loaded and corrected. Turning the chip off
 * shows everything.
 */
export function passesModelFilters(
  entry: { streaming: boolean; translate: boolean },
  filters: { preview: boolean; translate: boolean },
  held: boolean,
): boolean {
  if (filters.translate && !entry.translate) return false;
  return held || !filters.preview || entry.streaming;
}

/**
 * The model table's picks for the languages: streaming models only, so every
 * pick can write live notes. For a language no streaming model covers, falls
 * back to the catalog's unfiltered picks and reports `liveNotes: false` so the
 * screen can warn that live notes won't appear with that model.
 *
 * `offered` limits both to models the app can download; `streams` says
 * whether the app's copy of a model streams.
 */
export function shorthandPicksFor(
  languages: string[],
  offered: (model: CatalogModel) => boolean,
  streams: (model: CatalogModel) => boolean,
) {
  const pool = CURATED_ASR.filter((m) => offered(m) && streams(m));
  // `recommendModels` falls back to candidates over its error floor, rated
  // `unsupported` or `experimental` for the language. Those don't count as
  // covering it: such a model would be offered as the recommendation for a
  // language it can't be relied on for.
  const streaming = (
    pool.length ? picksFor(languages, offered, pool) : []
  ).filter((p) => p.status !== "unsupported" && p.status !== "experimental");
  if (streaming.length) return { picks: streaming, liveNotes: true };
  const picks = picksFor(languages, offered);
  return { picks, liveNotes: picks.length === 0 };
}
