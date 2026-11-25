import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import {
  commands,
  type CredentialStatus,
  type NotesAgent,
  type NotesAgentBackend,
  type NotesLlmProvider,
} from "@/bindings";
import { ApiKeyField } from "@/components/settings/PostProcessingSettingsApi/ApiKeyField";
import { Dropdown } from "@/components/ui/Dropdown";
import { Input } from "@/components/ui/Input";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useAdvanced } from "@/shorthand/useAdvanced";
import {
  agentFields,
  agentMissingKey,
  BACKEND_LABEL_KEYS,
  NOTES_AGENT_BACKENDS,
  NOTES_LLM_PROVIDERS,
  PROVIDER_LABEL_KEYS,
  withDefaults,
  type AgentTextField,
} from "./notesAgentState";

/** Matches `shorthand::google::STATUS_EVENT`. */
const STATUS_EVENT = "google-docs-status-changed";

interface NotesAgentRowProps {
  grouped?: boolean;
}

/** Copy for each text field, by the setting it edits. */
const TEXT_FIELD_KEYS: Record<AgentTextField, string> = {
  codex_model: "model",
  cursor_model: "model",
  acp_model: "model",
  acp_program: "program",
  acp_args: "args",
  llm_model: "llmModel",
  llm_base_url: "endpoint",
};

/**
 * Fork-only rows: who writes the meeting notes into the doc, with the same
 * five choices as the Obsidian plugin, and whatever that choice needs. Each
 * text field saves when it loses focus; the setting is saved whole, because
 * the runner reads a choice's fields together.
 */
export const NotesAgentRows: React.FC<NotesAgentRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const { advanced } = useAdvanced();
  const agent = withDefaults(getSetting("google_notes_agent"));
  const [drafts, setDrafts] = useState<Partial<Record<AgentTextField, string>>>(
    {},
  );
  const [keyStatus, setKeyStatus] = useState<CredentialStatus | undefined>();
  const [keyError, setKeyError] = useState<string | null>(null);

  const refreshKey = useCallback(async () => {
    try {
      setKeyStatus((await commands.getGoogleDocsStatus()).notes_llm_key);
    } catch {
      setKeyStatus("unavailable");
    }
  }, []);

  useEffect(() => {
    refreshKey();
    const unlisten = listen(STATUS_EVENT, () => refreshKey());
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [refreshKey]);

  const save = (next: Partial<NotesAgent>) =>
    updateSetting("google_notes_agent", { ...agent, ...next });

  const commitText = (field: AgentTextField) => {
    const draft = drafts[field];
    setDrafts(({ [field]: _done, ...rest }) => rest);
    if (draft !== undefined && draft.trim() !== agent[field]) {
      void save({ [field]: draft.trim() });
    }
  };

  const applyKey = async (
    call: Promise<Awaited<ReturnType<typeof commands.setGoogleNotesLlmKey>>>,
  ) => {
    const result = await call;
    if (result.status === "error") {
      setKeyError(result.error);
      throw new Error(result.error);
    }
    setKeyError(null);
    setKeyStatus(result.data.notes_llm_key);
  };

  const missing = agentMissingKey(agent, keyStatus);
  const pickerDescription = [
    t(`settings.notes.agent.description.${agent.backend}`),
    missing ? t(missing) : null,
  ]
    .filter(Boolean)
    .join(" ");
  const saving = isUpdating("google_notes_agent");

  const textRow = (field: AgentTextField) => {
    const copy = TEXT_FIELD_KEYS[field];
    const required =
      field === "llm_base_url" && agent.llm_provider === "openai-compatible";
    return (
      <SettingContainer
        key={field}
        title={t(`settings.notes.agent.${copy}.title`)}
        description={t(
          `settings.notes.agent.${copy}.${required ? "descriptionRequired" : "description"}`,
        )}
        descriptionMode="inline"
        grouped={grouped}
      >
        <div className="ps-4">
          <Input
            type="text"
            value={drafts[field] ?? agent[field]}
            onChange={(event) =>
              setDrafts((current) => ({
                ...current,
                [field]: event.target.value,
              }))
            }
            onBlur={() => commitText(field)}
            placeholder={t(
              `settings.notes.agent.${copy}.${required ? "placeholderRequired" : "placeholder"}`,
            )}
            variant="compact"
            className="w-64"
            spellCheck={false}
            autoComplete="off"
          />
        </div>
      </SettingContainer>
    );
  };

  return (
    <>
      <SettingContainer
        title={t("settings.notes.agent.title")}
        description={pickerDescription}
        descriptionMode="inline"
        grouped={grouped}
      >
        <div className="ps-4">
          <Dropdown
            options={NOTES_AGENT_BACKENDS.map((backend) => ({
              value: backend,
              label: t(BACKEND_LABEL_KEYS[backend]),
            }))}
            selectedValue={agent.backend}
            onSelect={(value) => save({ backend: value as NotesAgentBackend })}
            disabled={saving}
            className="w-64"
          />
        </div>
      </SettingContainer>
      {agentFields(agent, advanced).map((field) => {
        if (field === "llm_provider") {
          return (
            <SettingContainer
              key={field}
              title={t("settings.notes.agent.provider.title")}
              description={t("settings.notes.agent.provider.description")}
              descriptionMode="inline"
              grouped={grouped}
            >
              <div className="ps-4">
                <Dropdown
                  options={NOTES_LLM_PROVIDERS.map((provider) => ({
                    value: provider,
                    label: t(PROVIDER_LABEL_KEYS[provider]),
                  }))}
                  selectedValue={agent.llm_provider}
                  onSelect={(value) =>
                    save({ llm_provider: value as NotesLlmProvider })
                  }
                  disabled={saving}
                  className="w-64"
                />
              </div>
            </SettingContainer>
          );
        }
        if (field === "llm_key") {
          const optional = agent.llm_provider === "openai-compatible";
          return (
            <SettingContainer
              key={field}
              title={t("settings.notes.agent.key.title")}
              description={
                keyError
                  ? t("settings.notes.agent.saveFailed", { error: keyError })
                  : t(
                      `settings.notes.agent.key.${optional ? "descriptionOptional" : "description"}`,
                    )
              }
              descriptionMode="inline"
              grouped={grouped}
            >
              <div className="flex w-[272px] items-center gap-2 ps-4 [&_input]:min-w-0">
                <ApiKeyField
                  status={keyStatus ?? "unavailable"}
                  onCommit={(secret) =>
                    applyKey(commands.setGoogleNotesLlmKey(secret))
                  }
                  onClear={() => {
                    applyKey(commands.clearGoogleNotesLlmKey()).catch(() => {
                      // `applyKey` already shows the error.
                    });
                  }}
                  disabled={
                    keyStatus === undefined || keyStatus === "unavailable"
                  }
                />
              </div>
            </SettingContainer>
          );
        }
        return textRow(field);
      })}
    </>
  );
};
