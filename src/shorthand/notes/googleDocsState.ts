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
  | "disconnect"
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
  disconnect: "settings.notes.google.action.disconnect",
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
      actions: [],
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
  return {
    descriptionKey: status.engine_found
      ? `${STATUS}.connected`
      : `${STATUS}.connectedNoEngine`,
    params: {},
    actions: ["open_doc", "disconnect"],
    ...lastRun(status),
  };
}
