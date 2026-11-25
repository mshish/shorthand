/**
 * Fork-only. Bun coverage for `googleDocsState.ts`: each backend state gets
 * the right copy and buttons, and every key it can produce exists.
 */

import { describe, expect, test } from "bun:test";
import type { GoogleDocsStatus } from "@/bindings";
import en from "../locales/en.json";
import {
  describeGoogleDocs,
  GOOGLE_ACTION_LABEL_KEYS,
  type GoogleDocsRowState,
} from "./googleDocsState";

const status = (
  overrides: Partial<GoogleDocsStatus> = {},
): GoogleDocsStatus => ({
  sign_in_available: true,
  connected: false,
  document_url: null,
  engine_found: true,
  last_run: null,
  ...overrides,
});

const ready = (s: GoogleDocsStatus): GoogleDocsRowState => ({
  phase: "ready",
  status: s,
});

const ALL_STATES: GoogleDocsRowState[] = [
  { phase: "loading" },
  { phase: "connecting" },
  { phase: "error", message: "boom" },
  ready(status({ sign_in_available: false })),
  ready(status()),
  ready(
    status({
      connected: true,
      document_url: "https://docs.google.com/document/d/x/edit",
    }),
  ),
  ready(status({ connected: true, engine_found: false })),
  ready(
    status({
      connected: true,
      last_run: { kind: "running", title: "Meeting 1" },
    }),
  ),
  ready(
    status({
      connected: true,
      last_run: { kind: "finished", title: "Meeting 1" },
    }),
  ),
  ready(
    status({
      connected: true,
      last_run: {
        kind: "failed",
        title: "Meeting 1",
        message: "Shorthand is not running.",
      },
    }),
  ),
];

describe("describeGoogleDocs", () => {
  test("a build without an OAuth client says so and offers nothing", () => {
    const view = describeGoogleDocs(
      ready(status({ sign_in_available: false })),
    );
    expect(view.descriptionKey).toBe(
      "settings.notes.google.status.unavailable",
    );
    expect(view.actions).toEqual([]);
  });

  test("not connected offers both ways to connect", () => {
    expect(describeGoogleDocs(ready(status())).actions).toEqual([
      "connect_pick",
      "connect_create",
    ]);
  });

  test("connected offers open and disconnect, and flags a missing engine", () => {
    const view = describeGoogleDocs(ready(status({ connected: true })));
    expect(view.descriptionKey).toBe("settings.notes.google.status.connected");
    expect(view.actions).toEqual(["open_doc", "disconnect"]);
    expect(
      describeGoogleDocs(
        ready(status({ connected: true, engine_found: false })),
      ).descriptionKey,
    ).toBe("settings.notes.google.status.connectedNoEngine");
  });

  test("a failed meeting shows its reason", () => {
    const view = describeGoogleDocs(
      ready(
        status({
          connected: true,
          last_run: { kind: "failed", title: "Meeting 1", message: "boom" },
        }),
      ),
    );
    expect(view.lastRunKey).toBe("settings.notes.google.lastRun.failed");
    expect(view.lastRunParams).toEqual({ title: "Meeting 1", error: "boom" });
  });

  test("an error reading status offers a retry", () => {
    const view = describeGoogleDocs({ phase: "error", message: "boom" });
    expect(view.actions).toEqual(["retry"]);
    expect(view.params).toEqual({ error: "boom" });
  });

  test("every key the row can use exists in the fork's English strings", () => {
    const strings = en as Record<string, string>;
    for (const state of ALL_STATES) {
      const view = describeGoogleDocs(state);
      expect(typeof strings[view.descriptionKey]).toBe("string");
      if (view.lastRunKey)
        expect(typeof strings[view.lastRunKey]).toBe("string");
    }
    for (const key of Object.values(GOOGLE_ACTION_LABEL_KEYS)) {
      expect(typeof strings[key]).toBe("string");
    }
  });
});
