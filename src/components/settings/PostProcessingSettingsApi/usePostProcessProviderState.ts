import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { useSettings } from "../../../hooks/useSettings";
import {
  commands,
  type CredentialStatus,
  type PostProcessProvider,
} from "@/bindings";
import type { ModelOption } from "./types";
import type { DropdownOption } from "../../ui/Dropdown";

type PostProcessProviderState = {
  providerOptions: DropdownOption[];
  selectedProviderId: string;
  selectedProvider: PostProcessProvider | undefined;
  isCustomProvider: boolean;
  isAppleProvider: boolean;
  appleIntelligenceUnavailable: boolean;
  baseUrl: string;
  handleBaseUrlChange: (value: string) => void;
  isBaseUrlUpdating: boolean;
  apiKeyStatus: CredentialStatus;
  handleApiKeyChange: (value: string) => Promise<void>;
  handleApiKeyClear: () => void;
  isApiKeyUpdating: boolean;
  model: string;
  handleModelChange: (value: string) => void;
  modelOptions: ModelOption[];
  isModelUpdating: boolean;
  isFetchingModels: boolean;
  handleProviderSelect: (providerId: string) => void;
  handleModelSelect: (value: string) => void;
  handleModelCreate: (value: string) => void;
  handleRefreshModels: () => void;
};

const APPLE_PROVIDER_ID = "apple_intelligence";

