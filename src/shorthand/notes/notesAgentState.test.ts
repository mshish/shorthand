/**
 * Fork-only. Bun coverage for `notesAgentState.ts`: each note taker shows
 * the fields it needs, says what's missing, and every key it can produce
 * exists.
 */

import { describe, expect, test } from "bun:test";
import type { NotesAgent } from "@/bindings";
import en from "../locales/en.json";
import {
  agentFields,
  agentMissingKey,
  BACKEND_LABEL_KEYS,
  DEFAULT_NOTES_AGENT,
  NOTES_AGENT_BACKENDS,
  PROVIDER_LABEL_KEYS,
  withDefaults,
} from "./notesAgentState";

const agent = (overrides: Partial<NotesAgent> = {}): NotesAgent => ({
  ...DEFAULT_NOTES_AGENT,
  ...overrides,
});
const strings = en as Record<string, string>;

describe("agentFields", () => {
  test("Claude needs nothing beyond the choice", () => {
    expect(agentFields(agent(), false)).toEqual([]);
  });

  test("Codex and Cursor each get their own model field", () => {
    expect(agentFields(agent({ backend: "codex" }), false)).toEqual([
      "codex_model",
    ]);
    expect(agentFields(agent({ backend: "cursor" }), false)).toEqual([
      "cursor_model",
    ]);
  });

  test("another app asks for its program, arguments and model", () => {
    expect(agentFields(agent({ backend: "acp" }), false)).toEqual([
      "acp_program",
      "acp_args",
      "acp_model",
    ]);
  });

  test("a hosted provider hides the server URL unless Advanced is on or one is set", () => {
    const llm = agent({ backend: "llm" });
    expect(agentFields(llm, false)).toEqual([
      "llm_provider",
      "llm_model",
      "llm_key",
    ]);
    expect(agentFields(llm, true).includes("llm_base_url")).toBe(true);
    expect(
      agentFields(
        { ...llm, llm_base_url: "https://gateway.example" },
        false,
      ).includes("llm_base_url"),
    ).toBe(true);
  });

  test("an OpenAI-compatible server always shows its URL; Ollama takes no key", () => {
    expect(
      agentFields(
        agent({ backend: "llm", llm_provider: "openai-compatible" }),
        false,
      ),
    ).toEqual(["llm_provider", "llm_model", "llm_base_url", "llm_key"]);
    expect(
      agentFields(agent({ backend: "llm", llm_provider: "ollama" }), false),
    ).toEqual(["llm_provider", "llm_model"]);
  });
});

describe("agentMissingKey", () => {
  test("the tools that find their own defaults are never missing anything", () => {
    for (const backend of ["claude", "codex", "cursor"] as const) {
      expect(agentMissingKey(agent({ backend }), "missing")).toBe(null);
    }
  });

  test("another app without a program", () => {
    expect(agentMissingKey(agent({ backend: "acp" }), undefined)).toBe(
      "settings.notes.agent.missing.program",
    );
    expect(
      agentMissingKey(
        agent({ backend: "acp", acp_program: "/bin/a" }),
        undefined,
      ),
    ).toBe(null);
  });

  test("an API key setup reports the first gap: model, then URL, then key", () => {
    const llm = agent({ backend: "llm" });
    expect(agentMissingKey(llm, "missing")).toBe(
      "settings.notes.agent.missing.model",
    );
    expect(
      agentMissingKey(
        { ...llm, llm_provider: "openai-compatible", llm_model: "m" },
        "unavailable",
      ),
    ).toBe("settings.notes.agent.missing.endpoint");
    expect(agentMissingKey({ ...llm, llm_model: "gpt-5" }, "missing")).toBe(
      "settings.notes.agent.missing.key",
    );
    expect(agentMissingKey({ ...llm, llm_model: "gpt-5" }, "configured")).toBe(
      null,
    );
  });

  test("a local server without a key isn't flagged, since many need none", () => {
    expect(
      agentMissingKey(
        agent({ backend: "llm", llm_provider: "ollama", llm_model: "llama3" }),
        "missing",
      ),
    ).toBe(null);
  });
});

test("an older settings file gets the defaults for what it lacks", () => {
  expect(withDefaults(undefined)).toEqual(DEFAULT_NOTES_AGENT);
  expect(withDefaults({ backend: "cursor" })).toEqual(
    agent({ backend: "cursor" }),
  );
});

test("every key the row can show exists in en.json", () => {
  const keys = [
    ...Object.values(BACKEND_LABEL_KEYS),
    ...Object.values(PROVIDER_LABEL_KEYS),
    ...NOTES_AGENT_BACKENDS.map((b) => `settings.notes.agent.description.${b}`),
    ...["program", "model", "endpoint", "key"].map(
      (m) => `settings.notes.agent.missing.${m}`,
    ),
    ...["model", "program", "args", "llmModel", "endpoint"].flatMap((f) => [
      `settings.notes.agent.${f}.title`,
      `settings.notes.agent.${f}.description`,
      `settings.notes.agent.${f}.placeholder`,
    ]),
    "settings.notes.agent.endpoint.descriptionRequired",
    "settings.notes.agent.key.title",
    "settings.notes.agent.key.description",
    "settings.notes.agent.key.descriptionOptional",
    "settings.notes.agent.title",
    "settings.notes.agent.saveFailed",
  ];
  for (const key of keys) expect(typeof strings[key]).toBe("string");
});
