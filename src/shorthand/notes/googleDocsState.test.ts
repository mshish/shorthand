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
  document_name: null,
  signed_in: false,
  folder_url: null,
  folder_name: null,
  custom_client_id: null,
  custom_client_secret: "missing",
  using_custom_client: false,
  notes_llm_key: "missing",
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
  ready(status({ connected: true, document_name: "Team standups" })),
  ready(status({ signed_in: true })),
  ready(
    status({
      signed_in: true,
      folder_url: "https://drive.google.com/drive/folders/f",
    }),
  ),
  ready(
    status({
      signed_in: true,
      folder_url: "https://drive.google.com/drive/folders/f",
      folder_name: "Shorthand Meeting Notes",
    }),
  ),
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
  test("a connected doc is named when its name is known", () => {
    const view = describeGoogleDocs(
      ready(status({ connected: true, document_name: "Team standups" })),
    );
    expect(view.descriptionKey).toBe(
      "settings.notes.google.status.connectedNamed",
    );
    expect(view.params).toEqual({ name: "Team standups" });
  });

  test("signing in can be cancelled", () => {
    expect(describeGoogleDocs({ phase: "connecting" }).actions).toEqual([
      "cancel",
    ]);
  });

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
    expect(view.actions).toEqual(["open_doc", "change_doc", "disconnect"]);
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

  test("a doc per meeting asks for the folder, then names it", () => {
    const folder = "https://drive.google.com/drive/folders/f";
    const fresh = describeGoogleDocs(ready(status()), "doc_per_meeting");
    expect(fresh.titleKey).toBe("settings.notes.google.folder.rowTitle");
    expect(fresh.descriptionKey).toBe(
      "settings.notes.google.folder.notConnected",
    );
    expect(fresh.actions).toEqual(["connect_folder"]);
    const signedIn = describeGoogleDocs(
      ready(status({ signed_in: true })),
      "doc_per_meeting",
    );
    expect(signedIn.descriptionKey).toBe(
      "settings.notes.google.folder.signedInNoFolder",
    );
    expect(signedIn.actions).toEqual(["set_up_folder"]);
    const named = describeGoogleDocs(
      ready(
        status({
          signed_in: true,
          folder_url: folder,
          folder_name: "Shorthand Meeting Notes",
        }),
      ),
      "doc_per_meeting",
    );
    expect(named.descriptionKey).toBe(
      "settings.notes.google.folder.connectedNamed",
    );
    expect(named.params).toEqual({ name: "Shorthand Meeting Notes" });
    expect(named.actions).toEqual(["open_folder", "disconnect"]);
  });

  test("one doc keeps its own title", () => {
    expect(describeGoogleDocs(ready(status())).titleKey).toBe(
      "settings.notes.google.rowTitle",
    );
  });

  test("every key the row can use exists in the fork's English strings", () => {
    const strings = en as Record<string, string>;
    for (const state of ALL_STATES) {
      for (const layout of ["tab_per_meeting", "doc_per_meeting"] as const) {
        const view = describeGoogleDocs(state, layout);
        expect(typeof strings[view.titleKey]).toBe("string");
        expect(typeof strings[view.descriptionKey]).toBe("string");
        if (view.lastRunKey)
          expect(typeof strings[view.lastRunKey]).toBe("string");
      }
    }
    for (const key of Object.values(GOOGLE_ACTION_LABEL_KEYS)) {
      expect(typeof strings[key]).toBe("string");
    }
  });
});