export const usePostProcessProviderState = (): PostProcessProviderState => {
  const {
    settings,
    isUpdating,
    setPostProcessProvider,
    updatePostProcessBaseUrl,
    updatePostProcessApiKey,
    updatePostProcessModel,
    fetchPostProcessModels,
    postProcessModelOptions,
    postProcessApiKeyStatus,
  } = useSettings();

  // Settings are guaranteed to have providers after migration
  const providers = settings?.post_process_providers || [];

  const selectedProviderId = useMemo(() => {
    return settings?.post_process_provider_id || providers[0]?.id || "openai";
  }, [providers, settings?.post_process_provider_id]);

  const selectedProvider = useMemo(() => {
    return (
      providers.find((provider) => provider.id === selectedProviderId) ||
      providers[0]
    );
  }, [providers, selectedProviderId]);

  const isAppleProvider = selectedProvider?.id === APPLE_PROVIDER_ID;
  const [appleIntelligenceUnavailable, setAppleIntelligenceUnavailable] =
    useState(false);

  // Use settings directly as single source of truth
  const baseUrl = selectedProvider?.base_url ?? "";
  // The settings payload never carries the key itself; whether a provider
  // is configured comes from the OS credential store instead.
  const apiKeyStatus: CredentialStatus =
    postProcessApiKeyStatus[selectedProviderId] ?? "missing";
  const model = settings?.post_process_models?.[selectedProviderId] ?? "";

  const providerOptions = useMemo<DropdownOption[]>(() => {
    return providers.map((provider) => ({
      value: provider.id,
      label: provider.label,
    }));
  }, [providers]);

  const handleProviderSelect = useCallback(
    async (providerId: string) => {
      // Clear error state on any selection attempt (allows dismissing the error)
      setAppleIntelligenceUnavailable(false);

      if (providerId === selectedProviderId) return;

      // Check Apple Intelligence availability before selecting
      if (providerId === APPLE_PROVIDER_ID) {
        const available = await commands.checkAppleIntelligenceAvailable();
        if (!available) {
          setAppleIntelligenceUnavailable(true);
          // Don't return - still set the provider so dropdown shows the selection
          // The backend gracefully handles unavailable Apple Intelligence
        }
      }

      await setPostProcessProvider(providerId);

      // Auto-fetch available models for the new provider so the model dropdown
      // reflects what's actually valid. Without this, a stale model value from
      // a previous provider/base_url can persist and silently 404 at runtime.
      // Skip when the provider isn't configured yet (no API key / empty base URL)
      // to avoid unnecessary backend errors.
      if (providerId !== APPLE_PROVIDER_ID) {
        const provider = providers.find((p) => p.id === providerId);
        const hasBaseUrl = (provider?.base_url ?? "").trim() !== "";
        const hasApiKey =
          (postProcessApiKeyStatus[providerId] ?? "missing") === "configured";

        if (provider?.id === "custom" ? hasBaseUrl : hasApiKey) {
          void fetchPostProcessModels(providerId);
        }
      }
    },
    [
      selectedProviderId,
      setPostProcessProvider,
      fetchPostProcessModels,
      providers,
      postProcessApiKeyStatus,
    ],
  );

  const handleBaseUrlChange = useCallback(
    (value: string) => {
      if (!selectedProvider || selectedProvider.id !== "custom") {
        return;
      }
      const trimmed = value.trim();
      if (trimmed && trimmed !== baseUrl) {
        void updatePostProcessBaseUrl(selectedProvider.id, trimmed);
      }
    },
    [selectedProvider, baseUrl, updatePostProcessBaseUrl],
  );

  const handleApiKeyChange = useCallback(
    async (value: string) => {
      const trimmed = value.trim();
      // An empty (or whitespace-only) entry is "no change" when a key is
      // already configured — the field never shows the saved value to blank
      // it out by accident. Use the clear action to remove it explicitly.
      if (trimmed === "") return;
      try {
        await updatePostProcessApiKey(selectedProviderId, trimmed);
      } catch (error) {
        toast.error(String(error));
        // Re-thrown so the field (which awaits this call) knows the write
        // failed and does not clear what the user just typed.
        throw error;
      }
    },
    [selectedProviderId, updatePostProcessApiKey],
  );

  const handleApiKeyClear = useCallback(async () => {
    try {
      await updatePostProcessApiKey(selectedProviderId, "");
    } catch (error) {
      toast.error(String(error));
    }
  }, [selectedProviderId, updatePostProcessApiKey]);

  const handleModelChange = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (trimmed !== model) {
        void updatePostProcessModel(selectedProviderId, trimmed);
      }
    },
    [model, selectedProviderId, updatePostProcessModel],
  );

  const handleModelSelect = useCallback(
    (value: string) => {
      void updatePostProcessModel(selectedProviderId, value.trim());
    },
    [selectedProviderId, updatePostProcessModel],
  );

  const handleModelCreate = useCallback(
    (value: string) => {
      void updatePostProcessModel(selectedProviderId, value);
    },
    [selectedProviderId, updatePostProcessModel],
  );

  const handleRefreshModels = useCallback(() => {
    if (isAppleProvider) return;
    void fetchPostProcessModels(selectedProviderId);
  }, [fetchPostProcessModels, isAppleProvider, selectedProviderId]);

  const availableModelsRaw = postProcessModelOptions[selectedProviderId] || [];

  const modelOptions = useMemo<ModelOption[]>(() => {
    const seen = new Set<string>();
    const options: ModelOption[] = [];

    const upsert = (value: string | null | undefined) => {
      const trimmed = value?.trim();
      if (!trimmed || seen.has(trimmed)) return;
      seen.add(trimmed);
      options.push({ value: trimmed, label: trimmed });
    };

    // Add available models from API
    for (const candidate of availableModelsRaw) {
      upsert(candidate);
    }

    // Ensure current model is in the list
    upsert(model);

    return options;
  }, [availableModelsRaw, model]);

  const isBaseUrlUpdating = isUpdating(
    `post_process_base_url:${selectedProviderId}`,
  );
  const isApiKeyUpdating = isUpdating(
    `post_process_api_key:${selectedProviderId}`,
  );
  const isModelUpdating = isUpdating(
    `post_process_model:${selectedProviderId}`,
  );
  const isFetchingModels = isUpdating(
    `post_process_models_fetch:${selectedProviderId}`,
  );

  const isCustomProvider = selectedProvider?.id === "custom";

  // No automatic fetching - user must click refresh button

  return {
    providerOptions,
    selectedProviderId,
    selectedProvider,
    isCustomProvider,
    isAppleProvider,
    appleIntelligenceUnavailable,
    baseUrl,
    handleBaseUrlChange,
    isBaseUrlUpdating,
    apiKeyStatus,
    handleApiKeyChange,
    handleApiKeyClear,
    isApiKeyUpdating,
    model,
    handleModelChange,
    modelOptions,
    isModelUpdating,
    isFetchingModels,
    handleProviderSelect,
    handleModelSelect,
    handleModelCreate,
    handleRefreshModels,
  };
};
