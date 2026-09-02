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
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::NoVault
        );
    }

    #[test]
    fn unreadable_registry_is_no_vault() {
        let fx = Fixture::new();
        fx.write_registry_raw("{ not json");
        assert_eq!(
            resolve_status(&fx.config_dir()),
            ObsidianPluginStatus::NoVault
        );
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
