# Notes: Obsidian plugin install — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Notes" settings section that reports whether the Shorthand plugin for Obsidian is installed in the vault Obsidian would open, and installs it via `obsidian://show-plugin?id=shorthand`.

**Architecture:** One fork-only Rust module reads Obsidian's vault registry and the vault's `.obsidian/` folder to produce a tagged status enum, and opens the install URI from the Rust side of the opener plugin. One pure TypeScript function maps that status (plus UI phase) to a description key and a button; a thin React row renders it and refreshes on window focus; a new section holds the row.

**Tech Stack:** Rust (tauri 2, tauri-specta, serde_json, tempfile for tests), React + react-i18next, Bun test runner (already in use under `src/shorthand`).

**Spec:** `docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md`

## Global Constraints

- Work in the worktree `D:/tools/shorthand-repos/shorthand-app/.worktrees/notes-obsidian` on branch `feat/notes-obsidian-install`. Never touch the main checkout.
- **Zero new dependencies.** No new crates, no new npm packages. `tempfile` is already a dev-dependency; `bun:test` is already used.
- **Additive changes only.** New files under `src/shorthand/` and `src-tauri/src/shorthand/`. The only upstream files edited are `src-tauri/src/lib.rs` (two lines in `collect_commands!`) and `src/bindings.ts` (generated; hand-edited to match specta's output). Do not reformat, reorder or "tidy" anything around those edits.
- Plugin id is exactly `shorthand`. Install URI is exactly `obsidian://show-plugin?id=shorthand`. Obsidian download URL is exactly `https://obsidian.md/download`.
- Section id `notes`, sidebar label key `sidebar.notes` = "Notes", placed after `aicleanup` and before `app` in `SHORTHAND_SECTIONS`.
- **All user-visible strings go in `src/shorthand/locales/en.json`**, flat dotted keys, kept in alphabetical key order. Never in `src/i18n/locales/**`. Copy is verbatim from the spec's Copy table.
- Copy style: sentence case, second person, no exclamation marks, "Could not" not "Failed to".
- The install URI is opened from Rust via `tauri_plugin_opener::OpenerExt` (`app.opener().open_url(...)`), never from the frontend's `openUrl` (its default scope rejects `obsidian://`).
- Rust builds are slow from a cold target dir. Set `CARGO_TARGET_DIR=D:/tools/shorthand-repos/shorthand-app/src-tauri/target` (the main checkout's existing target) for every cargo command so the worktree shares its cache.
- Every task ends with a commit on `feat/notes-obsidian-install`. Commit messages follow the repo's `type(scope): subject` style, e.g. `feat(notes): …`.
- Serena MCP tools (`find_symbol`, `get_symbols_overview`, `replace_symbol_body`, `insert_after_symbol`) are available and preferred for locating and editing symbols in existing files; plain file edits are fine for new files.

---

### Task 1: Rust status resolver and commands

**Files:**
- Create: `src-tauri/src/shorthand/obsidian.rs`
- Modify: `src-tauri/src/shorthand/mod.rs` (add `pub mod obsidian;`)
- Modify: `src-tauri/src/lib.rs` — inside `collect_commands![ … ]`, immediately after the line `commands::change_follow_stream_enabled_setting,`
- Modify: `src/bindings.ts` (hand-add the two commands and the type; see Step 7)

**Interfaces:**
- Produces (Rust): `pub const OBSIDIAN_PLUGIN_ID: &str`, `pub enum ObsidianPluginStatus`, `pub fn resolve_status(config_dir: &Path) -> ObsidianPluginStatus`, commands `get_obsidian_plugin_status` and `open_obsidian_plugin_page`.
- Produces (TS, in `src/bindings.ts`): `commands.getObsidianPluginStatus(): Promise<Result<ObsidianPluginStatus, string>>`, `commands.openObsidianPluginPage(): Promise<Result<null, string>>`, and
  ```ts
  export type ObsidianPluginStatus =
    | { kind: "obsidian_not_found" }
    | { kind: "no_vault" }
    | { kind: "not_installed"; vault_name: string }
    | { kind: "installed"; vault_name: string; version: string; enabled: boolean }
  ```
  Tasks 2 and 3 import these.

- [ ] **Step 1: Create the module with the types and a failing test file**

Create `src-tauri/src/shorthand/obsidian.rs` with this content (tests included; `resolve_status` is stubbed so the tests compile and fail):

```rust
//! Fork-only "Notes" support: is the Shorthand plugin for Obsidian installed,
//! and how does a person get it installed. See
//! docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md.
//!
//! Two facts are read from disk and nothing is written. Obsidian keeps a
//! machine-wide vault registry at `<config>/obsidian/obsidian.json`, and each
//! vault keeps its plugins under `<vault>/.obsidian/plugins/<id>/`. The app
//! never installs the plugin itself: it opens Obsidian on the plugin's own
//! directory page (`obsidian://show-plugin?id=…`, the URI obsidian.md's
//! "Install" buttons use) and Obsidian's Install and Enable buttons do the
//! rest. That is deliberate. Dropping `main.js` into a vault from outside is
//! trivial and Obsidian loads it silently on next launch with no consent
//! step, which is exactly what a third-party app should not be doing to
//! someone's notes.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// The `id` in the plugin's `manifest.json`: also its directory name under
/// `.obsidian/plugins/` and the id the community directory lists it under.
pub const OBSIDIAN_PLUGIN_ID: &str = "shorthand";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObsidianPluginStatus {
    /// No Obsidian config folder, or no vault registry inside it.
    ObsidianNotFound,
    /// Obsidian has run, but the registry lists no vault or is unreadable.
    NoVault,
    /// The vault a URI would land in has no `plugins/shorthand/manifest.json`.
    NotInstalled { vault_name: String },
    /// The manifest is there. `enabled` is whether the id appears in the
    /// vault's `community-plugins.json`; `version` is the manifest's, or
    /// empty if the manifest could not be parsed.
    Installed {
        vault_name: String,
        version: String,
        enabled: bool,
    },
}

#[derive(Deserialize)]
struct VaultRegistry {
    #[serde(default)]
    vaults: HashMap<String, VaultEntry>,
}

#[derive(Deserialize)]
struct VaultEntry {
    path: PathBuf,
    #[serde(default)]
    ts: u64,
    #[serde(default)]
    open: bool,
}

#[derive(Deserialize)]
struct PluginManifest {
    #[serde(default)]
    version: String,
}

/// Obsidian's global config folder: `%APPDATA%\obsidian` on Windows,
/// `~/Library/Application Support/obsidian` on macOS, `$XDG_CONFIG_HOME/obsidian`
/// (or `~/.config/obsidian`) on Linux. Tauri's `config_dir` is exactly that
/// root on all three, so no extra crate is needed.
pub fn obsidian_config_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().config_dir().ok().map(|dir| dir.join("obsidian"))
}

/// Resolves what is on disk for the vault Obsidian would open. Pure, so it
/// can be tested against a fixture tree; `config_dir` is the folder
/// `obsidian_config_dir` returns.
pub fn resolve_status(_config_dir: &Path) -> ObsidianPluginStatus {
    todo!("Task 1 Step 3")
}

#[tauri::command]
#[specta::specta]
pub fn get_obsidian_plugin_status(app: AppHandle) -> Result<ObsidianPluginStatus, String> {
    Ok(match obsidian_config_dir(&app) {
        Some(dir) => resolve_status(&dir),
        None => ObsidianPluginStatus::ObsidianNotFound,
    })
}

/// Opens Obsidian on the plugin's directory page, where its own Install and
/// Enable buttons are. Obsidian picks the vault (the URI has no vault
/// parameter); with Restricted mode on it lands on the Community plugins
/// settings tab instead, which the frontend copy explains.
///
/// Opened from the Rust side of the opener plugin on purpose: the frontend
/// `openUrl` command's default scope allows only http, https, mailto and tel,
/// and widening it means editing an upstream capability file.
#[tauri::command]
#[specta::specta]
pub fn open_obsidian_plugin_page(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(
            format!("obsidian://show-plugin?id={OBSIDIAN_PLUGIN_ID}"),
            None::<String>,
        )
        .map_err(|e| format!("Failed to open Obsidian: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A fake Obsidian install: `<root>/obsidian/obsidian.json` plus any
    /// number of vault folders beside it.
    struct Fixture {
        root: TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                root: tempfile::tempdir().expect("tempdir"),
            }
        }

        fn config_dir(&self) -> PathBuf {
            self.root.path().join("obsidian")
        }

        /// Creates `<root>/<name>/.obsidian/` and returns the vault path.
        fn vault(&self, name: &str) -> PathBuf {
            let path = self.root.path().join(name);
            fs::create_dir_all(path.join(".obsidian")).expect("vault dir");
            path
        }

        /// Writes the registry. Each entry is `(id, vault path, ts, open)`.
        fn write_registry(&self, entries: &[(&str, &Path, u64, bool)]) {
            let vaults: serde_json::Map<String, serde_json::Value> = entries
                .iter()
                .map(|(id, path, ts, open)| {
                    (
                        (*id).to_string(),
                        serde_json::json!({ "path": path, "ts": ts, "open": open }),
                    )
                })
                .collect();
            self.write_registry_raw(&serde_json::json!({ "vaults": vaults }).to_string());
        }

        fn write_registry_raw(&self, contents: &str) {
            fs::create_dir_all(self.config_dir()).expect("config dir");
            fs::write(self.config_dir().join("obsidian.json"), contents).expect("registry");
        }

        fn install_plugin(&self, vault: &Path, manifest: &str) {
            let dir = vault
                .join(".obsidian")
                .join("plugins")
                .join(OBSIDIAN_PLUGIN_ID);
            fs::create_dir_all(&dir).expect("plugin dir");
            fs::write(dir.join("manifest.json"), manifest).expect("manifest");
        }

        fn enable_plugins(&self, vault: &Path, ids: &[&str]) {
            fs::write(
                vault.join(".obsidian").join("community-plugins.json"),
                serde_json::to_string(ids).expect("json"),
            )
            .expect("community-plugins");
        }
    }

    #[test]
    fn no_config_folder_is_obsidian_not_found() {
        let fx = Fixture::new();
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::ObsidianNotFound
        );
    }

    #[test]
    fn registry_without_vaults_is_no_vault() {
        let fx = Fixture::new();
        fx.write_registry(&[]);
        assert_eq!(resolve_status(&fx.config_dir()), ObsidianPluginStatus::NoVault);
    }

    #[test]
    fn unreadable_registry_is_no_vault() {
        let fx = Fixture::new();
        fx.write_registry_raw("{ not json");
        assert_eq!(resolve_status(&fx.config_dir()), ObsidianPluginStatus::NoVault);
    }

    #[test]
    fn vault_without_manifest_is_not_installed() {
        let fx = Fixture::new();
        let vault = fx.vault("Personal");
        fx.write_registry(&[("a1", &vault, 10, true)]);
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::NotInstalled {
                vault_name: "Personal".to_string()
            }
        );
    }

    #[test]
    fn manifest_present_but_not_listed_is_installed_and_disabled() {
        let fx = Fixture::new();
        let vault = fx.vault("Personal");
        fx.write_registry(&[("a1", &vault, 10, true)]);
        fx.install_plugin(&vault, r#"{"id":"shorthand","version":"0.6.0"}"#);
        fx.enable_plugins(&vault, &["dataview"]);
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::Installed {
                vault_name: "Personal".to_string(),
                version: "0.6.0".to_string(),
                enabled: false,
            }
        );
    }

    #[test]
    fn manifest_present_and_listed_is_installed_and_enabled() {
        let fx = Fixture::new();
        let vault = fx.vault("Personal");
        fx.write_registry(&[("a1", &vault, 10, true)]);
        fx.install_plugin(&vault, r#"{"id":"shorthand","version":"0.6.0"}"#);
        fx.enable_plugins(&vault, &["dataview", "shorthand"]);
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::Installed {
                vault_name: "Personal".to_string(),
                version: "0.6.0".to_string(),
                enabled: true,
            }
        );
    }

    #[test]
    fn unparsable_manifest_is_installed_with_empty_version() {
        let fx = Fixture::new();
        let vault = fx.vault("Personal");
        fx.write_registry(&[("a1", &vault, 10, true)]);
        fx.install_plugin(&vault, "not json");
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::Installed {
                vault_name: "Personal".to_string(),
                version: String::new(),
                enabled: false,
            }
        );
    }

    #[test]
    fn open_vault_wins_over_newer_closed_vault() {
        let fx = Fixture::new();
        let open = fx.vault("Open");
        let newer = fx.vault("Newer");
        fx.write_registry(&[("a1", &open, 10, true), ("b2", &newer, 99, false)]);
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::NotInstalled {
                vault_name: "Open".to_string()
            }
        );
    }

    #[test]
    fn newest_vault_is_picked_when_none_is_open() {
        let fx = Fixture::new();
        let old = fx.vault("Old");
        let newest = fx.vault("Newest");
        fx.write_registry(&[("a1", &old, 10, false), ("b2", &newest, 99, false)]);
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::NotInstalled {
                vault_name: "Newest".to_string()
            }
        );
    }
}
```

Add to `src-tauri/src/shorthand/mod.rs`, keeping the list alphabetical:

```rust
pub mod obsidian;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run (from `src-tauri`, with `CARGO_TARGET_DIR` set per Global Constraints):

