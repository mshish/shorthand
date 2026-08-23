import React from "react";
import { useTranslation } from "react-i18next";
import { PostProcessingSettingsApi } from "@/components/settings/PostProcessingSettingsApi";
import { PostProcessingSettingsPrompts } from "@/components/settings/PostProcessingSettingsPrompts";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "AI cleanup" section: the LLM connection and the prompt library,
 * and nothing else.
 *
 * Upstream's `PostProcessingSettings` renders three things in one screen — the
 * `transcribe_with_post_process` shortcut, the API connection, and the prompt
 * library — because upstream has a single transcription mode and no reason to
 * tell them apart. This fork has two modes, and the redesign's rule (Part 2 of
 * `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md`) splits
 * them: a row is per-mode if it has a `DictationSettings` counterpart or a
 * mode-specific binding id, and shared otherwise.
 *
 * By that rule the shortcut, the on/off toggle and the prompt *choice* are
 * per-mode and live in the Modes tabs; the provider, key, base URL, model and
 * the prompt *library* are shared and appear exactly once — here. That is the
 * whole reason this file exists rather than a registration of upstream's
 * screen: taking the connection half without the per-mode half is not
 * something upstream's component can be asked to do, and asking it via a prop
 * would mean editing an upstream file for a fork-only concern.
 *
 * The two children are imported unchanged, so upstream keeps ownership of the
 * provider plumbing and the prompt CRUD; only the composition is ours.
 */
export const AICleanupSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      {/* Says the one thing that is not obvious from the rows below: what is
          shared and what is not. The connection and the prompt library are
          global; whether cleanup runs at all, and which prompt it uses, are
          per-mode and set under Modes. A fork-only string, so it lives in
          FORK_ONLY_STRINGS rather than in a locale file. */}
      <p className="px-1 text-xs text-mid-gray">
        {t("settings.aiCleanup.sharedNote")}
      </p>

      <Sheet title={t("settings.postProcessing.api.title")}>
        {/* Bundles ProviderSelect, ApiKeyField (or the Apple Intelligence
            availability alert), BaseUrlField — revealed only by the custom
            provider — and ModelSelect. It takes no props. */}
        <PostProcessingSettingsApi />
      </Sheet>

      <Sheet title={t("settings.postProcessing.prompts.title")}>
        {/* The library: create, edit and delete prompts. Which prompt a mode
            runs is chosen per mode, in that mode's tab. */}
        <PostProcessingSettingsPrompts />
      </Sheet>
    </div>
  );
};
