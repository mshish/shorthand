/**
 * Fork-only. What the Obsidian plugin row says and which button it shows,
 * as a pure function of what the backend reported and where the person is
 * in the hand-off. Kept out of the React component so the decision table
 * is testable with `bun test` and the component stays a renderer. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md,
 * "Copy".
 */

import type { ObsidianPluginStatus } from "@/bindings";

/** The one button a row can show. `null` is a text-only row. */
export type ObsidianPluginAction =
  | "get_obsidian"
  | "install"
  | "show"
  | "retry";

export type ObsidianPluginRowState =
  | { phase: "loading" }
  | { phase: "error"; message: string }
  | {
      phase: "ready";
      status: ObsidianPluginStatus;
      /**
       * True after Install has been pressed and until a refresh reports the
       * plugin installed. Only changes the copy while the status is still
       * `not_installed`: the person has been sent to Obsidian and the row
       * should say what to do there, including the one way the hand-off
       * visibly does nothing (Restricted mode).
       */
      awaitingObsidian: boolean;
    };

export interface ObsidianPluginView {
  /** Fork string key for the row description. */
  descriptionKey: string;
  /** Interpolation values for that key. */
  params: Record<string, string>;
  action: ObsidianPluginAction | null;
}

/** Fork string key for each button's label. */
export const ACTION_LABEL_KEYS: Record<ObsidianPluginAction, string> = {
  get_obsidian: "settings.notes.obsidian.action.getObsidian",
  install: "settings.notes.obsidian.action.install",
  show: "settings.notes.obsidian.action.show",
  retry: "settings.notes.obsidian.action.retry",
};

const STATUS = "settings.notes.obsidian.status";

export function describeObsidianPlugin(
  state: ObsidianPluginRowState,
): ObsidianPluginView {
  if (state.phase === "loading") {
    return { descriptionKey: `${STATUS}.checking`, params: {}, action: null };
  }
  if (state.phase === "error") {
    return {
      descriptionKey: `${STATUS}.checkFailed`,
      params: { error: state.message },
      action: "retry",
    };
  }

  const { status, awaitingObsidian } = state;
  switch (status.kind) {
    case "obsidian_not_found":
      return {
        descriptionKey: `${STATUS}.obsidianNotFound`,
        params: {},
        action: "get_obsidian",
      };
    case "no_vault":
      return { descriptionKey: `${STATUS}.noVault`, params: {}, action: null };
    case "not_installed":
      return {
        descriptionKey: awaitingObsidian
          ? `${STATUS}.awaitingObsidian`
          : `${STATUS}.notInstalled`,
        params: { vault: status.vault_name },
        action: "install",
      };
    case "installed":
      if (!status.enabled) {
        return {
          descriptionKey: `${STATUS}.installedDisabled`,
          params: { vault: status.vault_name },
          action: "show",
        };
      }
      return status.version
        ? {
            descriptionKey: `${STATUS}.installed`,
            params: { vault: status.vault_name, version: status.version },
            action: "show",
          }
        : {
            descriptionKey: `${STATUS}.installedUnknownVersion`,
            params: { vault: status.vault_name },
            action: "show",
          };
  }
}
