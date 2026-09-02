/**
 * Fork-only. Bun coverage for the row-state → copy/button decisions in
 * `obsidianPluginState.ts`: one case per row of the spec's Copy table.
 */

import { describe, expect, test } from "bun:test";
import {
  describeObsidianPlugin,
  type ObsidianPluginRowState,
} from "./obsidianPluginState";

const ready = (
  status: Extract<ObsidianPluginRowState, { phase: "ready" }>["status"],
  awaitingObsidian = false,
): ObsidianPluginRowState => ({ phase: "ready", status, awaitingObsidian });

describe("describeObsidianPlugin", () => {
  test("loading -> checking, no button", () => {
    expect(describeObsidianPlugin({ phase: "loading" })).toEqual({
      descriptionKey: "settings.notes.obsidian.status.checking",
      params: {},
      action: null,
    });
  });

  test("error -> check failed with the message, retry button", () => {
    expect(describeObsidianPlugin({ phase: "error", message: "boom" })).toEqual(
      {
        descriptionKey: "settings.notes.obsidian.status.checkFailed",
        params: { error: "boom" },
        action: "retry",
      },
    );
  });

  test("obsidian not found -> get Obsidian", () => {
    expect(
      describeObsidianPlugin(ready({ kind: "obsidian_not_found" })),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.obsidianNotFound",
      params: {},
      action: "get_obsidian",
    });
  });

  test("no vault -> text only", () => {
    expect(describeObsidianPlugin(ready({ kind: "no_vault" }))).toEqual({
      descriptionKey: "settings.notes.obsidian.status.noVault",
      params: {},
      action: null,
    });
  });

  test("not installed -> install button, vault named", () => {
    expect(
      describeObsidianPlugin(
        ready({ kind: "not_installed", vault_name: "Personal" }),
      ),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.notInstalled",
      params: { vault: "Personal" },
      action: "install",
    });
  });

  test("not installed after pressing install -> awaiting Obsidian, button stays", () => {
    expect(
      describeObsidianPlugin(
        ready({ kind: "not_installed", vault_name: "Personal" }, true),
      ),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.awaitingObsidian",
      params: { vault: "Personal" },
      action: "install",
    });
  });

  test("awaiting flag is ignored once installed", () => {
    expect(
      describeObsidianPlugin(
        ready(
          {
            kind: "installed",
            vault_name: "Personal",
            version: "0.6.0",
            enabled: true,
          },
          true,
        ),
      ).descriptionKey,
    ).toBe("settings.notes.obsidian.status.installed");
  });

  test("installed and enabled -> version shown, show button", () => {
    expect(
      describeObsidianPlugin(
        ready({
          kind: "installed",
          vault_name: "Personal",
          version: "0.6.0",
          enabled: true,
        }),
      ),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installed",
      params: { vault: "Personal", version: "0.6.0" },
      action: "show",
    });
  });

  test("installed, enabled, version unknown -> no version in copy", () => {
    expect(
      describeObsidianPlugin(
        ready({
          kind: "installed",
          vault_name: "Personal",
          version: "",
          enabled: true,
        }),
      ),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installedUnknownVersion",
      params: { vault: "Personal" },
      action: "show",
    });
  });

  test("installed but switched off -> how to enable, show button", () => {
    expect(
      describeObsidianPlugin(
        ready({
          kind: "installed",
          vault_name: "Personal",
          version: "0.6.0",
          enabled: false,
        }),
      ),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.installedDisabled",
      params: { vault: "Personal" },
      action: "show",
    });
  });
});
