/**
 * Fork-only. Which fields the note taker row shows for each choice, and
 * what it says is still missing, as pure functions of the saved setting.
 * The same split as `googleDocsState.ts`, so the rules are testable with
 * `bun test`. The choices and their names follow the Obsidian plugin's.
 */

import type {
  CredentialStatus,
  NotesAgent,
  NotesAgentBackend,
  NotesLlmProvider,
} from "@/bindings";

/** Matches `NotesAgent::default()` in `shorthand::google::agent`. */
export const DEFAULT_NOTES_AGENT: NotesAgent = {
  backend: "claude",
  codex_model: "",
  cursor_model: "",
  acp_program: "",
  acp_args: "",
  acp_model: "",
  llm_provider: "openai",
  llm_model: "",
  llm_base_url: "",
};

export const NOTES_AGENT_BACKENDS: NotesAgentBackend[] = [
  "claude",
  "codex",
  "cursor",
  "acp",
  "llm",
];

export const NOTES_LLM_PROVIDERS: NotesLlmProvider[] = [
  "openai",
  "anthropic",
  "ollama",
  "openai-compatible",
];

export const BACKEND_LABEL_KEYS: Record<NotesAgentBackend, string> = {
  claude: "settings.notes.agent.option.claude",
  codex: "settings.notes.agent.option.codex",
  cursor: "settings.notes.agent.option.cursor",
  acp: "settings.notes.agent.option.acp",
  llm: "settings.notes.agent.option.llm",
};

export const PROVIDER_LABEL_KEYS: Record<NotesLlmProvider, string> = {
  openai: "settings.notes.agent.provider.openai",
  anthropic: "settings.notes.agent.provider.anthropic",
  ollama: "settings.notes.agent.provider.ollama",
  "openai-compatible": "settings.notes.agent.provider.openaiCompatible",
};

/** The text fields, by the setting each one edits. */
export type AgentTextField =
  | "codex_model"
  | "cursor_model"
  | "acp_program"
  | "acp_args"
  | "acp_model"
  | "llm_model"
  | "llm_base_url";

export type AgentField = AgentTextField | "llm_provider" | "llm_key";

const has = (value: string) => value.trim().length > 0;

/**
 * The rows under the picker. The endpoint is for proxies and self-hosted
 * servers, so it stays behind Advanced unless the provider needs one or
 * one is already set. Ollama on this computer takes no key.
 */
export function agentFields(
  agent: NotesAgent,
  advanced: boolean,
): AgentField[] {
  switch (agent.backend) {
    case "claude":
      return [];
    case "codex":
      return ["codex_model"];
    case "cursor":
      return ["cursor_model"];
    case "acp":
      return ["acp_program", "acp_args", "acp_model"];
    case "llm": {
      const fields: AgentField[] = ["llm_provider", "llm_model"];
      if (
        agent.llm_provider === "openai-compatible" ||
        advanced ||
        has(agent.llm_base_url)
      ) {
        fields.push("llm_base_url");
      }
      if (agent.llm_provider !== "ollama") fields.push("llm_key");
      return fields;
    }
  }
}

/**
 * The one thing that would stop the next meeting's notes, or null. Mirrors
 * the checks `NotesAgent::capture_args` makes before a meeting starts, plus
 * a missing key for a hosted provider, which core would only find out
 * about on its first request.
 */
export function agentMissingKey(
  agent: NotesAgent,
  keyStatus: CredentialStatus | undefined,
): string | null {
  if (agent.backend === "acp" && !has(agent.acp_program)) {
    return "settings.notes.agent.missing.program";
  }
  if (agent.backend !== "llm") return null;
  if (!has(agent.llm_model)) return "settings.notes.agent.missing.model";
  if (agent.llm_provider === "openai-compatible" && !has(agent.llm_base_url)) {
    return "settings.notes.agent.missing.endpoint";
  }
  const hosted =
    agent.llm_provider === "openai" || agent.llm_provider === "anthropic";
  if (hosted && keyStatus === "missing") {
    return "settings.notes.agent.missing.key";
  }
  return null;
}

/** Fills in anything an older settings file didn't have. */
export function withDefaults(
  agent: Partial<NotesAgent> | undefined,
): NotesAgent {
  return { ...DEFAULT_NOTES_AGENT, ...(agent ?? {}) };
}
