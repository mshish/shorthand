/**
 * Fork-only. Bun coverage for the row-state → copy/button decisions in
 * `obsidianPluginState.ts`: one case per row of the spec's Copy table.
 */

import { describe, expect, test } from "bun:test";
import en from "../locales/en.json";
import {
  ACTION_LABEL_KEYS,
  describeObsidianPlugin,
  type ObsidianPluginRowState,
} from "./obsidianPluginState";

const ready = (
  status: Extract<ObsidianPluginRowState, { phase: "ready" }>["status"],
  awaitingObsidian = false,
): ObsidianPluginRowState => ({ phase: "ready", status, awaitingObsidian });

/**
 * Every state exercised below, shared with the key-existence test so the
 * list of inputs lives in one place rather than being retyped there.
 */
const ALL_STATES: ObsidianPluginRowState[] = [
  { phase: "loading" },
  { phase: "error", message: "boom" },
  ready({ kind: "obsidian_not_found" }),
  ready({ kind: "no_vault" }),
  ready({ kind: "not_installed", vault_name: "Personal" }),
  ready({ kind: "not_installed", vault_name: "Personal" }, true),
  ready(
    {
      kind: "installed",
      vault_name: "Personal",
      version: "0.6.0",
      enabled: true,
    },
    true,
  ),
  ready({
    kind: "installed",
    vault_name: "Personal",
    version: "0.6.0",
    enabled: true,
  }),
  ready({
    kind: "installed",
    vault_name: "Personal",
    version: "",
    enabled: true,
  }),
  ready({
    kind: "installed",
    vault_name: "Personal",
    version: "0.6.0",
    enabled: false,
  }),
];

describe("describeObsidianPlugin", () => {
  test("loading -> checking, no button", () => {
    expect(describeObsidianPlugin(ALL_STATES[0])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.checking",
      params: {},
      action: null,
    });
  });

  test("error -> check failed with the message, retry button", () => {
    expect(describeObsidianPlugin(ALL_STATES[1])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.checkFailed",
      params: { error: "boom" },
      action: "retry",
    });
  });

  test("obsidian not found -> get Obsidian", () => {
    expect(describeObsidianPlugin(ALL_STATES[2])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.obsidianNotFound",
      params: {},
      action: "get_obsidian",
    });
  });

  test("no vault -> text only", () => {
    expect(describeObsidianPlugin(ALL_STATES[3])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.noVault",
      params: {},
      action: null,
    });
  });

  test("not installed -> install button, vault named", () => {
    expect(describeObsidianPlugin(ALL_STATES[4])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.notInstalled",
      params: { vault: "Personal" },
      action: "install",
    });
  });

  test("not installed after pressing install -> awaiting Obsidian, button stays", () => {
    expect(describeObsidianPlugin(ALL_STATES[5])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.awaitingObsidian",
      params: { vault: "Personal" },
      action: "install",
    });
  });

  test("awaiting flag is ignored once installed", () => {
    expect(describeObsidianPlugin(ALL_STATES[6]).descriptionKey).toBe(
      "settings.notes.obsidian.status.installed",
    );
  });

  test("installed and enabled -> version shown, show button", () => {
    expect(describeObsidianPlugin(ALL_STATES[7])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installed",
      params: { vault: "Personal", version: "0.6.0" },
      action: "show",
    });
  });

  test("installed, enabled, version unknown -> no version in copy", () => {
    expect(describeObsidianPlugin(ALL_STATES[8])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installedUnknownVersion",
      params: { vault: "Personal" },
      action: "show",
    });
  });

  test("installed but switched off -> how to enable, show button", () => {
    expect(describeObsidianPlugin(ALL_STATES[9])).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installedDisabled",
      params: { vault: "Personal" },
      action: "show",
    });
  });
});

describe("describeObsidianPlugin copy keys", () => {
  const catalogue: Record<string, string> = en;

  test("every descriptionKey and action label key exists in en.json", () => {
    for (const state of ALL_STATES) {
      const { descriptionKey } = describeObsidianPlugin(state);
      expect(descriptionKey in catalogue).toBe(true);
    }
    for (const key of Object.values(ACTION_LABEL_KEYS)) {
      expect(key in catalogue).toBe(true);
    }
  });
});
