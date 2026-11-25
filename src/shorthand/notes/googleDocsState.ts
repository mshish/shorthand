/**
 * Fork-only. What the Google Docs row says and which buttons it shows, as a
 * pure function of what the backend reported. Kept out of the component so
 * the decisions are testable with `bun test`, the same split as
 * `obsidianPluginState.ts`.
 */

import type { GoogleDocsStatus } from "@/bindings";

export type GoogleDocsAction =
  | "connect_pick"
  | "connect_create"
  | "open_doc"
  | "change_doc"
  | "disconnect"
  | "cancel"
  | "retry";

export type GoogleDocsRowState =
  | { phase: "loading" }
  | { phase: "error"; message: string }
  | { phase: "connecting" }
  | { phase: "ready"; status: GoogleDocsStatus };

export interface GoogleDocsView {
  descriptionKey: string;
  params: Record<string, string>;
  actions: GoogleDocsAction[];
  /** A second line about the most recent meeting, when there is one. */
  lastRunKey: string | null;
  lastRunParams: Record<string, string>;
}

export const GOOGLE_ACTION_LABEL_KEYS: Record<GoogleDocsAction, string> = {
  connect_pick: "settings.notes.google.action.connectPick",
  connect_create: "settings.notes.google.action.connectCreate",
  open_doc: "settings.notes.google.action.openDoc",
  change_doc: "settings.notes.google.action.changeDoc",
  disconnect: "settings.notes.google.action.disconnect",
  cancel: "settings.notes.google.action.cancel",
  retry: "settings.notes.google.action.retry",
};

const STATUS = "settings.notes.google.status";
const LAST_RUN = "settings.notes.google.lastRun";

function lastRun(status: GoogleDocsStatus): {
  lastRunKey: string | null;
  lastRunParams: Record<string, string>;
} {
  const run = status.last_run;
  if (!run) return { lastRunKey: null, lastRunParams: {} };
  switch (run.kind) {
    case "running":
      return {
        lastRunKey: `${LAST_RUN}.running`,
        lastRunParams: { title: run.title },
      };
    case "finished":
      return {
        lastRunKey: `${LAST_RUN}.finished`,
        lastRunParams: { title: run.title },
      };
    case "failed":
      return {
        lastRunKey: `${LAST_RUN}.failed`,
        lastRunParams: { title: run.title, error: run.message },
      };
  }
}

export function describeGoogleDocs(state: GoogleDocsRowState): GoogleDocsView {
  const none = { lastRunKey: null, lastRunParams: {} };
  if (state.phase === "loading") {
    return {
      descriptionKey: `${STATUS}.checking`,
      params: {},
      actions: [],
      ...none,
    };
  }
  if (state.phase === "connecting") {
    return {
      descriptionKey: `${STATUS}.connecting`,
      params: {},
      actions: ["cancel"],
      ...none,
    };
  }
  if (state.phase === "error") {
    return {
      descriptionKey: `${STATUS}.failed`,
      params: { error: state.message },
      actions: ["retry"],
      ...none,
    };
  }

  const { status } = state;
  if (!status.sign_in_available) {
    return {
      descriptionKey: `${STATUS}.unavailable`,
      params: {},
      actions: [],
      ...none,
    };
  }
  if (!status.connected) {
    return {
      descriptionKey: `${STATUS}.notConnected`,
      params: {},
      actions: ["connect_pick", "connect_create"],
      ...lastRun(status),
    };
  }
  const name = status.document_name;
  if (!status.engine_found) {
    return {
      descriptionKey: `${STATUS}.connectedNoEngine`,
      params: {},
      actions: ["open_doc", "change_doc", "disconnect"],
      ...lastRun(status),
    };
  }
  return {
    descriptionKey: name ? `${STATUS}.connectedNamed` : `${STATUS}.connected`,
    params: name ? { name } : {},
    actions: ["open_doc", "change_doc", "disconnect"],
    ...lastRun(status),
  };
}
