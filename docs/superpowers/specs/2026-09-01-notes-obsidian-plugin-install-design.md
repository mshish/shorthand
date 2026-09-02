# Notes: installing the Obsidian plugin from the app — design

Status: approved in conversation 2026-09-01; implementation follows in
`docs/superpowers/plans/2026-09-01-notes-obsidian-plugin-install.md`.

## The goal

A person installs Shorthand and nothing else. When they want their notes in
Obsidian, the app tells them whether the Shorthand plugin for Obsidian is
installed, and if it is not, gets them there in one click. No BRAT, no hand
copying into `.obsidian/plugins/`, no README step.

This replaces the plan for a separate `shorthand-config` app as the place
sinks are set up. The app owns a **Notes** section; each sink declares its
own setup shape. Obsidian's is a hand-off to Obsidian. Google's, later, will
be in-app. This spec covers Obsidian only.

The plugin is assumed published to the Obsidian community directory under
the id `shorthand` (its `manifest.json` id today). Nothing here works
before that — and nothing here is built for before that.

## What is true today

Verified 2026-09-01 against Obsidian 1.13.7 on this machine, not assumed.

| Fact                                                                             | Evidence                                                                                                                   |
| -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Obsidian keeps a machine-wide vault registry                                     | `%APPDATA%\obsidian\obsidian.json` — `{"vaults":{"<id>":{"path":…,"ts":…,"open":true}}}`                                   |
| Same folder on every OS                                                          | Documented: `~/Library/Application Support/obsidian`, `$XDG_CONFIG_HOME/obsidian` or `~/.config/obsidian`, `%APPDATA%\obsidian` |
| Tauri's `path().config_dir()` is exactly that root on all three                  | Tauri 2 docs for `config_dir`                                                                                              |
| `obsidian://show-plugin?id=<id>` opens Obsidian on the plugin's directory page   | It is what obsidian.md's own "Install" buttons emit, and what Obsidian's "Copy share link" copies                          |
| The handler is registered in Obsidian itself, next to `open`, `search`, `new`    | `obsidian-1.13.7.asar`: `this.register("show-plugin", …)`                                                                  |
| With Restricted mode on, the same URI lands on the Community plugins settings tab | Same handler: `if(!plugins.isEnabled()){settings.openTabById("community-plugins")}`                                       |
| The URI takes no vault parameter; it acts on the frontmost window                | Same handler                                                                                                               |
| A plugin dropped into `.obsidian/plugins/` from outside loads silently on next launch | No per-plugin trust gate exists in the bundle — Restricted mode is the only gate, and it is global                    |
| Installed plugin = `<vault>/.obsidian/plugins/shorthand/manifest.json`           | Local vault                                                                                                                |
| Enabled plugin = id listed in `<vault>/.obsidian/community-plugins.json`         | Local vault                                                                                                                |

## The design

**Hand off, do not install.** The app never writes into a vault. It fires
`obsidian://show-plugin?id=shorthand`; Obsidian opens on the plugin's page
with its own Install and Enable buttons. That is the consent step, in
Obsidian's UI, not ours. The filesystem route is technically trivial and
deliberately not taken: a third-party app writing executable JavaScript
into someone's notes with no consent step is the thing not to be.

**Whichever vault Obsidian would pick.** The URI has no vault parameter, so
neither does the first version. To *report* status the app has to guess the
same vault Obsidian will: the registry entry marked `open`, else the most
recently used (`ts`). When several are open, the most recently used of
those. A vault picker is future work, not v1.

**Read status from disk; refresh when the window regains focus.** Status is
resolved by reading the registry and the vault's `.obsidian/` folder — no
Obsidian process involved. The row re-checks on mount, when the app window
regains focus (which is what happens after the person installs in Obsidian
and comes back), and on demand. That is the feedback loop: click Install →
Obsidian in front → Install, Enable → back to Shorthand → row now says
installed.

### Backend

`src-tauri/src/shorthand/obsidian.rs`, registered from `shorthand/mod.rs`
and `lib.rs`'s `collect_commands!`. Fork-only, additive, off nothing.

```rust
pub const OBSIDIAN_PLUGIN_ID: &str = "shorthand";

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObsidianPluginStatus {
    ObsidianNotFound,                      // no config folder / no registry
    NoVault,                               // registry has no vault, or is unreadable
    NotInstalled { vault_name: String },
    Installed { vault_name: String, version: String, enabled: bool },
}

pub fn resolve_status(config_dir: &Path) -> ObsidianPluginStatus;   // pure, tested
#[tauri::command] fn get_obsidian_plugin_status(app) -> Result<ObsidianPluginStatus, String>;
#[tauri::command] fn open_obsidian_plugin_page(app) -> Result<(), String>;  // opener().open_url(obsidian://show-plugin?id=shorthand)
```