```bash
cargo test shorthand::obsidian
```

Expected: 9 tests, all FAIL (panic at `todo!("Task 1 Step 3")`). If the crate fails to *compile*, fix that first — the stub must compile.

- [ ] **Step 3: Implement `resolve_status`**

Replace the stub with:

```rust
/// The vault an `obsidian://` URI without a `vault` parameter lands in: the
/// one marked open, else the most recently used. When more than one is
/// open, the most recently used of those — the closest thing on disk to
/// "the frontmost window".
fn pick_vault(vaults: &HashMap<String, VaultEntry>) -> Option<&VaultEntry> {
    vaults.values().max_by_key(|vault| (vault.open, vault.ts))
}

fn vault_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Resolves what is on disk for the vault Obsidian would open. Pure, so it
/// can be tested against a fixture tree; `config_dir` is the folder
/// `obsidian_config_dir` returns.
pub fn resolve_status(config_dir: &Path) -> ObsidianPluginStatus {
    let Ok(raw) = std::fs::read_to_string(config_dir.join("obsidian.json")) else {
        return ObsidianPluginStatus::ObsidianNotFound;
    };
    let Ok(registry) = serde_json::from_str::<VaultRegistry>(&raw) else {
        return ObsidianPluginStatus::NoVault;
    };
    let Some(vault) = pick_vault(&registry.vaults) else {
        return ObsidianPluginStatus::NoVault;
    };

    let vault_name = vault_name(&vault.path);
    let dot_obsidian = vault.path.join(".obsidian");
    let manifest_path = dot_obsidian
        .join("plugins")
        .join(OBSIDIAN_PLUGIN_ID)
        .join("manifest.json");
    let Ok(manifest_raw) = std::fs::read_to_string(manifest_path) else {
        return ObsidianPluginStatus::NotInstalled { vault_name };
    };

    let version = serde_json::from_str::<PluginManifest>(&manifest_raw)
        .map(|manifest| manifest.version)
        .unwrap_or_default();
    let enabled = std::fs::read_to_string(dot_obsidian.join("community-plugins.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|ids| ids.iter().any(|id| id == OBSIDIAN_PLUGIN_ID))
        .unwrap_or(false);

    ObsidianPluginStatus::Installed {
        vault_name,
        version,
        enabled,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test shorthand::obsidian
```

Expected: `test result: ok. 9 passed`. Output must be warning-free (an unused import or dead-code warning is a finding).

- [ ] **Step 5: Register the commands in `lib.rs`**

In `src-tauri/src/lib.rs`, inside `collect_commands![ … ]`, directly after the line
`commands::change_follow_stream_enabled_setting,` add exactly:

```rust
            shorthand::obsidian::get_obsidian_plugin_status,
            shorthand::obsidian::open_obsidian_plugin_page,
```

Touch nothing else in that file.

- [ ] **Step 6: Build and format-check the backend**

```bash
cargo build
cargo fmt -- --check
```

Expected: both clean. If `cargo fmt -- --check` reports `obsidian.rs`, run `cargo fmt` and re-check; do not let it touch any other file (if it wants to, that file was already unformatted — leave it and say so in your report).

- [ ] **Step 7: Hand-add the bindings**

`src/bindings.ts` is generated by tauri-specta at debug-build startup, which cannot run here without launching the GUI. Add the entries by hand in the shape specta emits, so the next real regeneration produces no diff.

In the `commands` object, directly after the `changeFollowStreamEnabledSetting` method block, add:

```ts
async getObsidianPluginStatus() : Promise<Result<ObsidianPluginStatus, string>> {
    try {
    return { status: "ok", data: await TAURI_INVOKE("get_obsidian_plugin_status") };
} catch (e) {
    if(e instanceof Error) throw e;
    else return { status: "error", error: e  as any };
}
},
/**
 * Opens Obsidian on the plugin's directory page, where its own Install and
 * Enable buttons are. Obsidian picks the vault (the URI has no vault
 * parameter); with Restricted mode on it lands on the Community plugins
 * settings tab instead, which the frontend copy explains.
 * 
 * Opened from the Rust side of the opener plugin on purpose: the frontend
 * `openUrl` command's default scope allows only http, https, mailto and tel,
 * and widening it means editing an upstream capability file.
 */
async openObsidianPluginPage() : Promise<Result<null, string>> {
    try {
    return { status: "ok", data: await TAURI_INVOKE("open_obsidian_plugin_page") };
} catch (e) {
    if(e instanceof Error) throw e;
    else return { status: "error", error: e  as any };
}
},
```

Among the `export type …` declarations (they are alphabetical), insert in alphabetical position:

```ts
export type ObsidianPluginStatus = 
/**
 * No Obsidian config folder, or no vault registry inside it.
 */
{ kind: "obsidian_not_found" } | 
/**
 * Obsidian has run, but the registry lists no vault or is unreadable.
 */
{ kind: "no_vault" } | 
/**
 * The vault a URI would land in has no `plugins/shorthand/manifest.json`.
 */
{ kind: "not_installed"; vault_name: string } | 
/**
 * The manifest is there. `enabled` is whether the id appears in the
 * vault's `community-plugins.json`; `version` is the manifest's, or
 * empty if the manifest could not be parsed.
 */
{ kind: "installed"; vault_name: string; version: string; enabled: boolean }
```

Then confirm the frontend still typechecks:

```bash
bun run build
```

Expected: clean. (`bun install --frozen-lockfile` has already been run in this worktree; if `node_modules` is missing, run it.)

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/shorthand/obsidian.rs src-tauri/src/shorthand/mod.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(notes): resolve Obsidian plugin status and open its install page"
```

---

### Task 2: Strings and the pure state mapping

**Files:**
- Modify: `src/shorthand/locales/en.json` (add 18 keys, alphabetical)
- Create: `src/shorthand/notes/obsidianPluginState.ts`
- Create: `src/shorthand/notes/obsidianPluginState.test.ts`

**Interfaces:**
- Consumes: `ObsidianPluginStatus` from `@/bindings` (Task 1).
- Produces: `describeObsidianPlugin(state: ObsidianPluginRowState): ObsidianPluginView`, the `ObsidianPluginRowState` and `ObsidianPluginAction` types, and `ACTION_LABEL_KEYS`. Task 3 imports all of these.

- [ ] **Step 1: Add the strings**

Insert these keys into `src/shorthand/locales/en.json`, each in its alphabetical position among the existing keys (the file is sorted; `settings.notes.*` goes after `settings.modes.*` and before `settings.notetaking.*`; `sidebar.notes` goes after `sidebar.modes`):

```json
"settings.notes.obsidian.action.getObsidian": "Get Obsidian",
"settings.notes.obsidian.action.install": "Install in Obsidian",
"settings.notes.obsidian.action.retry": "Try again",
"settings.notes.obsidian.action.show": "Show in Obsidian",
"settings.notes.obsidian.description": "The Shorthand plugin for Obsidian follows each capture and writes the note into your vault. It runs inside Obsidian, so that is where it gets installed.",
"settings.notes.obsidian.openFailed": "Could not open Obsidian: {{error}}",
"settings.notes.obsidian.status.awaitingObsidian": "Obsidian is opening on the plugin's page. Choose Install, then Enable, and come back here. If Obsidian shows its Community plugins settings instead, turn off Restricted mode and try again.",
"settings.notes.obsidian.status.checkFailed": "Could not check whether the plugin is installed: {{error}}",
"settings.notes.obsidian.status.checking": "Checking…",
"settings.notes.obsidian.status.installed": "Installed in {{vault}}, version {{version}}.",
"settings.notes.obsidian.status.installedDisabled": "Installed in {{vault}} but switched off. Turn it on in Obsidian under Settings → Community plugins.",
"settings.notes.obsidian.status.installedUnknownVersion": "Installed in {{vault}}.",
"settings.notes.obsidian.status.notInstalled": "Not installed in {{vault}}. Obsidian will open on the plugin's page; choose Install, then Enable.",
"settings.notes.obsidian.status.noVault": "Obsidian is installed but has no vault yet. Open or create a vault in Obsidian, then come back here.",
"settings.notes.obsidian.status.obsidianNotFound": "Obsidian isn't installed on this computer, or hasn't been opened yet. Install Obsidian and open a vault, then come back here.",
"settings.notes.obsidian.title": "Obsidian",
"settings.notes.plugin.title": "Shorthand plugin",
"sidebar.notes": "Notes",
```

- [ ] **Step 2: Write the failing test**

Create `src/shorthand/notes/obsidianPluginState.test.ts`:

```ts
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
    expect(
      describeObsidianPlugin({ phase: "error", message: "boom" }),
    ).toEqual({
      descriptionKey: "settings.notes.obsidian.status.checkFailed",
      params: { error: "boom" },
      action: "retry",
    });
  });

  test("obsidian not found -> get Obsidian", () => {
    expect(describeObsidianPlugin(ready({ kind: "obsidian_not_found" }))).toEqual(
      {
        descriptionKey: "settings.notes.obsidian.status.obsidianNotFound",
        params: {},
        action: "get_obsidian",
      },
    );
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
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
bun test src/shorthand/notes
```

Expected: fails to resolve `./obsidianPluginState`.

- [ ] **Step 4: Write the mapping**

Create `src/shorthand/notes/obsidianPluginState.ts`:

```ts
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
export type ObsidianPluginAction = "get_obsidian" | "install" | "show" | "retry";

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
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
bun test src/shorthand/notes
```

Expected: `10 pass, 0 fail`.

- [ ] **Step 6: Run the string gates and the Rust build**

`src-tauri/build.rs` re-reads `en.json` (for `tray.*` / `transcript.*` keys), so a change to that file is a backend change too — see `src/shorthand/locales/README.md`.

```bash
bun run check:fork-translations
bun run check:locale-drift
bunx prettier --check src/shorthand
cd src-tauri && cargo build && cd ..
```

Expected: all clean.

- [ ] **Step 7: Commit**

```bash
git add src/shorthand/locales/en.json src/shorthand/notes/obsidianPluginState.ts src/shorthand/notes/obsidianPluginState.test.ts
git commit -m "feat(notes): copy and state mapping for the Obsidian plugin row"
```

---

### Task 3: The row, the section, and the sidebar entry

**Files:**
- Create: `src/shorthand/notes/ObsidianPluginRow.tsx`
- Create: `src/shorthand/settings/NotesSettings.tsx`
- Modify: `src/shorthand/sections.ts` (one import, one icon import, one entry)
- Modify: `scripts/check-settings-coverage.ts` (add `join(SRC, "shorthand/notes")` to `SETTINGS_COMPONENT_DIRS`)

**Interfaces:**
- Consumes: `commands.getObsidianPluginStatus`, `commands.openObsidianPluginPage` (Task 1); `describeObsidianPlugin`, `ACTION_LABEL_KEYS`, `ObsidianPluginRowState`, `ObsidianPluginAction` (Task 2).
- Produces: `NotesSettings` React component, registered as section `notes`.

- [ ] **Step 1: Create the row**

`src/shorthand/notes/ObsidianPluginRow.tsx`:

```tsx
import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/Button";
import { SettingContainer } from "@/components/ui/SettingContainer";
import {
  ACTION_LABEL_KEYS,
  describeObsidianPlugin,
  type ObsidianPluginAction,
  type ObsidianPluginRowState,
} from "./obsidianPluginState";

const OBSIDIAN_DOWNLOAD_URL = "https://obsidian.md/download";

interface ObsidianPluginRowProps {
  grouped?: boolean;
}

/**
 * Fork-only row: is the Shorthand plugin installed in the vault Obsidian
 * would open, and the one button that moves that along.
 *
 * The status is read from disk by the backend, so it is only as fresh as the
 * last check. The check that matters is the one after the person comes back
 * from Obsidian having pressed Install there — which is a window-focus event
 * from here — so the row re-checks on every focus as well as on mount. What
 * the row says for each status lives in `obsidianPluginState.ts`; this file
 * only fetches and renders.
 */
export const ObsidianPluginRow: React.FC<ObsidianPluginRowProps> = ({
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [state, setState] = useState<ObsidianPluginRowState>({
    phase: "loading",
  });
  const [awaitingObsidian, setAwaitingObsidian] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    // tauri-specta resolves a backend `Err` as `{status: "error"}`, not a
    // rejection — see docs/FRONTEND_TESTING.md for the bug that taught us.
    const result = await commands.getObsidianPluginStatus();
    if (result.status === "error") {
      setState({ phase: "error", message: String(result.error) });
      return;
    }
    setState({ phase: "ready", status: result.data, awaitingObsidian });
  }, [awaitingObsidian]);

  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  const act = async (action: ObsidianPluginAction) => {
    setOpenError(null);
    switch (action) {
      case "retry":
        await refresh();
        return;
      case "get_obsidian":
        await openUrl(OBSIDIAN_DOWNLOAD_URL);
        return;
      case "install":
      case "show": {
        const result = await commands.openObsidianPluginPage();
        if (result.status === "error") {
          setOpenError(String(result.error));
          return;
        }
        if (action === "install") {
          setAwaitingObsidian(true);
          setState((current) =>
            current.phase === "ready"
              ? { ...current, awaitingObsidian: true }
              : current,
          );
        }
      }
    }
  };

  const view = describeObsidianPlugin(state);
  const description = openError
    ? t("settings.notes.obsidian.openFailed", { error: openError })
    : t(view.descriptionKey, view.params);

  return (
    <SettingContainer
      title={t("settings.notes.plugin.title")}
      description={description}
      descriptionMode="inline"
      grouped={grouped}
    >
      {view.action && (
        <Button
          variant={view.action === "install" ? "primary" : "secondary"}
          size="md"
          onClick={() => act(view.action as ObsidianPluginAction)}
        >
          {t(ACTION_LABEL_KEYS[view.action])}
        </Button>
      )}
    </SettingContainer>
  );
};
```

- [ ] **Step 2: Create the section**

`src/shorthand/settings/NotesSettings.tsx`:

```tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ObsidianPluginRow } from "@/shorthand/notes/ObsidianPluginRow";
import { Sheet } from "@/shorthand/ui/Sheet";

/**
 * Fork-only "Notes" section: where captured notes end up, and what has to
 * be in place for each destination.
 *
 * One sheet per destination. Obsidian's setup is a hand-off — the plugin
 * runs inside Obsidian, so it is installed there — and that is the whole of
 * this section today. A destination configured in-app (an OAuth sign-in,
 * say) would be another sheet here, not another section. See
 * docs/superpowers/specs/2026-09-01-notes-obsidian-plugin-install-design.md.
 */
export const NotesSettings: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="max-w-3xl w-full mx-auto space-y-8">
      <Sheet
        title={t("settings.notes.obsidian.title")}
        description={t("settings.notes.obsidian.description")}
      >
        <ObsidianPluginRow grouped={true} />
      </Sheet>
    </div>
  );
};
```

- [ ] **Step 3: Register the section**

In `src/shorthand/sections.ts`:

1. Add `BookOpen` to the `lucide-react` import list (keep the existing order; append it).
2. Add `import { NotesSettings } from "./settings/NotesSettings";` after the `AICleanupSettings` import.
3. Insert this entry between `aicleanup` and `app`:

```ts
  // Where the notes go. Sits after the modes and the cleanup that produce a
  // transcript, and before the app's own preferences: by the time someone
  // reaches this row they know what the app captures, and the next question
  // is where it lands.
  notes: {
    labelKey: "sidebar.notes",
    icon: BookOpen,
    component: NotesSettings,
    enabled: () => true,
  },
```

4. Update the doc comment's order sentence ("The order is the order a person meets the product: …") to include the new step: "… then the optional cleanup, then where the notes go, then the app itself, …".

- [ ] **Step 4: Teach the coverage check about the new directory**

In `scripts/check-settings-coverage.ts`, add `join(SRC, "shorthand/notes"),` to `SETTINGS_COMPONENT_DIRS` after the `shorthand/dictation` entry. (The directory also holds a `.ts` module and a `.test.ts`; confirm the script only inventories `.tsx` files — if it does not, say so in your report rather than working around it.)

- [ ] **Step 5: Run every frontend gate**

```bash
bun run build
bun run lint
bun run check:settings
bun run check:fork-translations
bun test src/shorthand
bunx prettier --check src scripts
```

Expected: all clean. `bun run lint` includes `i18next/no-literal-string`; there should be no literal user-visible strings in the two new components.

- [ ] **Step 6: Commit**

```bash
git add src/shorthand/notes/ObsidianPluginRow.tsx src/shorthand/settings/NotesSettings.tsx src/shorthand/sections.ts scripts/check-settings-coverage.ts
git commit -m "feat(notes): Notes section with the Obsidian plugin install row"
```