The URI is opened from Rust via `tauri_plugin_opener::OpenerExt`, not from
the frontend's `openUrl`: the frontend command's default scope allows only
`http`, `https`, `mailto` and `tel`, and widening it means editing an
upstream capability file. The Rust API is not scoped.

### Frontend

- `src/shorthand/notes/obsidianPluginState.ts` — pure mapping from
  `{loading | error | ready(status, awaitingObsidian)}` to
  `{descriptionKey, params, action}`. Bun-tested.
- `src/shorthand/notes/ObsidianPluginRow.tsx` — the row: fetches, listens
  for `focus`, renders one `SettingContainer` with one button (or none).
- `src/shorthand/settings/NotesSettings.tsx` — the section: one `Sheet`
  headed "Obsidian" holding the row. Future sinks add sheets here.
- `src/shorthand/sections.ts` — `notes` registered after `aicleanup`,
  before `app`: the order a person meets the product — what the shortcuts
  do, what it listens to, what it transcribes with, optional cleanup, **where
  the notes go**, then the app itself.

### Copy

Sidebar label **Notes**, not Sync. Sentence case, second person, no
exclamation marks, "Could not" not "Failed to", matching
`src/shorthand/locales/en.json`. All strings are fork-only and live there.

| State                  | Row description                                                                                                                                                                                       | Button                        |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------- |
| checking               | Checking…                                                                                                                                                                                             | —                             |
| check failed           | Could not check whether the plugin is installed: {{error}}                                                                                                                                            | Try again (secondary)         |
| Obsidian not found     | Obsidian isn't installed on this computer, or hasn't been opened yet. Install Obsidian and open a vault, then come back here.                                                                         | Get Obsidian (secondary)      |
| no vault               | Obsidian is installed but has no vault yet. Open or create a vault in Obsidian, then come back here.                                                                                                  | —                             |
| not installed          | Not installed in {{vault}}. Obsidian will open on the plugin's page; choose Install, then Enable.                                                                                                     | Install in Obsidian (primary) |
| awaiting Obsidian      | Obsidian is opening on the plugin's page. Choose Install, then Enable, and come back here. If Obsidian shows its Community plugins settings instead, turn off Restricted mode and try again.          | Install in Obsidian (primary) |
| installed, enabled     | Installed in {{vault}}, version {{version}}.                                                                                                                                                          | Show in Obsidian (secondary)  |
| installed, no version  | Installed in {{vault}}.                                                                                                                                                                               | Show in Obsidian (secondary)  |
| installed, switched off | Installed in {{vault}} but switched off. Turn it on in Obsidian under Settings → Community plugins.                                                                                                  | Show in Obsidian (secondary)  |
| open failed            | Could not open Obsidian: {{error}}                                                                                                                                                                    | (unchanged)                   |

Sheet: title **Obsidian**; description "The Shorthand plugin for Obsidian
follows each capture and writes the note into your vault. It runs inside
Obsidian, so that is where it gets installed." Row title **Shorthand plugin**.

"Awaiting Obsidian" is the not-installed state after the button has been
pressed, until a refresh reports installed. It carries the Restricted-mode
hint because that is the one way the hand-off visibly "does nothing".

## Not in scope

- A vault picker, or a `vault` parameter. Obsidian's own URI has neither.
- BRAT, or any pre-publication install path. Hand people the README until
  the directory listing lands.
- Detecting Restricted mode ahead of time. The URI handles it, the copy
  explains it.
- Flatpak Obsidian on Linux (`~/.var/app/md.obsidian.Obsidian/config/obsidian`).
- Google, or any other sink. The section is built to take more sheets.
- Playwright coverage. `docs/FRONTEND_TESTING.md` recommends it and no
  harness exists yet; building one is its own plan. The state mapping is
  unit-tested with the Bun harness that already exists; the React row is
  deliberately thin.

## Testing

- Rust: `resolve_status` against a `tempfile` fixture tree — nine cases
  covering every enum arm, the open-wins-over-newer rule, and the
  newest-when-none-open rule.
- Bun: `describeObsidianPlugin` — one case per row in the copy table.
- Gates that already exist and must stay green: `bun run build`,
  `bun run lint`, `bun run check:settings`, `bun run check:fork-translations`,
  `bun run check:locale-drift`, `cargo build`, `cargo fmt --check`,
  `prettier --check`.
- Manual, by a person: click Install, watch Obsidian open on the plugin
  page, install, come back, watch the row change.
