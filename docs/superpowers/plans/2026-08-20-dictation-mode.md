# Dictation Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in dictation mode to Shorthand — Handy's transcribe-and-paste-into-the-focused-window behaviour — running alongside meeting transcription with its own shortcuts and its own settings, without changing meeting mode's defaults or its settings screens.

**Architecture:** A fork-only _active-mode cell_ records which mode the in-flight capture belongs to; a pure `apply_mode(settings, mode)` returns an `AppSettings` with the dictation overrides applied. Because it returns a full `AppSettings`, every consumer in upstream code changes exactly one line — `get_settings(x)` becomes `resolve_settings(x)` — so no upstream function signature changes. Two new shortcut bindings (`dictate`, `dictate_with_post_process`) reuse the existing `TranscribeAction`; mode is derived from the binding id. Settings live in one nested `dictation` field, so the frontend store needs one updater entry rather than thirteen.

**Tech Stack:** Rust + Tauri 2.x (`tauri-plugin-store`, `tauri-specta`), React 18 + TypeScript, Zustand, Tailwind, i18next, Bun.

**Spec:** [`docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md`](../specs/2026-08-20-shorthand-dictation-mode-design.md)

## Global Constraints

Every task's requirements implicitly include this section.

**Branch:** all work lands on `shorthand`. Never commit to `main` (a clean mirror of `upstream/main`).

**Fork mergeability — the governing constraint.** This repo merges from `cjpais/Handy` indefinitely. Every line changed in a file upstream also changes is a merge conflict, forever.

- Prefer new files. New fork-only code goes under `src-tauri/src/shorthand/` or `src/shorthand/`.
- When you must edit an upstream file, keep it small and local. Do not reformat, reorder imports, rename neighbouring symbols, or tidy surrounding code.
- The spec's "Complete list of upstream files edited" table is the budget. Touching an upstream file not on that list needs an explicit justification in the commit message.

**Load-bearing invariants — do not "improve" these:**

- `apply_mode` returns a full `AppSettings`, not a narrower struct. That is what keeps upstream call sites to one line each.
- `DictationSettings::default().paste_method` must be `CtrlV` (Windows/macOS) or `Direct` (Linux) — **never** `PasteMethod::None`. The top-level `impl Default for PasteMethod` returns `None` for meeting mode; inheriting it would make enabling dictation silently do nothing.
- The mode cell defaults to `Meeting`, is set on every capture start, and is **never cleared**. Clearing it would introduce a race with async work that outlives the recording.
- A process-global `AtomicBool` for the mode cell is deliberate and matches house style — see `OVERLAY_ENABLED` and `WINDOWS_OVERLAY_IS_STREAMING` in `overlay.rs` and `WEBVIEW_LOG_STREAMING` in `lib.rs`. `app.manage()` is reserved for manager objects in this codebase.

**Shortcut defaults — exact values:**

| Binding                        | Windows / Linux        | macOS                     |
| ------------------------------ | ---------------------- | ------------------------- |
| `transcribe`                   | `ctrl+alt+space`       | `ctrl+shift+space`        |
| `transcribe_with_post_process` | `ctrl+alt+shift+space` | `ctrl+shift+option+space` |
| `dictate`                      | `ctrl+space`           | `option+space`            |
| `dictate_with_post_process`    | `ctrl+shift+space`     | `option+shift+space`      |
| `cancel`                       | `escape`               | `escape`                  |

No settings migration and no `CURRENT_SETTINGS_SCHEMA_VERSION` bump: the bindings merge fills vacant keys only, so existing installs keep their shortcuts and the frozen v0.9.0 fixture keeps its `f13`.

**Testing:**

- Rust tests go in `#[cfg(test)] mod tests` in the same file, matching this repo's convention.
- `cargo test` and `cargo clippy` must pass at the end of every task.
- **There is no React test harness, and you must not add one.** Adding vitest/jest/testing-library puts devDependencies in upstream's `package.json` and `bun.lock` — another permanent conflict surface. Frontend tasks verify through `bun run lint`, `bun run check:translations`, and the explicit manual checks each task lists.

**Internationalisation:** 24 locale files under `src/i18n/locales/`. Every new user-facing string needs a key in **all** of them, with the English text as the value in every one. `check:translations` fails CI on any gap, and `eslint-plugin-i18next`'s `no-literal-string` rule forbids bare literals in JSX. Reuse existing keys where the label is identical.

**Generated code:** `src/bindings.ts` is emitted by tauri-specta under `#[cfg(debug_assertions)]` and carries a do-not-edit header. Regenerate it with a debug build. Never hand-edit it.

**Commits:** conventional prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on _why_, not _what_.

---

### Task 1: Mode cell + `DictationSettings` data model

**Files:**

- Create: `src-tauri/src/shorthand/mod.rs`
- Create: `src-tauri/src/shorthand/mode.rs`
- Create: `src-tauri/src/shorthand/dictation.rs`
- Modify: `src-tauri/src/lib.rs:21-22`
- Modify: `src-tauri/src/settings.rs:490-491` (add the `dictation` field)
- Modify: `src-tauri/src/settings.rs:933-934` (add the default-settings initializer)

**Interfaces:**

- Consumes: `crate::settings::{AppSettings, AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool}` (all already exist in `settings.rs`)
- Produces:
  - `pub enum Mode { Meeting, Dictation }` (`Debug, Clone, Copy, PartialEq, Eq, Default`, `#[default] Meeting`)
  - `pub fn mode_for_binding(binding_id: &str) -> Mode`
  - `pub fn set_active(app: &AppHandle, binding_id: &str)`
  - `pub fn active(app: &AppHandle) -> Mode`
  - `pub struct DictationSettings { .. }` (13 fields) with an explicit `impl Default`
  - `AppSettings.dictation: DictationSettings` field, consumed by every later task

This task does **not** define `apply_mode`, `resolve_settings`, or `resolve_push_to_talk` — those are produced by Tasks 2 and 3, which extend `dictation.rs` rather than replacing it.

- [ ] **Step 1: Write the failing test for `mode_for_binding`, and scaffold the module it lives in**

A brand-new module needs to exist and be wired into the crate before `cargo test` can even see it, so this step creates the skeleton and the first test together.

Create `src-tauri/src/shorthand/mod.rs`:

```rust
//! Fork-only dictation-mode feature: an active-mode cell plus the settings
//! and resolvers it gates. See
//! docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.

pub mod dictation;
pub mod mode;
```

Create `src-tauri/src/shorthand/mode.rs`:

```rust
//! The fork-only "active mode" cell. `TranscribeAction::start` calls
//! `set_active` once per capture; every per-mode resolver in
//! `shorthand::dictation` reads it back via `active`. See "The active-mode
//! cell" in the design doc for why this is a process-wide cell rather than a
//! parameter threaded through `clipboard::paste`, `overlay::show_overlay_state`,
//! and `actions.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Meeting,
    Dictation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting() {
        assert_eq!(mode_for_binding("dictate"), Mode::Dictation);
        assert_eq!(
            mode_for_binding("dictate_with_post_process"),
            Mode::Dictation
        );
        assert_eq!(mode_for_binding("transcribe"), Mode::Meeting);
        assert_eq!(
            mode_for_binding("transcribe_with_post_process"),
            Mode::Meeting
        );
        assert_eq!(mode_for_binding("cancel"), Mode::Meeting);
        assert_eq!(mode_for_binding("unknown"), Mode::Meeting);
    }
}
```

Add `pub mod shorthand;` to `src-tauri/src/lib.rs`, between the existing `mod settings;` and `mod shortcut;` lines:

```rust
mod settings;
pub mod shorthand;
mod shortcut;
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test shorthand::mode::tests::mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting`
Expected: FAIL to compile — `cannot find function 'mode_for_binding' in this scope`.

- [ ] **Step 3: Implement `mode_for_binding`, `set_active`, and `active`**

Add to `src-tauri/src/shorthand/mode.rs`, below the `Mode` enum and above the `#[cfg(test)]` block:

```rust
/// Only one capture runs at a time — `AudioRecordingManager` tracks a single
/// `is_recording` flag, and `TranscriptionCoordinator`'s `Stage` state machine
/// serialises transcribe bindings — so a single process-wide flag is safe:
/// there is never more than one capture this cell could ambiguously describe.
static ACTIVE_MODE_IS_DICTATION: AtomicBool = AtomicBool::new(false);

/// "dictate" and "dictate_with_post_process" are dictation; every other
/// binding id (including ones this module doesn't know about) is meeting.
pub fn mode_for_binding(binding_id: &str) -> Mode {
    match binding_id {
        "dictate" | "dictate_with_post_process" => Mode::Dictation,
        _ => Mode::Meeting,
    }
}

/// Records the mode of the capture that is starting. Called once, from
/// `TranscribeAction::start`. Never cleared: "the mode of the most recently
/// started capture" is always the right answer for work belonging to that
/// capture, including async work that outlives the recording itself. A
/// cleared cell would introduce a race an uncleared one does not have.
pub fn set_active(_app: &AppHandle, binding_id: &str) {
    let is_dictation = mode_for_binding(binding_id) == Mode::Dictation;
    ACTIVE_MODE_IS_DICTATION.store(is_dictation, Ordering::Release);
}

/// The mode of the most recently started capture. Defaults to `Meeting`, so
/// any code path reached before the first capture behaves exactly as it did
/// before this module existed.
pub fn active(_app: &AppHandle) -> Mode {
    if ACTIVE_MODE_IS_DICTATION.load(Ordering::Acquire) {
        Mode::Dictation
    } else {
        Mode::Meeting
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test shorthand::mode::tests::mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting`
Expected: PASS

- [ ] **Step 5: Write the failing test for `DictationSettings::default()`**

Create `src-tauri/src/shorthand/dictation.rs`:

```rust
//! Dictation-mode settings and the per-mode field resolver. See
//! docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.

use crate::settings::{AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Dictation's own copy of settings meeting mode also has, so enabling or
/// configuring dictation never touches a meeting-mode value. See "Per-mode
/// and shared settings" in the design doc for which fields live here versus
/// staying shared on `AppSettings`.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct DictationSettings {
    pub enabled: bool,
    pub push_to_talk: bool,
    pub paste_method: PasteMethod,
    pub clipboard_handling: ClipboardHandling,
    pub auto_submit: bool,
    pub auto_submit_key: AutoSubmitKey,
    pub append_trailing_space: bool,
    pub typing_tool: TypingTool,
    pub overlay_style: OverlayStyle,
    pub save_recordings: bool,
    pub save_transcripts: bool,
    pub post_process_enabled: bool,
    pub post_process_selected_prompt_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paste_method_is_not_none() {
        assert_ne!(DictationSettings::default().paste_method, PasteMethod::None);
    }
}
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::default_paste_method_is_not_none`
Expected: FAIL to compile — the derives above require `DictationSettings: Default` (container-level `#[serde(default)]` needs it), and no `Default` impl exists yet: `the trait bound 'DictationSettings: Default' is not satisfied`.

- [ ] **Step 7: Implement `impl Default for DictationSettings`**

Add to `src-tauri/src/shorthand/dictation.rs`, below the struct and above `#[cfg(test)]`:

```rust
impl Default for DictationSettings {
    // `PasteMethod`'s own `#[default]` is `None` (see settings.rs) because
    // this fork delivers meeting transcripts to follower processes instead
    // of the focused window. Dictation is the opposite: pasting into the
    // focused window is the entire feature, so it must NOT inherit that
    // default — it needs Handy's original per-platform choice. Do not
    // collapse this back into a derived `Default`; that would silently
    // reintroduce `PasteMethod::None` here.
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        let paste_method = PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        let paste_method = PasteMethod::CtrlV;

        Self {
            enabled: false,
            // Meetings run an hour and are toggled; dictation is seconds and
            // is held.
            push_to_talk: true,
            paste_method,
            clipboard_handling: ClipboardHandling::default(),
            auto_submit: false,
            auto_submit_key: AutoSubmitKey::default(),
            append_trailing_space: false,
            typing_tool: TypingTool::default(),
            // The compact pill, not a live-transcript panel over the text
            // field being dictated into.
            overlay_style: OverlayStyle::Minimal,
            // Consent, not preference — stays opt-in like meeting mode's
            // equivalent toggles.
            save_recordings: false,
            save_transcripts: false,
            post_process_enabled: false,
            post_process_selected_prompt_id: None,
        }
    }
}
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::default_paste_method_is_not_none`
Expected: PASS

- [ ] **Step 9: Add the `dictation` field to `AppSettings`**

In `src-tauri/src/settings.rs`, the `AppSettings` struct currently ends (lines 489-491):

```rust
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
    #[serde(default)]
    pub show_all_settings: bool,
}
```

Change to:

```rust
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
    #[serde(default)]
    pub show_all_settings: bool,
    /// Dictation-mode settings, applied over the equivalent fields above when
    /// the capture in flight is `shorthand::mode::Mode::Dictation`. See
    /// `shorthand::dictation::apply_mode`.
    #[serde(default)]
    pub dictation: crate::shorthand::dictation::DictationSettings,
}
```

And in `get_default_settings()`, which currently ends (lines 931-934):

```rust
        vad_enabled: default_vad_enabled(),
        overlay_style: default_overlay_style(),
        show_all_settings: false,
    }
```

Change to:

```rust
        vad_enabled: default_vad_enabled(),
        overlay_style: default_overlay_style(),
        show_all_settings: false,
        dictation: crate::shorthand::dictation::DictationSettings::default(),
    }
```

- [ ] **Step 10: Run the existing settings tests to confirm they pass unmodified**

Run: `cd src-tauri && cargo test settings::tests`
Expected: PASS, including `empty_store_parses_with_defaults` and `frozen_v0_9_store_parses_strictly_and_migrates_only_paste_method` — both survive without any edit because `AppSettings` carries a container-level `#[serde(default)]`, so the new `dictation` field is simply absent from both fixtures and falls back to `DictationSettings::default()`.

- [ ] **Step 11: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS (no regressions elsewhere)

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/shorthand/mod.rs src-tauri/src/shorthand/mode.rs src-tauri/src/shorthand/dictation.rs src-tauri/src/lib.rs src-tauri/src/settings.rs
git commit -m "feat(shorthand): add the dictation mode cell and settings data model

Dictation mode needs its own settings and a way to tell which mode the
capture in flight belongs to, without threading a parameter through every
upstream function that currently reads global settings."
```

---

### Task 2: Bindings and dispatch, gated off — two bindings, five skip-guard sites

**Files:**

- Modify: `src-tauri/src/settings.rs:815-865` (`get_default_settings()` bindings block)
- Modify: `src-tauri/src/settings.rs` tests module (add the distinct-shortcuts test, near `default_settings_disable_saving_recordings_and_transcripts`)
- Modify: `src-tauri/src/shorthand/dictation.rs` (append `resolve_push_to_talk`)
- Modify: `src-tauri/src/transcription_coordinator.rs:78-80` (`is_transcribe_binding`) and its test module
- Modify: `src-tauri/src/actions.rs:1142-1163` (`ACTION_MAP`)
- Modify: `src-tauri/src/shortcut/handler.rs:29-45` (`handle_shortcut_event`)
- Modify: `src-tauri/src/shortcut/mod.rs:252-265` (`resume_all_shortcuts`)
- Modify: `src-tauri/src/shortcut/mod.rs:435-497` (`register_all_shortcuts_for_implementation`)
- Modify: `src-tauri/src/shortcut/tauri_impl.rs:17-40` (`init_shortcuts`)
- Modify: `src-tauri/src/shortcut/handy_keys.rs:425-458` (`init_shortcuts`)
- Modify: `src-tauri/src/secure_input.rs:513-534` (`reconcile_fallback`)

**Interfaces:**

- Consumes: `crate::shorthand::mode::{Mode, mode_for_binding}` (Task 1)
- Produces: `pub fn resolve_push_to_talk(settings: &AppSettings, binding_id: &str) -> bool` (`shorthand::dictation`), consumed by `shortcut/handler.rs` in this task and by nothing later. Two new binding ids, `"dictate"` and `"dictate_with_post_process"`, that Tasks 3-7 assume exist in `AppSettings::bindings` and in `ACTION_MAP`.

- [ ] **Step 1: Write the failing test for the five default bindings' shortcuts**

In `src-tauri/src/settings.rs`, add to the `#[cfg(test)] mod tests` block, near `default_settings_disable_saving_recordings_and_transcripts`:

```rust
    /// The five default bindings must not collide with each other on this
    /// platform. `cfg` means this only covers the host platform; the other
    /// two are a review-time reading of the `cfg` branches in
    /// `get_default_settings`, not a test.
    #[test]
    fn default_bindings_have_distinct_shortcuts_on_this_platform() {
        let bindings = get_default_settings().bindings;
        let ids = [
            "transcribe",
            "transcribe_with_post_process",
            "dictate",
            "dictate_with_post_process",
            "cancel",
        ];
        let mut shortcuts: Vec<&str> = ids
            .iter()
            .map(|id| {
                bindings
                    .get(*id)
                    .unwrap_or_else(|| panic!("missing default binding '{id}'"))
                    .current_binding
                    .as_str()
            })
            .collect();
        let before_dedup = shortcuts.len();
        shortcuts.sort_unstable();
        shortcuts.dedup();
        assert_eq!(
            shortcuts.len(),
            before_dedup,
            "default shortcuts must be pairwise distinct on this platform"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test settings::tests::default_bindings_have_distinct_shortcuts_on_this_platform`
Expected: FAIL — panics with `missing default binding 'dictate'` (the binding doesn't exist yet).

- [ ] **Step 3: Add the dictation shortcut defaults and rename meeting's**

In `src-tauri/src/settings.rs`, `get_default_settings()` currently starts (lines 815-865):

```rust
pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );
```

Replace it with (this both moves meeting's default shortcuts off Handy's combos and adds the two dictation bindings on Handy's old combos, so dictation and a plain Handy install can share muscle memory):

```rust
pub fn get_default_settings() -> AppSettings {
    // Dictation mode (below) takes Handy's original combos, so meeting mode
    // moves off them — that lets this fork and a plain Handy install run
    // side by side during a transition.
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+alt+space";
    #[cfg(target_os = "macos")]
    let default_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+alt+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_post_process_shortcut = "ctrl+alt+shift+space";
    #[cfg(target_os = "macos")]
    let default_post_process_shortcut = "ctrl+shift+option+space";
    #[cfg(target_os = "linux")]
    let default_post_process_shortcut = "ctrl+alt+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_post_process_shortcut = "alt+shift+space";

    bindings.insert(
        "transcribe_with_post_process".to_string(),
        ShortcutBinding {
            id: "transcribe_with_post_process".to_string(),
            name: "Transcribe with Post-Processing".to_string(),
            description: "Converts your speech into text and applies AI post-processing."
                .to_string(),
            default_binding: default_post_process_shortcut.to_string(),
            current_binding: default_post_process_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    // Dictation takes the combos meeting mode used before this fork added
    // dictation, so muscle memory from plain Handy transfers.
    #[cfg(target_os = "windows")]
    let default_dictate_shortcut = "ctrl+space";
    #[cfg(target_os = "macos")]
    let default_dictate_shortcut = "option+space";
    #[cfg(target_os = "linux")]
    let default_dictate_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_dictate_shortcut = "ctrl+alt+space";

    bindings.insert(
        "dictate".to_string(),
        ShortcutBinding {
            id: "dictate".to_string(),
            name: "Dictate".to_string(),
            description: "Converts your speech into text and pastes it into the focused window."
                .to_string(),
            default_binding: default_dictate_shortcut.to_string(),
            current_binding: default_dictate_shortcut.to_string(),
        },
    );
    #[cfg(target_os = "windows")]
    let default_dictate_post_process_shortcut = "ctrl+shift+space";
    #[cfg(target_os = "macos")]
    let default_dictate_post_process_shortcut = "option+shift+space";
    #[cfg(target_os = "linux")]
    let default_dictate_post_process_shortcut = "ctrl+shift+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_dictate_post_process_shortcut = "ctrl+alt+shift+space";

    bindings.insert(
        "dictate_with_post_process".to_string(),
        ShortcutBinding {
            id: "dictate_with_post_process".to_string(),
            name: "Dictate with Post-Processing".to_string(),
            description:
                "Converts your speech into text, applies AI post-processing, and pastes it into the focused window."
                    .to_string(),
            default_binding: default_dictate_post_process_shortcut.to_string(),
            current_binding: default_dictate_post_process_shortcut.to_string(),
        },
    );
```

The rest of `get_default_settings()` (the `AppSettings { .. }` literal) is unchanged by this step.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test settings::tests::default_bindings_have_distinct_shortcuts_on_this_platform`
Expected: PASS

- [ ] **Step 5: Write the failing test for `resolve_push_to_talk`**

In `src-tauri/src/shorthand/dictation.rs`, add the import and the test. Change the top of the file:

```rust
use crate::settings::{AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool};
```

to:

```rust
use super::mode::{self, Mode};
use crate::settings::{
    AppSettings, AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool,
};
```

And add to the `#[cfg(test)] mod tests` block, alongside `default_paste_method_is_not_none`:

```rust
    #[test]
    fn resolve_push_to_talk_reads_the_matching_mode_field() {
        let mut settings = crate::settings::get_default_settings();
        settings.push_to_talk = false;
        settings.dictation.push_to_talk = true;

        assert!(!resolve_push_to_talk(&settings, "transcribe"));
        assert!(!resolve_push_to_talk(&settings, "cancel"));
        assert!(resolve_push_to_talk(&settings, "dictate"));
        assert!(resolve_push_to_talk(&settings, "dictate_with_post_process"));
    }
```

- [ ] **Step 6: Run the test to verify it fails**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::resolve_push_to_talk_reads_the_matching_mode_field`
Expected: FAIL to compile — `cannot find function 'resolve_push_to_talk' in this scope`.

- [ ] **Step 7: Implement `resolve_push_to_talk`**

Add to `src-tauri/src/shorthand/dictation.rs`, below the `Default` impl and above `#[cfg(test)]`:

```rust
/// Whether push-to-talk applies to `binding_id`'s capture. Read at dispatch
/// time in `shortcut::handler::handle_shortcut_event`, before
/// `TranscribeAction::start` runs — so, unlike every other resolver in this
/// module, it cannot go through the mode cell (`mode::active` isn't updated
/// for this press yet). It derives the mode from `binding_id` directly
/// instead, the same way `mode::set_active` will a moment later.
pub fn resolve_push_to_talk(settings: &AppSettings, binding_id: &str) -> bool {
    match mode::mode_for_binding(binding_id) {
        Mode::Dictation => settings.dictation.push_to_talk,
        Mode::Meeting => settings.push_to_talk,
    }
}
```

- [ ] **Step 8: Run the test to verify it passes**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::resolve_push_to_talk_reads_the_matching_mode_field`
Expected: PASS

- [ ] **Step 9: Write the failing test for `is_transcribe_binding`**

In `src-tauri/src/transcription_coordinator.rs`, add to the existing `#[cfg(test)] mod tests` block (near the top, alongside the other small unit tests):

```rust
    #[test]
    fn is_transcribe_binding_recognises_meeting_and_dictation_bindings() {
        assert!(is_transcribe_binding("transcribe"));
        assert!(is_transcribe_binding("transcribe_with_post_process"));
        assert!(is_transcribe_binding("dictate"));
        assert!(is_transcribe_binding("dictate_with_post_process"));
        assert!(!is_transcribe_binding("cancel"));
        assert!(!is_transcribe_binding("test"));
    }
```

- [ ] **Step 10: Run the test to verify it fails**

Run: `cd src-tauri && cargo test transcription_coordinator::tests::is_transcribe_binding_recognises_meeting_and_dictation_bindings`
Expected: FAIL — `assertion failed: is_transcribe_binding("dictate")` (the function doesn't know the id yet, so it returns `false`).

- [ ] **Step 11: Update `is_transcribe_binding`**

In `src-tauri/src/transcription_coordinator.rs`, this exact function (lines 78-80):

```rust
pub fn is_transcribe_binding(id: &str) -> bool {
    id == "transcribe" || id == "transcribe_with_post_process"
}
```

Change to:

```rust
pub fn is_transcribe_binding(id: &str) -> bool {
    matches!(
        id,
        "transcribe" | "transcribe_with_post_process" | "dictate" | "dictate_with_post_process"
    )
}
```

This is not optional: `handle_shortcut_event` routes anything failing this check straight to `ACTION_MAP`'s bare press/release path, bypassing the coordinator's state machine — which would let a dictation press race a meeting-mode `stop_recording` against the same `AudioRecordingManager`.

- [ ] **Step 12: Run the test to verify it passes**

Run: `cd src-tauri && cargo test transcription_coordinator::tests::is_transcribe_binding_recognises_meeting_and_dictation_bindings`
Expected: PASS

- [ ] **Step 13: Add the two `ACTION_MAP` entries**

In `src-tauri/src/actions.rs`, `ACTION_MAP` currently reads (lines 1142-1163):

```rust
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
```

Insert the two new entries between the `transcribe_with_post_process` and `cancel` inserts:

```rust
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "transcribe_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "dictate".to_string(),
        Arc::new(TranscribeAction {
            post_process: false,
        }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "dictate_with_post_process".to_string(),
        Arc::new(TranscribeAction { post_process: true }) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
```

`TranscribeAction` keeps its exact shape — dictation reuses it unchanged, distinguishing only via `binding_id` (which mode resolves from, per Task 1's `mode_for_binding`).

- [ ] **Step 14: Resolve push-to-talk per binding in the dispatcher**

In `src-tauri/src/shortcut/handler.rs`, this exact block (lines 35-45):

```rust
    let settings = get_settings(app);

    // Transcribe bindings are handled by the coordinator.
    if is_transcribe_binding(binding_id) {
        if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
            coordinator.send_input(binding_id, hotkey_string, is_pressed, settings.push_to_talk);
        } else {
            warn!("TranscriptionCoordinator is not initialized");
        }
        return;
    }
```

Change to:

```rust
    let settings = get_settings(app);

    // Transcribe bindings are handled by the coordinator.
    if is_transcribe_binding(binding_id) {
        if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
            let push_to_talk =
                crate::shorthand::dictation::resolve_push_to_talk(&settings, binding_id);
            coordinator.send_input(binding_id, hotkey_string, is_pressed, push_to_talk);
        } else {
            warn!("TranscriptionCoordinator is not initialized");
        }
        return;
    }
```

- [ ] **Step 15: Add the `dictate` skip guard to `resume_all_shortcuts`**

In `src-tauri/src/shortcut/mod.rs`, this exact block (lines 252-265):

```rust
pub fn resume_all_shortcuts(app: &AppHandle) {
    let settings = get_settings(app);
    for (id, binding) in &settings.bindings {
        if id == "cancel" {
            continue;
        }
        if id == "transcribe_with_post_process" && !settings.post_process_enabled {
            continue;
        }
        if let Err(e) = register_shortcut(app, binding.clone()) {
            debug!("resume_all_shortcuts: could not register '{}': {}", id, e);
        }
    }
}
```

Change to:

```rust
pub fn resume_all_shortcuts(app: &AppHandle) {
    let settings = get_settings(app);
    for (id, binding) in &settings.bindings {
        if id == "cancel" {
            continue;
        }
        if id == "transcribe_with_post_process" && !settings.post_process_enabled {
            continue;
        }
        if id == "dictate" && !settings.dictation.enabled {
            continue;
        }
        if id == "dictate_with_post_process"
            && !(settings.dictation.enabled && settings.dictation.post_process_enabled)
        {
            continue;
        }
        if let Err(e) = register_shortcut(app, binding.clone()) {
            debug!("resume_all_shortcuts: could not register '{}': {}", id, e);
        }
    }
}
```

- [ ] **Step 16: Add the same skip guard to `register_all_shortcuts_for_implementation`**

In `src-tauri/src/shortcut/mod.rs`, this exact block (lines 449-452, inside `register_all_shortcuts_for_implementation`, which uses `current_settings` rather than `settings`):

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !current_settings.post_process_enabled {
            continue;
        }
```

Change to:

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !current_settings.post_process_enabled {
            continue;
        }
        if id == "dictate" && !current_settings.dictation.enabled {
            continue;
        }
        if id == "dictate_with_post_process"
            && !(current_settings.dictation.enabled
                && current_settings.dictation.post_process_enabled)
        {
            continue;
        }
```

- [ ] **Step 17: Add the skip guard to `tauri_impl::init_shortcuts`**

In `src-tauri/src/shortcut/tauri_impl.rs`, this exact block (lines 26-29, inside `init_shortcuts`, which uses `user_settings`):

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
```

Change to:

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
        if id == "dictate" && !user_settings.dictation.enabled {
            continue;
        }
        if id == "dictate_with_post_process"
            && !(user_settings.dictation.enabled && user_settings.dictation.post_process_enabled)
        {
            continue;
        }
```

- [ ] **Step 18: Add the skip guard to `handy_keys::init_shortcuts`**

In `src-tauri/src/shortcut/handy_keys.rs`, this exact block (lines 436-439, inside `init_shortcuts`, which also uses `user_settings`):

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
```

Change to:

```rust
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
        if id == "dictate" && !user_settings.dictation.enabled {
            continue;
        }
        if id == "dictate_with_post_process"
            && !(user_settings.dictation.enabled && user_settings.dictation.post_process_enabled)
        {
            continue;
        }
```

- [ ] **Step 19: Add the skip guard to `secure_input::reconcile_fallback`**

In `src-tauri/src/secure_input.rs`, this exact block (lines 524-529, inside `reconcile_fallback`, which uses `settings`):

```rust
                if id == "cancel" && !state.cancel_requested.load(Ordering::SeqCst) {
                    continue;
                }
                if id == "transcribe_with_post_process" && !settings.post_process_enabled {
                    continue;
                }
```

Change to:

```rust
                if id == "cancel" && !state.cancel_requested.load(Ordering::SeqCst) {
                    continue;
                }
                if id == "transcribe_with_post_process" && !settings.post_process_enabled {
                    continue;
                }
                if id == "dictate" && !settings.dictation.enabled {
                    continue;
                }
                if id == "dictate_with_post_process"
                    && !(settings.dictation.enabled && settings.dictation.post_process_enabled)
                {
                    continue;
                }
```

This is what makes "off by default" true rather than merely unreachable from the UI: without these five guards, the vacant-key merge in `get_settings` would still add `dictate`/`dictate_with_post_process` to every existing store, and every init/resume/reconcile path would happily register them even though `dictation.enabled` is `false`.

- [ ] **Step 20: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 21: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/shorthand/dictation.rs src-tauri/src/transcription_coordinator.rs src-tauri/src/actions.rs src-tauri/src/shortcut/handler.rs src-tauri/src/shortcut/mod.rs src-tauri/src/shortcut/tauri_impl.rs src-tauri/src/shortcut/handy_keys.rs src-tauri/src/secure_input.rs
git commit -m "feat(shorthand): wire dictation bindings and dispatch, gated off by default

Registers the two dictation bindings and routes them through the same
coordinator and push-to-talk logic as meeting mode, but every registration
path skips them until dictation.enabled (and, for the post-process binding,
dictation.post_process_enabled) is turned on — nothing user-visible changes
until Task 7 adds a way to turn it on."
```

---

### Task 3: Delivery resolver + follow-stream gate

**Files:**

- Modify: `src-tauri/src/shorthand/dictation.rs` (append `apply_mode`, `resolve_settings`, and their tests)
- Modify: `src-tauri/src/follow_stream/hub.rs` (add the no-active-session pinning test)
- Modify: `src-tauri/src/clipboard.rs:4` (import) and `:724-725` (the resolver swap)
- Modify: `src-tauri/src/actions.rs:547-550` (`set_active` call) and `:595-597` (the `hub.begin()` guard)

**Interfaces:**

- Consumes: `crate::shorthand::mode::{Mode, active, set_active}` (Task 1); `crate::settings::AppSettings` and its `PasteMethod`/`ClipboardHandling`/`AutoSubmitKey`/`TypingTool` fields
- Produces:
  - `pub fn apply_mode(settings: AppSettings, mode: Mode) -> AppSettings`
  - `pub fn resolve_settings(app: &AppHandle) -> AppSettings`

  Both are consumed by Tasks 4, 5, and 6 (which only add call sites — `apply_mode`'s field coverage is complete after this task and is not edited again).

- [ ] **Step 1: Write the failing cross-talk-guard tests for `apply_mode`**

Add to the `#[cfg(test)] mod tests` block in `src-tauri/src/shorthand/dictation.rs`:

```rust
    #[test]
    fn apply_mode_leaves_every_field_unchanged_for_meeting() {
        let mut settings = crate::settings::get_default_settings();
        settings.push_to_talk = false;
        settings.paste_method = PasteMethod::CtrlV;
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.auto_submit = true;
        settings.auto_submit_key = AutoSubmitKey::CtrlEnter;
        settings.append_trailing_space = true;
        settings.typing_tool = TypingTool::Wtype;
        settings.overlay_style = OverlayStyle::Live;
        settings.save_recordings = true;
        settings.save_transcripts = true;
        settings.post_process_enabled = true;
        settings.post_process_selected_prompt_id = Some("meeting-prompt".to_string());

        // Deliberately different from every field above, so a leak from
        // `dictation` into the Meeting-mode result would be visible.
        settings.dictation.push_to_talk = false;
        settings.dictation.paste_method = PasteMethod::None;
        settings.dictation.clipboard_handling = ClipboardHandling::DontModify;
        settings.dictation.auto_submit = false;
        settings.dictation.auto_submit_key = AutoSubmitKey::Enter;
        settings.dictation.append_trailing_space = false;
        settings.dictation.typing_tool = TypingTool::Auto;
        settings.dictation.overlay_style = OverlayStyle::Minimal;
        settings.dictation.save_recordings = false;
        settings.dictation.save_transcripts = false;
        settings.dictation.post_process_enabled = false;
        settings.dictation.post_process_selected_prompt_id =
            Some("dictation-prompt".to_string());

        let result = apply_mode(settings, Mode::Meeting);

        assert!(!result.push_to_talk);
        assert_eq!(result.paste_method, PasteMethod::CtrlV);
        assert_eq!(result.clipboard_handling, ClipboardHandling::CopyToClipboard);
        assert!(result.auto_submit);
        assert_eq!(result.auto_submit_key, AutoSubmitKey::CtrlEnter);
        assert!(result.append_trailing_space);
        assert_eq!(result.typing_tool, TypingTool::Wtype);
        assert_eq!(result.overlay_style, OverlayStyle::Live);
        assert!(result.save_recordings);
        assert!(result.save_transcripts);
        assert!(result.post_process_enabled);
        assert_eq!(
            result.post_process_selected_prompt_id,
            Some("meeting-prompt".to_string())
        );
    }

    #[test]
    fn apply_mode_overrides_every_per_mode_field_for_dictation() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_model = "whisper-large-v3-turbo".to_string();
        settings.push_to_talk = false;
        settings.paste_method = PasteMethod::None;
        settings.clipboard_handling = ClipboardHandling::DontModify;
        settings.auto_submit = false;
        settings.auto_submit_key = AutoSubmitKey::Enter;
        settings.append_trailing_space = false;
        settings.typing_tool = TypingTool::Auto;
        settings.overlay_style = OverlayStyle::None;
        settings.save_recordings = false;
        settings.save_transcripts = false;
        settings.post_process_enabled = false;
        settings.post_process_selected_prompt_id = None;

        settings.dictation.push_to_talk = true;
        settings.dictation.paste_method = PasteMethod::CtrlV;
        settings.dictation.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.dictation.auto_submit = true;
        settings.dictation.auto_submit_key = AutoSubmitKey::CmdEnter;
        settings.dictation.append_trailing_space = true;
        settings.dictation.typing_tool = TypingTool::Ydotool;
        settings.dictation.overlay_style = OverlayStyle::Minimal;
        settings.dictation.save_recordings = true;
        settings.dictation.save_transcripts = true;
        settings.dictation.post_process_enabled = true;
        settings.dictation.post_process_selected_prompt_id =
            Some("dictation-prompt".to_string());

        let result = apply_mode(settings, Mode::Dictation);

        assert!(result.push_to_talk);
        assert_eq!(result.paste_method, PasteMethod::CtrlV);
        assert_eq!(result.clipboard_handling, ClipboardHandling::CopyToClipboard);
        assert!(result.auto_submit);
        assert_eq!(result.auto_submit_key, AutoSubmitKey::CmdEnter);
        assert!(result.append_trailing_space);
        assert_eq!(result.typing_tool, TypingTool::Ydotool);
        assert_eq!(result.overlay_style, OverlayStyle::Minimal);
        assert!(result.save_recordings);
        assert!(result.save_transcripts);
        assert!(result.post_process_enabled);
        assert_eq!(
            result.post_process_selected_prompt_id,
            Some("dictation-prompt".to_string())
        );
        // A field `apply_mode` does not own must survive from the base settings.
        assert_eq!(result.selected_model, "whisper-large-v3-turbo");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::apply_mode`
Expected: FAIL to compile — `cannot find function 'apply_mode' in this scope`.

- [ ] **Step 3: Implement `apply_mode` and `resolve_settings`**

Add `use tauri::AppHandle;` to the top of `src-tauri/src/shorthand/dictation.rs`, alongside the existing `use` lines:

```rust
use super::mode::{self, Mode};
use crate::settings::{
    AppSettings, AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
```

Then add, below `resolve_push_to_talk` and above `#[cfg(test)]`:

```rust
/// Pure and unit-testable. Returns `settings` unchanged for `Mode::Meeting`;
/// for `Mode::Dictation` returns a copy with the per-mode fields overridden
/// from `settings.dictation`. Because this returns a full `AppSettings`, its
/// callers (`clipboard::paste`, `overlay::show_overlay_state`, and the reads
/// in `actions.rs`) each change one line — `get_settings(x)` becomes
/// `resolve_settings(x)` — instead of taking a narrower struct that would
/// force real edits into their bodies.
pub fn apply_mode(settings: AppSettings, mode: Mode) -> AppSettings {
    match mode {
        Mode::Meeting => settings,
        Mode::Dictation => {
            let dictation = settings.dictation.clone();
            AppSettings {
                push_to_talk: dictation.push_to_talk,
                paste_method: dictation.paste_method,
                clipboard_handling: dictation.clipboard_handling,
                auto_submit: dictation.auto_submit,
                auto_submit_key: dictation.auto_submit_key,
                append_trailing_space: dictation.append_trailing_space,
                typing_tool: dictation.typing_tool,
                overlay_style: dictation.overlay_style,
                save_recordings: dictation.save_recordings,
                save_transcripts: dictation.save_transcripts,
                post_process_enabled: dictation.post_process_enabled,
                post_process_selected_prompt_id: dictation.post_process_selected_prompt_id,
                ..settings
            }
        }
    }
}

/// `apply_mode(get_settings(app), mode::active(app))` — the one call every
/// per-mode resolver in the upstream call sites makes.
pub fn resolve_settings(app: &AppHandle) -> AppSettings {
    apply_mode(crate::settings::get_settings(app), mode::active(app))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test shorthand::dictation::tests::apply_mode`
Expected: PASS

- [ ] **Step 5: Pin the follow-stream no-active-session behavior**

In `src-tauri/src/follow_stream/hub.rs`, add a new test to the existing `#[cfg(test)] mod tests` block, right after `partial_without_an_active_session_emits_nothing`:

```rust
    #[test]
    fn finish_no_speech_and_partial_without_begin_broadcast_nothing() {
        // Dictation must never reach the follow-stream hub: `TranscribeAction::start`
        // skips `hub.begin()` for a dictation capture (see the actions.rs change in
        // this task), and this test pins the consequence that makes that single skip
        // sufficient — every other hub call is already a silent no-op without a
        // preceding `begin`. If a later refactor to `finish_with` or `partial` ever
        // breaks that, this test catches it even though nothing here calls `begin`.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.finish(Some(Speaker::Me), "orphaned final");
        hub.no_speech();
        hub.partial(StreamSource::Mic, "ignored", "");

        assert!(follower.drain().is_empty());
    }
```

This behavior already exists in `finish_with` (`let Some(active) = state.active.take() else { return };`) and `partial` — this step only adds test coverage, so it is not expected to fail first.

Run: `cd src-tauri && cargo test follow_stream::hub::tests::finish_no_speech_and_partial_without_begin_broadcast_nothing`
Expected: PASS immediately.

- [ ] **Step 6: Swap `clipboard::paste`'s settings read for the resolver**

In `src-tauri/src/clipboard.rs`, the import (line 4) currently reads:

```rust
use crate::settings::{get_settings, AutoSubmitKey, ClipboardHandling, PasteMethod};
```

Change to (dropping `get_settings`, whose only call site in this file is about to move):

```rust
use crate::settings::{AutoSubmitKey, ClipboardHandling, PasteMethod};
```

And `paste()` currently starts (line 724-725):

```rust
pub fn paste(text: String, app_handle: AppHandle) -> Result<(), String> {
    let settings = get_settings(&app_handle);
```

Change to:

```rust
pub fn paste(text: String, app_handle: AppHandle) -> Result<(), String> {
    let settings = crate::shorthand::dictation::resolve_settings(&app_handle);
```

Every other line of `paste()` is untouched — `settings.paste_method`, `.clipboard_handling`, `.auto_submit`, `.auto_submit_key`, `.append_trailing_space`, and `.typing_tool` now come from whichever mode is active; `.paste_delay_ms`, `.paste_delay_after_ms`, `.reliable_paste`, and `.external_script_path` are untouched because `apply_mode` never overrides them.

- [ ] **Step 7: Call `set_active` and gate `hub.begin()` in `TranscribeAction::start`**

In `src-tauri/src/actions.rs`, `start` currently begins (lines 547-550):

```rust
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
```

Change to:

```rust
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);
        crate::shorthand::mode::set_active(app, binding_id);

        // Load model in the background
```

And, later in the same function, this exact block (lines 595-597):

```rust
        if let Some(hub) = crate::follow_stream::hub(app) {
            hub.begin(model_supports_streaming);
        }
```

Change to:

```rust
        // A dictation capture must never reach the follow-stream hub. Skipping
        // `begin` alone is sufficient: every terminal hub call and `partial`
        // check for an active session first and silently no-op without one
        // (pinned in follow_stream::hub::tests).
        if crate::shorthand::mode::active(app) == crate::shorthand::mode::Mode::Meeting {
            if let Some(hub) = crate::follow_stream::hub(app) {
                hub.begin(model_supports_streaming);
            }
        }
```

- [ ] **Step 8: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/shorthand/dictation.rs src-tauri/src/follow_stream/hub.rs src-tauri/src/clipboard.rs src-tauri/src/actions.rs
git commit -m "feat(shorthand): resolve delivery settings through the mode cell and silence follow-stream for dictation

clipboard::paste now reads paste method, clipboard handling, auto-submit, and
trailing-space through apply_mode instead of raw settings, so a dictation
capture delivers text the way dictation is configured rather than the way
meeting mode is. Meeting mode is unaffected: apply_mode is the identity
function for it. A dictation capture also never opens a follow-stream
session, since it carries only the user's own voice."
```

---

### Task 4: Overlay resolver

**Files:**

- Modify: `src-tauri/src/actions.rs:617` (overlay-style match in `start`)
- Modify: `src-tauri/src/actions.rs:749` (overlay-style read in `stop`)
- Modify: `src-tauri/src/overlay.rs:485` (overlay-style read in `show_overlay_state`)

**Interfaces:**

- Consumes: `crate::shorthand::dictation::resolve_settings` (Task 3) — unchanged by this task, only given two more call sites.

`apply_mode`'s field coverage (Task 3) already includes `overlay_style`, so this task has no new pure logic to test-drive — it is a three-site call-site swap, verified by the regression suite and by the manual overlay check in the later UI increments (out of this plan's scope).

- [ ] **Step 1: Swap the overlay-style read in `TranscribeAction::start`**

In `src-tauri/src/actions.rs`, this exact line (line 617):

```rust
        match settings.overlay_style {
```

Change to:

```rust
        match crate::shorthand::dictation::resolve_settings(app).overlay_style {
```

(`settings` — the plain `get_settings(app)` from earlier in `start` — is still used for `is_always_on`, `selected_model_info`, and `vad_policy` on the surrounding lines; those are unaffected because `apply_mode` never touches `always_on_microphone`, `selected_model`, or `vad_enabled`.)

- [ ] **Step 2: Swap the overlay-style read in `TranscribeAction::stop`**

In `src-tauri/src/actions.rs`, this exact line (line 749):

```rust
        let style = get_settings(app).overlay_style;
```

Change to:

```rust
        let style = crate::shorthand::dictation::resolve_settings(app).overlay_style;
```

- [ ] **Step 3: Swap the overlay-style read in `overlay::show_overlay_state`**

In `src-tauri/src/overlay.rs`, this exact block (lines 481-488):

```rust
fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Whether the overlay shows at all is governed by overlay_style; position
    // only chooses Top vs Bottom placement. Checked here (off the main thread)
    // so the common overlay-disabled case never pays for a main-thread hop.
    let settings = settings::get_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }
```

Change to:

```rust
fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Whether the overlay shows at all is governed by overlay_style; position
    // only chooses Top vs Bottom placement. Checked here (off the main thread)
    // so the common overlay-disabled case never pays for a main-thread hop.
    let settings = crate::shorthand::dictation::resolve_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }
```

Without this, setting dictation's overlay to `None` while meeting's stays `Live` would still flash a processing overlay mid-dictation, because this function reads `overlay_style` on every state transition purely to decide whether to early-return.

- [ ] **Step 4: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/actions.rs src-tauri/src/overlay.rs
git commit -m "feat(shorthand): resolve overlay style through the mode cell

All three overlay_style reads (the two in actions.rs sizing/switching the
overlay, and the one in show_overlay_state deciding whether to show it at
all) now consult apply_mode, so dictation's compact-pill default can differ
from meeting's live-transcript default without either mode's overlay
flashing the other's style mid-capture."
```

---

### Task 5: Save-toggle resolver

**Files:**

- Modify: `src-tauri/src/actions.rs:815` (`persistence_settings` read in `TranscribeAction::stop`'s async task)

**Interfaces:**

- Consumes: `crate::shorthand::dictation::resolve_settings` (Task 3) — unchanged, given one more call site.

`apply_mode` already covers `save_recordings` and `save_transcripts` (Task 3). `HistoryManager::save_entry` reading the mode cell for the `source` column is Task 10 (History work, out of this plan's scope) — this task only makes the persistence _toggles_ per-mode; History rows stay unlabeled until Task 10 lands.

- [ ] **Step 1: Swap the persistence-settings read**

In `src-tauri/src/actions.rs`, this exact block (lines 810-817):

```rust
                    // Persistence toggles: whether to keep the WAV on disk and
                    // whether to keep the transcript text in history. These
                    // govern persistence only; delivery (paste/clipboard/
                    // follow-stream) happens unconditionally below regardless
                    // of either flag.
                    let persistence_settings = get_settings(&ah);
                    let save_recordings = persistence_settings.save_recordings;
                    let save_transcripts = persistence_settings.save_transcripts;
```

Change to:

```rust
                    // Persistence toggles: whether to keep the WAV on disk and
                    // whether to keep the transcript text in history. These
                    // govern persistence only; delivery (paste/clipboard/
                    // follow-stream) happens unconditionally below regardless
                    // of either flag.
                    let persistence_settings = crate::shorthand::dictation::resolve_settings(&ah);
                    let save_recordings = persistence_settings.save_recordings;
                    let save_transcripts = persistence_settings.save_transcripts;
```

`persistence_settings` is used only for these two fields in this scope, so this is a self-contained swap.

- [ ] **Step 2: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/actions.rs
git commit -m "feat(shorthand): resolve save-recording toggles through the mode cell

save_recordings and save_transcripts now come from apply_mode, so turning on
dictation's transcript-saving does not also start saving meeting audio (and
vice versa) — consent for capturing other people's voices in a meeting stays
a separate decision from consent for the user's own voice in dictation."
```

---

### Task 6: Post-process resolver and prompt

**Files:**

- Modify: `src-tauri/src/actions.rs:505` (`settings` read in `process_transcription_output`)

**Interfaces:**

- Consumes: `crate::shorthand::dictation::resolve_settings` (Task 3) — unchanged, given one more call site.

`apply_mode` already covers `post_process_enabled` and `post_process_selected_prompt_id` (Task 3). `post_process_prompts` (the list itself) and the provider/API-key/model configuration stay shared and are untouched by `apply_mode`, so this single swap is sufficient for the whole function, including the nested call to `post_process_transcription`.

- [ ] **Step 1: Swap the settings read in `process_transcription_output`**

In `src-tauri/src/actions.rs`, this exact block (lines 500-506):

```rust
pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();
```

Change to:

```rust
pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
    post_process: bool,
) -> ProcessedTranscription {
    let settings = crate::shorthand::dictation::resolve_settings(app);
    let mut final_text = transcription.to_string();
```

`settings` here flows into `resolve_effective_language` (keys off `selected_language`, shared, untouched), into `post_process_transcription(&settings, &final_text)` (reads `post_process_selected_prompt_id`, now per-mode, and `post_process_prompts`/providers/models, still shared), and into the `settings.post_process_selected_prompt_id` read a few lines later for `post_process_prompt` — all three now see the resolved value with this one change.

- [ ] **Step 2: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/actions.rs
git commit -m "feat(shorthand): resolve the post-processing prompt through the mode cell

process_transcription_output now reads post_process_selected_prompt_id
through apply_mode, so dictation's cleanup prompt and meeting's summary
prompt (if either is set) stay independent even though both share the same
providers, API keys, and model choices."
```

---

### Task 7: Command and store wiring

**Files:**

- Modify: `src-tauri/src/shortcut/mod.rs` (append `change_dictation_settings`, after `change_save_transcripts_setting`)
- Modify: `src-tauri/src/lib.rs:731-732` (`collect_commands!` entry)
- Modify: `src/stores/settingsStore.ts:4-10` (type import) and settingUpdaters map
- Regenerate: `src/bindings.ts` (via debug build — never hand-edited)

**Interfaces:**

- Consumes: `crate::shorthand::dictation::DictationSettings` (Task 1); `crate::shortcut::{register_shortcut, unregister_shortcut}` (existing, `shortcut/mod.rs`); `crate::secure_input::reconcile_fallback` (existing)
- Produces: `#[tauri::command] #[specta::specta] pub fn change_dictation_settings(app: AppHandle, dictation: DictationSettings) -> Result<(), String>`, and the generated `commands.changeDictationSettings(dictation: DictationSettings): Promise<Result<null, string>>` TypeScript binding Tasks 8-9 (out of this plan's scope) will call from `DictationSettings.tsx`.

This task has no new pure logic to test-drive (the command's job is to persist a struct and keep the two dictation shortcuts' registration state in sync with it) — it is verified by `cargo build`, `cargo clippy`, and `bun run build`, plus the manual "restart, confirm every dictation setting persisted" check in the spec's later increments.

- [ ] **Step 1: Add the `change_dictation_settings` command**

In `src-tauri/src/shortcut/mod.rs`, `change_save_transcripts_setting` currently ends (lines 1326-1333):

```rust
#[tauri::command]
#[specta::specta]
pub fn change_save_transcripts_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.save_transcripts = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}
```

Add immediately after it:

```rust

#[tauri::command]
#[specta::specta]
pub fn change_dictation_settings(
    app: AppHandle,
    dictation: crate::shorthand::dictation::DictationSettings,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let was_registered = settings.dictation.enabled;
    let was_post_process_registered = was_registered && settings.dictation.post_process_enabled;

    settings.dictation = dictation;
    let now_registered = settings.dictation.enabled;
    let now_post_process_registered = now_registered && settings.dictation.post_process_enabled;
    settings::write_settings(&app, settings.clone());

    // Registering the two dictation shortcuts only at the next app start
    // would leave "Enable Dictation" doing nothing until a restart — the
    // exact "silently does nothing on first try" failure this feature must
    // avoid. Register/unregister immediately, mirroring
    // change_post_process_enabled_setting's handling of the meeting-mode
    // post-process binding.
    // The results are propagated rather than discarded. register_shortcut
    // returns Err when the combo is already claimed, and init_shortcuts only
    // error!-logs that — so a user whose chosen key is taken by another app
    // would otherwise see the toggle turn on and the key do nothing, with a
    // log line as the only explanation. Returning Err makes the store's
    // updateSetting revert its optimistic write, so the toggle visibly does
    // not stick, and Task 8's enable toggle turns that into a message.
    let mut failures: Vec<String> = Vec::new();

    if now_registered != was_registered {
        if let Some(binding) = settings.bindings.get("dictate").cloned() {
            let result = if now_registered {
                register_shortcut(&app, binding)
            } else {
                unregister_shortcut(&app, binding)
            };
            if let Err(e) = result {
                failures.push(e);
            }
        }
    }
    if now_post_process_registered != was_post_process_registered {
        if let Some(binding) = settings.bindings.get("dictate_with_post_process").cloned() {
            let result = if now_post_process_registered {
                register_shortcut(&app, binding)
            } else {
                unregister_shortcut(&app, binding)
            };
            if let Err(e) = result {
                failures.push(e);
            }
        }
    }

    crate::secure_input::reconcile_fallback(&app);

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}
```

- [ ] **Step 2: Register the command in `collect_commands!`**

In `src-tauri/src/lib.rs`, this exact line (line 731):

```rust
            shortcut::change_save_transcripts_setting,
```

Change to:

```rust
            shortcut::change_save_transcripts_setting,
            shortcut::change_dictation_settings,
```

- [ ] **Step 3: Run cargo build and cargo test**

Run: `cd src-tauri && cargo build`
Expected: builds cleanly.

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no new warnings

- [ ] **Step 4: Regenerate `src/bindings.ts`**

`src/bindings.ts` is generated by tauri-specta under `#[cfg(debug_assertions)]` and carries a do-not-edit header. Regenerate it with a debug build rather than hand-editing:

Run: `bun run tauri dev` (or any debug build that reaches the specta export step), then stop it once `src/bindings.ts` has been rewritten.

Confirm `DictationSettings` and `changeDictationSettings` now appear in `src/bindings.ts`.

- [ ] **Step 5: Add the `dictation` updater entry**

In `src/stores/settingsStore.ts`, the type import currently reads (lines 4-9):

```ts
import type {
  AppSettings as Settings,
  AudioDevice,
  TranscribeAcceleratorSetting,
  OrtAcceleratorSetting,
} from "@/bindings";
```

Change to:

```ts
import type {
  AppSettings as Settings,
  AudioDevice,
  TranscribeAcceleratorSetting,
  OrtAcceleratorSetting,
  DictationSettings,
} from "@/bindings";
```

And in `settingUpdaters`, add an entry alongside the other `Result<(), String>`-style updaters such as `save_recordings`/`save_transcripts` (near the end of the map, after `extra_recording_buffer_ms`):

```ts
  extra_recording_buffer_ms: (value) =>
    commands.changeExtraRecordingBufferSetting(value as number),
  dictation: (value) =>
    commands.changeDictationSettings(value as DictationSettings),
};
```

Step 5 is not optional: without an updater, `updateSetting("dictation", ...)` logs "No handler for setting" and the Dictation section's toggles would appear to work, then revert on reload — this is exactly what Task 9's UI (out of this plan's scope) will call once it exists.

- [ ] **Step 6: Run the frontend build**

Run: `bun run build`
Expected: builds cleanly (confirms `DictationSettings` is exported from `bindings.ts` and the updater map type-checks).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/shortcut/mod.rs src-tauri/src/lib.rs src/bindings.ts src/stores/settingsStore.ts
git commit -m "feat(shorthand): add the change_dictation_settings command and store wiring

Gives the (not-yet-built) Dictation settings UI a single command to persist
the whole DictationSettings struct at once, matching how the existing
post-processing settings already read a sub-struct, spread it, override one
key, and write the whole object back. Also keeps the two dictation shortcuts'
registration in sync immediately, rather than only at next launch."
```

### Task 8: UI — Dictation section skeleton (enable toggle, both shortcut rows, Accessibility row)

**Component-reuse decision (applies to Tasks 8–9):** every upstream setting-row
component this feature would want to reuse — `PasteMethod.tsx`,
`ClipboardHandling.tsx`, `AutoSubmit.tsx`, `AppendTrailingSpace.tsx`,
`TypingTool.tsx`, `ShowOverlay.tsx`, `SaveRecordings.tsx`,
`SaveTranscripts.tsx`, `PushToTalk.tsx`, `PostProcessingToggle.tsx` — hardcodes
its own `getSetting("<top_level_key>")` / `updateSetting("<top_level_key>",
...)` call. Confirmed by reading all ten. `useSettings`'s `getSetting` /
`updateSetting` (`src/hooks/useSettings.ts`) are `<K extends keyof Settings>`
only; there is no nested-path variant, and per the locked interface contract
`settingsStore.ts` has exactly one updater keyed `"dictation"`. So option (a)
— add an optional target prop to each upstream component — would mean editing
ten upstream files, each to grow a `value`/`onChange` escape hatch, for a
feature the fork owns entirely. Option (b) — fork-only sibling components
under `src/shorthand/dictation/` that render the same primitives
(`SettingContainer`, `SettingsGroup`, `ToggleSwitch`, `Dropdown`) directly
against `settings.dictation.*` via the read-spread-write pattern — costs zero
upstream edits and follows the boundary AGENTS.md already asks for
(`--follow-stream` is the model: "give fork-only features a boundary").
Taking (b) for all ten. The one field that only has one live consumer
(`ShortcutInput` → `GlobalShortcutInput`/`HandyKeysShortcutInput`) reads
`settings.bindings[shortcutId]`, not a per-mode struct, and takes
`shortcutId` as a prop already — it is reused verbatim, no sibling needed.
`AccessibilityPermissions.tsx` similarly takes no settings-scoped props at
all (it only calls the OS permission APIs) and is reused verbatim.

A generic `DictationToggleField` covers every boolean field this section
needs across Tasks 8–9 (`enabled`, `push_to_talk`, `append_trailing_space`,
`save_recordings`, `save_transcripts`, `post_process_enabled`) instead of six
near-identical sibling files.

**Files:**

- Create: `src/shorthand/dictation/DictationToggleField.tsx`
- Create: `src/shorthand/DictationSettings.tsx`
- Modify: `src/shorthand/sections.ts:1-30` (full file — see step 3)
- Modify: `src/shorthand/visibility.ts:30-36`

**Interfaces:**

- Consumes: `DictationSettings` type from `@/bindings` (locked contract:
  `enabled`, `push_to_talk`, `paste_method`, `clipboard_handling`,
  `auto_submit`, `auto_submit_key`, `append_trailing_space`, `typing_tool`,
  `overlay_style`, `save_recordings`, `save_transcripts`,
  `post_process_enabled`, `post_process_selected_prompt_id`); `AppSettings`
  field `dictation?: DictationSettings`; `settingsStore.ts` updater keyed
  `"dictation"` calling `commands.changeDictationSettings`; bindings ids
  `dictate` and `dictate_with_post_process` (registered in `settings.rs` by
  Task 1–7); `useSettings()` (`getSetting`, `updateSetting`, `isUpdating`);
  `ShortcutInput` (`src/components/settings/ShortcutInput.tsx`, prop
  `shortcutId: string`); `AccessibilityPermissions`
  (`src/components/AccessibilityPermissions.tsx`, default export, no props,
  self-hides off-macOS or once granted); `SettingsGroup`, `ToggleSwitch`
  (`src/components/ui/*`).
- Produces: `DictationToggleField` — props `field: "enabled" | "push_to_talk"
| "append_trailing_space" | "save_recordings" | "save_transcripts" |
"post_process_enabled"`, `label: string`, `description: string`,
  `descriptionMode?`, `grouped?`, `disabled?` — consumed by Task 9.
  `DictationSettings` React component registered in `SECTIONS_CONFIG` under
  id `"dictation"`, consumed by `App.tsx`/`Sidebar.tsx` through the existing
  `SHORTHAND_SECTIONS` spread (no edit to either file needed, per the
  settings-UI plan's Task 4/5 groundwork).

**Deviation from the spec's exact row grouping, flagged here:** the spec's
detailed "Settings UI" section lists group 2 (Shortcut) as _only_
`ShortcutInput shortcutId="dictate"` + `PushToTalk` + the Accessibility row,
and puts `ShortcutInput shortcutId="dictate_with_post_process"` inside group 4
(AI cleanup). But the spec's own Increments list says Task 8 delivers "enable
toggle, **both shortcuts**, Accessibility row" and Task 9 delivers "output,
overlay, save and post-process rows" — no second shortcut. Those two parts of
the same spec disagree with each other. This plan follows the Increments text
(and the task boundary given for this plan): both `ShortcutInput` rows render
in Task 8, inside the "Shortcut" group. Task 9's "AI Cleanup" group therefore
contains only the enable toggle, the prompt picker, and the hint text — no
shortcut row. If group-4-owns-the-second-shortcut is what's actually wanted,
moving that one `<ShortcutInput .../>` line from the Shortcut group to the AI
Cleanup group in `DictationSettings.tsx` is a one-line change, not a
restructure.

**Second gap, flagged here:** `DictationSettings` (the locked Rust struct) has
no `external_script_path` field, but `PasteMethod`'s `external_script` option
(Linux-only) needs one to store the script path. The spec's per-mode/shared
settings table never mentions `external_script_path` at all. Task 9's
`DictationPasteMethod` omits the `external_script` option entirely rather than
inventing a field the locked contract doesn't have, or silently sharing the
top-level path across modes (which would reintroduce the cross-talk the mode
cell exists to prevent). See Task 9.

- [ ] **Step 1: Create the generic boolean-field toggle sibling**

```tsx
// src/shorthand/dictation/DictationToggleField.tsx
import React from "react";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

type BooleanDictationField = {
  [K in keyof DictationSettings]: DictationSettings[K] extends boolean
    ? K
    : never;
}[keyof DictationSettings];

interface DictationToggleFieldProps {
  field: BooleanDictationField;
  label: string;
  description: string;
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling covering every boolean row in the Dictation section
 * (enabled, push_to_talk, append_trailing_space, save_recordings,
 * save_transcripts, post_process_enabled). Upstream's equivalent toggles
 * (PushToTalk.tsx, SaveRecordings.tsx, SaveTranscripts.tsx,
 * AppendTrailingSpace.tsx, PostProcessingToggle.tsx) each hardcode a
 * top-level getSetting/updateSetting key and cannot address
 * settings.dictation.*; useSettings's getSetting/updateSetting are
 * `keyof Settings` only (src/hooks/useSettings.ts), so there is no
 * nested-path alternative. This reimplements the same read-spread-write
 * pattern the rest of the fork uses for nested settings, without editing
 * any upstream component. `isUpdating("dictation")` covers the whole
 * struct, not just this field, since the store has one updater entry for
 * the entire nested object.
 */
export const DictationToggleField: React.FC<DictationToggleFieldProps> = ({
  field,
  label,
  description,
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const checked = (dictation?.[field] as boolean | undefined) ?? false;

  return (
    <ToggleSwitch
      checked={checked}
      onChange={(value) =>
        updateSetting("dictation", {
          ...dictation,
          [field]: value,
        } as DictationSettings)
      }
      isUpdating={isUpdating("dictation")}
      disabled={disabled}
      label={label}
      description={description}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
```

Then create the enable toggle, which needs behaviour the generic field does
not have. `change_dictation_settings` (Task 7) returns `Err` when a dictation
shortcut cannot be registered because another app already owns the combo. The
store's `updateSetting` catches that, logs to the console, and reverts its
optimistic write — so the toggle silently flips back with no explanation.
This component notices the revert and says what happened, which is the whole
point of propagating the error:

```tsx
// src/shorthand/dictation/DictationEnableToggle.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

/**
 * The "Enable Dictation" toggle.
 *
 * Enabling registers two global shortcuts, which fails when another app
 * already owns the combo. `register_shortcut` reports that, and
 * `change_dictation_settings` propagates it, but `updateSetting` swallows
 * the rejection and rolls the optimistic write back — leaving a toggle that
 * springs back to off for no visible reason. Comparing the requested value
 * against the persisted one after the update settles is the only signal the
 * store leaves us, so that is what this reads.
 */
export const DictationEnableToggle: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const enabled = dictation?.enabled ?? false;
  const [failed, setFailed] = React.useState(false);

  const handleChange = async (value: boolean) => {
    setFailed(false);
    await updateSetting("dictation", {
      ...dictation,
      enabled: value,
    } as DictationSettings);
    const persisted =
      (getSetting("dictation") as DictationSettings | undefined)?.enabled ??
      false;
    setFailed(value && !persisted);
  };

  return (
    <>
      <ToggleSwitch
        checked={enabled}
        onChange={handleChange}
        isUpdating={isUpdating("dictation")}
        label={t("settings.dictation.enable.label")}
        description={t("settings.dictation.enable.description")}
        descriptionMode="tooltip"
        grouped={true}
      />
      {failed && (
        <p className="px-4 pb-2 text-sm text-red-500">
          {t("settings.dictation.enable.shortcutConflict")}
        </p>
      )}
    </>
  );
};
```

- [ ] **Step 2: Create the Dictation section skeleton**

```tsx
// src/shorthand/DictationSettings.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";
import { DictationToggleField } from "./dictation/DictationToggleField";
import { DictationEnableToggle } from "./dictation/DictationEnableToggle";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Dictation" section: the opt-in dictation mode that runs
 * alongside meeting transcription, with its own shortcuts and settings. See
 * docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.
 *
 * Rows below the enable toggle stay mounted and are individually disabled
 * rather than hidden while dictation is off, so the section previews what
 * enabling buys instead of reading as empty/broken. The Accessibility row is
 * the one exception: AccessibilityPermissions has no disabled state of its
 * own (it either self-hides or offers a live Grant button), so it is gated
 * on dictationEnabled by not rendering it at all rather than by disabling it.
 *
 * Extended by Task 9 with Output, AI Cleanup, Privacy groups and a footer
 * line; see that task for the full replacement of this file's body.
 */
export const DictationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup>
        <DictationEnableToggle />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.shortcut")}>
        <ShortcutInput
          shortcutId="dictate"
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationToggleField
          field="push_to_talk"
          label={t("settings.general.pushToTalk.label")}
          description={t("settings.general.pushToTalk.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <ShortcutInput
          shortcutId="dictate_with_post_process"
          grouped={true}
          disabled={!dictationEnabled}
        />
      </SettingsGroup>

      {dictationEnabled && <AccessibilityPermissions />}
    </div>
  );
};
```

- [ ] **Step 3: Register the section in `src/shorthand/sections.ts`**

Replace the full file contents:

```ts
// src/shorthand/sections.ts
import { Mic, Captions, AppWindow, Keyboard } from "lucide-react";
import { CaptureSettings } from "./CaptureSettings";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { AppSettings } from "./AppSettings";
import { DictationSettings } from "./DictationSettings";

/**
 * Fork-only sidebar section configs (Capture, Transcription, App,
 * Dictation), kept out of `src/components/Sidebar.tsx`'s `SECTIONS_CONFIG`
 * so registering or changing these never conflicts with upstream's own
 * entries in that object. Spread into `SECTIONS_CONFIG` first so the app
 * opens on Capture by default; see `src/shorthand/visibility.ts` for how
 * these replace upstream's general/models/advanced/postprocessing sections
 * in the simplified profile.
 */
export const SHORTHAND_SECTIONS = {
  capture: {
    labelKey: "sidebar.capture",
    icon: Mic,
    component: CaptureSettings,
    enabled: () => true,
  },
  transcription: {
    labelKey: "sidebar.transcription",
    icon: Captions,
    component: TranscriptionSettings,
    enabled: () => true,
  },
  app: {
    labelKey: "sidebar.app",
    icon: AppWindow,
    component: AppSettings,
    enabled: () => true,
  },
  dictation: {
    labelKey: "sidebar.dictation",
    icon: Keyboard,
    component: DictationSettings,
    enabled: () => true,
  },
};
```

- [ ] **Step 4: Add `"dictation"` to `FORK_ONLY_SECTIONS`**

```ts
// src/shorthand/visibility.ts — replace the FORK_ONLY_SECTIONS block (lines ~30-36)
/**
 * Fork-only section ids hidden when `show_all_settings` is true. `dictation`
 * has no upstream equivalent to fall back to, so it disappears along with
 * capture/transcription/app when the escape hatch is on — same rule, same
 * reason: don't show two settings surfaces for the same concern at once.
 */
export const FORK_ONLY_SECTIONS: ReadonlySet<string> = new Set([
  "capture",
  "transcription",
  "app",
  "dictation",
]);
```

- [ ] **Step 5: Verify the skeleton builds and renders**

Run: `bun run lint`
Expected: no errors. `no-literal-string` passes because every user-facing
string in the new files goes through `t()` — the keys themselves
(`settings.dictation.enable.label`, `settings.dictation.groups.shortcut`,
etc.) don't exist in any locale file yet; that's Task 11, not a lint failure.

Run: `bun run build`
Expected: succeeds. `DictationSettings` typechecks against the
`DictationSettings` type from `@/bindings` (available once Task 7's
`bindings.ts` regeneration has landed).

Manual (debug build, `bun run tauri dev`): the sidebar shows a "Dictation"
entry (rendered as its raw i18n key or blank label until Task 11 — expected).
Clicking it shows: an enable toggle, a "Shortcut" group with the dictate
shortcut row, a push-to-talk toggle, and the dictate-with-post-process
shortcut row, all editable regardless of the enable toggle's state (row
`disabled` styling is cosmetic only — `SettingContainer`/`ToggleSwitch`/
`ShortcutInput` don't block interaction on `disabled`, they just dim). Toggle
"enabled" on: on macOS, an Accessibility permissions card appears/disappears
under the Shortcut group as the toggle flips; on Windows/Linux nothing
additional appears (`AccessibilityPermissions` self-hides off-macOS).

- [ ] **Step 6: Commit**

```bash
git add src/shorthand/dictation/DictationToggleField.tsx \
        src/shorthand/DictationSettings.tsx \
        src/shorthand/sections.ts \
        src/shorthand/visibility.ts
git commit -m "$(cat <<'EOF'
feat(shorthand): add Dictation settings section skeleton

Enable toggle, both dictation shortcut rows, and the macOS Accessibility
status row. Every row is a fork-only sibling bound to settings.dictation.*
rather than an edit to the upstream row components, which only know how to
address top-level settings keys.
EOF
)"
```

---

### Task 9: UI — output, overlay, save and post-process rows; `Sidebar.tsx` predicate change

**Files:**

- Create: `src/shorthand/dictation/DictationPasteMethod.tsx`
- Create: `src/shorthand/dictation/DictationClipboardHandling.tsx`
- Create: `src/shorthand/dictation/DictationAutoSubmit.tsx`
- Create: `src/shorthand/dictation/DictationTypingTool.tsx`
- Create: `src/shorthand/dictation/DictationShowOverlay.tsx`
- Create: `src/shorthand/dictation/DictationPostProcessPrompt.tsx`
- Modify: `src/shorthand/DictationSettings.tsx` (full replacement — see step 7)
- Modify: `src/components/Sidebar.tsx:66`

**Interfaces:**

- Consumes: `DictationToggleField` (Task 8); `PasteMethod`, `ClipboardHandling`,
  `AutoSubmitKey`, `TypingTool`, `OverlayStyle`, `OverlayPosition`, `LLMPrompt`
  types from `@/bindings` (all already exported today — confirmed in
  `src/bindings.ts`: `PasteMethod = "ctrl_v" | "direct" | "none" |
"shift_insert" | "ctrl_shift_v" | "external_script"`, `ClipboardHandling =
"dont_modify" | "copy_to_clipboard"`, `AutoSubmitKey = "enter" |
"ctrl_enter" | "cmd_enter"`, `TypingTool = "auto" | "wtype" | "kwtype" |
"dotool" | "ydotool" | "xdotool"`, `OverlayStyle = "none" | "minimal" |
"live"`, `OverlayPosition = "top" | "bottom"`, `LLMPrompt = { id: string;
name: string; prompt: string }`); `commands.getAvailableTypingTools()`;
  top-level shared settings `overlay_position` and `post_process_prompts`
  (unchanged, read via ordinary `getSetting`).
- Produces: six new row components (`DictationPasteMethod`,
  `DictationClipboardHandling`, `DictationAutoSubmit`, `DictationTypingTool`,
  `DictationShowOverlay`, `DictationPostProcessPrompt`), each taking
  `descriptionMode?`, `grouped?`, `disabled?`; extended `DictationSettings`
  section component; `Sidebar.tsx`'s `postprocessing` section now also shows
  when dictation's post-processing is on, consumed by Task 11's manual
  verification.

**On the missing `external_script_path` field (see Task 8):**
`DictationPasteMethod` below omits the `external_script` option from its
dropdown entirely. `PasteMethod.tsx`'s original list includes it only on
Linux and needs a companion `Input` bound to `external_script_path`, which
`DictationSettings` (the locked struct) does not have. This is a real gap
between the spec's per-mode-settings table (which never mentions
`external_script_path` as shared or per-mode — it simply isn't discussed) and
the struct Tasks 1–7 produce. Leaving the option out is safer than silently
routing it to the shared top-level `external_script_path`, which would let a
Linux user's dictation paste method secretly depend on a path they set for
meeting mode. If dictation should support external-script paste, that needs
its own field in `DictationSettings` — a Task 1–7-scope change, out of reach
here.

- [ ] **Step 1: Create the paste-method row**

```tsx
// src/shorthand/dictation/DictationPasteMethod.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown, type DropdownOption } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import type { DictationSettings, PasteMethod } from "@/bindings";

interface DictationPasteMethodProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling of PasteMethod.tsx bound to
 * settings.dictation.paste_method. "External Script" is intentionally not
 * offered: DictationSettings has no external_script_path field of its own,
 * only the shared top-level one — see the gap noted in this task's header.
 */
export const DictationPasteMethod: React.FC<DictationPasteMethodProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const selectedMethod = (dictation?.paste_method || "ctrl_v") as PasteMethod;

  const mod = osType === "macos" ? "Cmd" : "Ctrl";
  const options: DropdownOption[] = [
    {
      value: "ctrl_v",
      label: t("settings.advanced.pasteMethod.options.clipboard", {
        modifier: mod,
      }),
    },
  ];

  if (osType !== "macos" || selectedMethod === "direct") {
    options.push({
      value: "direct",
      label: t("settings.advanced.pasteMethod.options.direct"),
      disabled: osType === "macos",
    });
  }

  options.push({
    value: "none",
    label: t("settings.advanced.pasteMethod.options.none"),
  });

  if (osType === "windows" || osType === "linux") {
    options.push(
      {
        value: "ctrl_shift_v",
        label: t("settings.advanced.pasteMethod.options.clipboardCtrlShiftV"),
      },
      {
        value: "shift_insert",
        label: t("settings.advanced.pasteMethod.options.clipboardShiftInsert"),
      },
    );
  }

  return (
    <SettingContainer
      title={t("settings.advanced.pasteMethod.title")}
      description={t("settings.advanced.pasteMethod.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition="bottom"
    >
      <Dropdown
        options={options}
        selectedValue={selectedMethod}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            paste_method: value as PasteMethod,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
```

- [ ] **Step 2: Create the clipboard-handling row**

```tsx
// src/shorthand/dictation/DictationClipboardHandling.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { ClipboardHandling, DictationSettings } from "@/bindings";

interface DictationClipboardHandlingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

export const DictationClipboardHandling: React.FC<
  DictationClipboardHandlingProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const selectedHandling = (dictation?.clipboard_handling ||
    "dont_modify") as ClipboardHandling;

  const options = [
    {
      value: "dont_modify",
      label: t("settings.advanced.clipboardHandling.options.dontModify"),
    },
    {
      value: "copy_to_clipboard",
      label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
    },
  ];

  return (
    <SettingContainer
      title={t("settings.advanced.clipboardHandling.title")}
      description={t("settings.advanced.clipboardHandling.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        selectedValue={selectedHandling}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            clipboard_handling: value as ClipboardHandling,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
```

- [ ] **Step 3: Create the auto-submit row**

```tsx
// src/shorthand/dictation/DictationAutoSubmit.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import type { AutoSubmitKey, DictationSettings } from "@/bindings";

interface DictationAutoSubmitProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

type AutoSubmitOptionValue = AutoSubmitKey | "off";

export const DictationAutoSubmit: React.FC<DictationAutoSubmitProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const osType = useOsType();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  const enabled = dictation?.auto_submit ?? false;
  const selectedKey = (dictation?.auto_submit_key || "enter") as AutoSubmitKey;
  const selectedValue: AutoSubmitOptionValue = enabled ? selectedKey : "off";
  const submitWithMetaLabel =
    osType === "macos"
      ? t("settings.advanced.autoSubmit.options.cmdEnter")
      : t("settings.advanced.autoSubmit.options.superEnter");

  const options = [
    { value: "off", label: t("settings.advanced.autoSubmit.options.off") },
    {
      value: "enter",
      label: t("settings.advanced.autoSubmit.options.enter"),
    },
    {
      value: "ctrl_enter",
      label: t("settings.advanced.autoSubmit.options.ctrlEnter"),
    },
    { value: "cmd_enter", label: submitWithMetaLabel },
  ];

  const handleSelect = (value: string) => {
    const selected = value as AutoSubmitOptionValue;
    if (selected === "off") {
      updateSetting("dictation", {
        ...dictation,
        auto_submit: false,
      } as DictationSettings);
      return;
    }
    updateSetting("dictation", {
      ...dictation,
      auto_submit: true,
      auto_submit_key: selected as AutoSubmitKey,
    } as DictationSettings);
  };

  return (
    <SettingContainer
      title={t("settings.advanced.autoSubmit.title")}
      description={t("settings.advanced.autoSubmit.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
    >
      <Dropdown
        options={options}
        selectedValue={selectedValue}
        onSelect={handleSelect}
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
```

- [ ] **Step 4: Create the Linux typing-tool row**

```tsx
// src/shorthand/dictation/DictationTypingTool.tsx
import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { useOsType } from "@/hooks/useOsType";
import { commands } from "@/bindings";
import type { DictationSettings, TypingTool } from "@/bindings";

interface DictationTypingToolProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

const allToolLabels: Record<string, string> = {
  wtype: "wtype",
  kwtype: "kwtype",
  dotool: "dotool",
  ydotool: "ydotool",
  xdotool: "xdotool",
};

export const DictationTypingTool: React.FC<DictationTypingToolProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const [availableTools, setAvailableTools] = useState<string[] | null>(null);
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  useEffect(() => {
    if (osType !== "linux") return;
    commands
      .getAvailableTypingTools()
      .then(setAvailableTools)
      .catch(() => {
        setAvailableTools(["auto"]);
      });
  }, [osType]);

  // Only relevant on Linux, and only when paste_method is "direct" — same
  // gating as upstream's TypingTool.tsx.
  if (osType !== "linux") {
    return null;
  }
  if (dictation?.paste_method !== "direct") {
    return null;
  }

  const tools = availableTools ?? ["auto"];
  const options = tools.map((tool) =>
    tool === "auto"
      ? { value: "auto", label: t("settings.advanced.typingTool.options.auto") }
      : { value: tool, label: allToolLabels[tool] ?? tool },
  );

  const selectedTool = (dictation?.typing_tool || "auto") as TypingTool;

  return (
    <SettingContainer
      title={t("settings.advanced.typingTool.title")}
      description={t("settings.advanced.typingTool.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      tooltipPosition="bottom"
    >
      <Dropdown
        options={options}
        selectedValue={selectedTool}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            typing_tool: value as TypingTool,
          } as DictationSettings)
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
```

- [ ] **Step 5: Create the overlay row (style per-mode, position shared)**

```tsx
// src/shorthand/dictation/DictationShowOverlay.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type {
  DictationSettings,
  OverlayPosition,
  OverlayStyle,
} from "@/bindings";

interface DictationShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only sibling of ShowOverlay.tsx. The overlay *style* dropdown is
 * per-mode (settings.dictation.overlay_style, default "minimal" per the
 * spec). The overlay *position* dropdown stays bound to the shared
 * top-level overlay_position via the ordinary getSetting/updateSetting path
 * — the spec is explicit that "top-versus-bottom is a screen-layout
 * preference, not a mode one", so it is not read from or written to the
 * dictation struct at all.
 */
export const DictationShowOverlay: React.FC<DictationShowOverlayProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
  disabled = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;

  const styleOptions = [
    { value: "none", label: t("settings.advanced.overlay.style.options.none") },
    {
      value: "minimal",
      label: t("settings.advanced.overlay.style.options.minimal"),
    },
    { value: "live", label: t("settings.advanced.overlay.style.options.live") },
  ];

  const positionOptions = [
    {
      value: "bottom",
      label: t("settings.advanced.overlay.position.options.bottom"),
    },
    {
      value: "top",
      label: t("settings.advanced.overlay.position.options.top"),
    },
  ];

  const selectedStyle = (dictation?.overlay_style || "minimal") as OverlayStyle;
  const selectedPosition: OverlayPosition =
    getSetting("overlay_position") === "top" ? "top" : "bottom";

  return (
    <>
      <SettingContainer
        title={t("settings.advanced.overlay.style.title")}
        description={t("settings.advanced.overlay.style.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        disabled={disabled}
      >
        <Dropdown
          options={styleOptions}
          selectedValue={selectedStyle}
          onSelect={(value) =>
            updateSetting("dictation", {
              ...dictation,
              overlay_style: value as OverlayStyle,
            } as DictationSettings)
          }
          disabled={disabled || isUpdating("dictation")}
        />
      </SettingContainer>

      {selectedStyle !== "none" && (
        <SettingContainer
          title={t("settings.advanced.overlay.position.title")}
          description={t(
            "settings.dictation.overlayPosition.sharedDescription",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
          disabled={disabled}
        >
          <Dropdown
            options={positionOptions}
            selectedValue={selectedPosition}
            onSelect={(value) =>
              updateSetting("overlay_position", value as OverlayPosition)
            }
            disabled={disabled || isUpdating("overlay_position")}
          />
        </SettingContainer>
      )}
    </>
  );
};
```

- [ ] **Step 6: Create the post-process prompt picker**

```tsx
// src/shorthand/dictation/DictationPostProcessPrompt.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "@/components/ui/Dropdown";
import { SettingContainer } from "@/components/ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { DictationSettings } from "@/bindings";

interface DictationPostProcessPromptProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
  disabled?: boolean;
}

/**
 * Fork-only prompt picker for dictation's AI cleanup. Reads the shared
 * top-level post_process_prompts list (prompt authoring stays in upstream's
 * Post-processing section, per the spec — this section only picks) but
 * writes the selection to settings.dictation.post_process_selected_prompt_id
 * so dictation and meeting mode can select different prompts from the same
 * shared list.
 */
export const DictationPostProcessPrompt: React.FC<
  DictationPostProcessPromptProps
> = ({ descriptionMode = "tooltip", grouped = false, disabled = false }) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const dictation = getSetting("dictation") as DictationSettings | undefined;
  const prompts = getSetting("post_process_prompts") || [];
  const selectedPromptId = dictation?.post_process_selected_prompt_id || "";

  return (
    <SettingContainer
      title={t("settings.postProcessing.prompts.selectedPrompt.title")}
      description={t(
        "settings.postProcessing.prompts.selectedPrompt.description",
      )}
      descriptionMode={descriptionMode}
      grouped={grouped}
      disabled={disabled}
      layout="stacked"
    >
      <Dropdown
        options={prompts.map((p) => ({ value: p.id, label: p.name }))}
        selectedValue={selectedPromptId || null}
        onSelect={(value) =>
          updateSetting("dictation", {
            ...dictation,
            post_process_selected_prompt_id: value,
          } as DictationSettings)
        }
        placeholder={
          prompts.length === 0
            ? t("settings.postProcessing.prompts.noPrompts")
            : t("settings.postProcessing.prompts.selectPrompt")
        }
        disabled={disabled || isUpdating("dictation")}
      />
    </SettingContainer>
  );
};
```

- [ ] **Step 7: Extend `DictationSettings.tsx` with Output, AI Cleanup, Privacy, footer**

Replace the full file contents (this supersedes Task 8's version):

```tsx
// src/shorthand/DictationSettings.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ShortcutInput } from "@/components/settings/ShortcutInput";
import AccessibilityPermissions from "@/components/AccessibilityPermissions";
import { SettingsGroup } from "@/components/ui/SettingsGroup";
import { useSettings } from "@/hooks/useSettings";
import { DictationToggleField } from "./dictation/DictationToggleField";
import { DictationEnableToggle } from "./dictation/DictationEnableToggle";
import { DictationPasteMethod } from "./dictation/DictationPasteMethod";
import { DictationClipboardHandling } from "./dictation/DictationClipboardHandling";
import { DictationAutoSubmit } from "./dictation/DictationAutoSubmit";
import { DictationTypingTool } from "./dictation/DictationTypingTool";
import { DictationShowOverlay } from "./dictation/DictationShowOverlay";
import { DictationPostProcessPrompt } from "./dictation/DictationPostProcessPrompt";
import type { DictationSettings as DictationSettingsType } from "@/bindings";

/**
 * Fork-only "Dictation" section. See
 * docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md for
 * the full row inventory and rationale. Rows below the enable toggle stay
 * mounted and are individually disabled rather than hidden while dictation
 * is off; the Accessibility row and the AI-cleanup prompt picker are the two
 * exceptions (see inline comments).
 */
export const DictationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting } = useSettings();
  const dictation = getSetting("dictation") as
    | DictationSettingsType
    | undefined;
  const dictationEnabled = dictation?.enabled ?? false;
  const postProcessEnabled =
    dictationEnabled && (dictation?.post_process_enabled ?? false);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup>
        <DictationEnableToggle />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.shortcut")}>
        <ShortcutInput
          shortcutId="dictate"
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationToggleField
          field="push_to_talk"
          label={t("settings.general.pushToTalk.label")}
          description={t("settings.general.pushToTalk.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <ShortcutInput
          shortcutId="dictate_with_post_process"
          grouped={true}
          disabled={!dictationEnabled}
        />
      </SettingsGroup>

      {/* Not disabled-when-off like the rows above: AccessibilityPermissions
          has no disabled prop, only self-hide/show-a-Grant-button states, so
          gating on dictationEnabled means not rendering it at all rather
          than rendering it inert. */}
      {dictationEnabled && <AccessibilityPermissions />}

      <SettingsGroup title={t("settings.advanced.groups.output")}>
        <DictationPasteMethod grouped={true} disabled={!dictationEnabled} />
        <DictationTypingTool grouped={true} disabled={!dictationEnabled} />
        <DictationClipboardHandling
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationAutoSubmit grouped={true} disabled={!dictationEnabled} />
        <DictationToggleField
          field="append_trailing_space"
          label={t("settings.debug.appendTrailingSpace.label")}
          description={t("settings.debug.appendTrailingSpace.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationShowOverlay grouped={true} disabled={!dictationEnabled} />
      </SettingsGroup>

      <SettingsGroup title={t("settings.dictation.groups.aiCleanup")}>
        <DictationToggleField
          field="post_process_enabled"
          label={t("settings.debug.postProcessingToggle.label")}
          description={t("settings.debug.postProcessingToggle.description")}
          grouped={true}
          disabled={!dictationEnabled}
        />
        {/* Disabled whenever post-processing itself is off, not just when
            dictation is off — picking a prompt for a toggle that won't run
            is a dead control. */}
        <DictationPostProcessPrompt
          grouped={true}
          disabled={!postProcessEnabled}
        />
      </SettingsGroup>
      <p className="px-4 text-xs text-mid-gray">
        {t("settings.dictation.postProcessing.hint")}
      </p>

      <SettingsGroup title={t("settings.dictation.groups.privacy")}>
        <DictationToggleField
          field="save_recordings"
          label={t("settings.dictation.privacy.saveRecordings.label")}
          description={t(
            "settings.dictation.privacy.saveRecordings.description",
          )}
          grouped={true}
          disabled={!dictationEnabled}
        />
        <DictationToggleField
          field="save_transcripts"
          label={t("settings.dictation.privacy.saveTranscripts.label")}
          description={t(
            "settings.dictation.privacy.saveTranscripts.description",
          )}
          grouped={true}
          disabled={!dictationEnabled}
        />
      </SettingsGroup>

      <p className="px-4 text-xs text-mid-gray">
        {t("settings.dictation.footer")}
      </p>
    </div>
  );
};
```

- [ ] **Step 8: Make the Post-processing section visible for dictation's post-processing too**

`src/components/Sidebar.tsx:66` currently reads:

```ts
    enabled: (settings) => settings?.post_process_enabled ?? false,
```

Change to:

```ts
    enabled: (settings) =>
      (settings?.post_process_enabled ?? false) ||
      (settings?.dictation?.post_process_enabled ?? false),
```

This is the one-line upstream edit the spec calls out: "[the postprocessing
section's] `enabled` predicate reads `post_process_enabled` alone. It must
also become visible when _dictation's_ post-processing is on."

- [ ] **Step 9: Verify**

Run: `bun run lint`
Expected: no errors — all new user-facing text goes through `t()`.

Run: `bun run build`
Expected: succeeds.

Manual (debug build): with dictation disabled, open Dictation — every Output,
AI Cleanup and Privacy row is visible but dimmed/non-interactive-looking.
Enable dictation: rows become interactive. Set Paste Method to something
other than the default and reload the app (restart `tauri dev`) — the value
persisted (this is the check that would catch a missing `settingUpdaters`
entry, which is Task 7's responsibility, not this task's, but it's the
cheapest way to confirm the wiring this task depends on actually landed).
Turn dictation's AI Cleanup toggle on with `post_process_enabled` off
meeting-side: the "Post-Processing" sidebar entry appears. Turn both off:
it disappears. Set the paste method to "Direct" on Linux: the typing-tool row
appears; switch away from Direct and it disappears, matching upstream's
`TypingTool.tsx` behavior. Set overlay style to something other than "None":
the position row appears and its value matches whatever meeting mode's
overlay position is currently set to (shared field) — changing it here also
changes meeting mode's overlay position.

- [ ] **Step 10: Commit**

```bash
git add src/shorthand/dictation/DictationPasteMethod.tsx \
        src/shorthand/dictation/DictationClipboardHandling.tsx \
        src/shorthand/dictation/DictationAutoSubmit.tsx \
        src/shorthand/dictation/DictationTypingTool.tsx \
        src/shorthand/dictation/DictationShowOverlay.tsx \
        src/shorthand/dictation/DictationPostProcessPrompt.tsx \
        src/shorthand/DictationSettings.tsx \
        src/components/Sidebar.tsx
git commit -m "$(cat <<'EOF'
feat(shorthand): add Dictation output, AI cleanup and privacy rows

Output/overlay/save rows mirror upstream's row components but target
settings.dictation.* via the fork's own siblings, since useSettings has no
nested-path updater. Post-processing's sidebar visibility now also honours
dictation's own post_process_enabled flag, so enabling AI cleanup from
either mode surfaces the shared provider/prompt configuration screen.
EOF
)"
```

---

### Task 10: History `source` column and per-row tag (severable)

This task is independent of Tasks 8–9's UI: cutting it leaves the per-mode
save toggles (`DictationToggleField field="save_recordings"` /
`"save_transcripts"`, already wired in Task 9) fully functional — recordings
and transcripts from both modes still save correctly. Only the History list
becomes unable to tell them apart, which is a display nicety, not a
correctness bug. If this task is dropped, no other task needs to change.

This is the one task in this plan's scope that touches Rust, so — unlike
Tasks 8, 9 and 11 — it is verified with `cargo test`, not manual clicking.

**Files:**

- Modify: `src-tauri/src/managers/history.rs:1-11` (imports)
- Modify: `src-tauri/src/managers/history.rs:20-34` (migrations)
- Modify: `src-tauri/src/managers/history.rs:55-67` (`HistoryEntry` struct +
  new `source_for_mode` helper)
- Modify: `src-tauri/src/managers/history.rs:215-227` (`map_history_entry`)
- Modify: `src-tauri/src/managers/history.rs:235-296` (`save_entry`)
- Modify: `src-tauri/src/managers/history.rs:307-331` (`update_transcription`
  SELECT)
- Modify: `src-tauri/src/managers/history.rs:475-514` (`get_history_entries`,
  three SELECTs)
- Modify: `src-tauri/src/managers/history.rs:524-544`
  (`get_latest_entry_with_conn`, test-only)
- Modify: `src-tauri/src/managers/history.rs:552-572`
  (`get_latest_completed_entry_with_conn`)
- Modify: `src-tauri/src/managers/history.rs:605-625` (`get_entry_by_id`)
- Modify: `src-tauri/src/managers/history.rs:672-775` (tests module)
- Modify: `src/components/settings/history/HistorySettings.tsx:369-370`
- Regenerate: `src/bindings.ts` (adds `source: string` to `HistoryEntry`)

**Interfaces:**

- Consumes: `crate::shorthand::mode::{active, Mode}` — the locked mode-cell
  contract from Task 1 (`pub enum Mode { Meeting, Dictation }`, `pub fn
active(app: &AppHandle) -> Mode`). `HistoryManager::save_entry`'s existing
  signature is unchanged; it already holds `self.app_handle: AppHandle`
  (confirmed in the struct definition), so no caller in `actions.rs` needs to
  change — that file is Task 5's territory, not this one's, and per the spec
  it stays untouched by this addition.
- Produces: `HistoryEntry.source: String` (Rust) / `source: string` (TS,
  after regeneration) with values `"meeting"` or `"dictation"`; a private
  `source_for_mode(mode: Mode) -> &'static str` helper, deliberately pure and
  independent of `AppHandle` so it's unit-testable without a Tauri app
  instance; a source tag rendered next to each History row's date.

**Design note — why `source` is a plain `String`, not a Rust enum:** the
column is `TEXT NOT NULL DEFAULT 'meeting'`. Storing it as a Rust enum would
need `FromSql`/`ToSql` impls just for two literal values used nowhere else in
this file. A `String` field matches the column type directly and keeps the
diff to the migration's own literal strings.

**Design note — why `save_entry` isn't directly tested:** exercising
`save_entry` itself would need a real `AppHandle` (it calls
`crate::shorthand::mode::active(&self.app_handle)`), and this file has no
existing pattern for constructing one in a unit test — every existing test
here uses `Connection::open_in_memory()` directly, bypassing `HistoryManager`
entirely. Splitting the mode→string mapping into the pure
`source_for_mode` function makes that half unit-testable without a mock
`AppHandle`; a second test exercises the actual `MIGRATIONS` array and a raw
SQL round-trip through the `source` column. Together they cover "the
migration and the source round-trip" from the verification requirements
without inventing Tauri test infrastructure this file doesn't already have.

- [ ] **Step 1: Import the mode cell**

`src-tauri/src/managers/history.rs:1-11` currently ends:

```rust
use tauri::AppHandle;
use tauri_specta::Event;
```

Add one line after it:

```rust
use tauri::AppHandle;
use tauri_specta::Event;
use crate::shorthand::mode::{active, Mode};
```

- [ ] **Step 2: Add the migration**

`src-tauri/src/managers/history.rs:20-34` currently ends:

```rust
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
];
```

Change to:

```rust
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN source TEXT NOT NULL DEFAULT 'meeting';"),
];
```

- [ ] **Step 3: Add the `source` field and the mode→string helper**

`src-tauri/src/managers/history.rs:55-66` currently:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
}
```

Change to:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
    pub source: String,
}

/// Maps the fork's mode cell to the value stored in
/// `transcription_history.source`. Pure and independent of `AppHandle` so
/// it's testable without a Tauri app instance — see this task's design note.
fn source_for_mode(mode: Mode) -> &'static str {
    match mode {
        Mode::Meeting => "meeting",
        Mode::Dictation => "dictation",
    }
}
```

- [ ] **Step 4: Map the new column in `map_history_entry`**

`src-tauri/src/managers/history.rs:215-227` currently:

```rust
    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
        })
    }
```

Change to:

```rust
    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
            source: row.get("source")?,
        })
    }
```

- [ ] **Step 5: `save_entry` writes the resolved mode**

`src-tauri/src/managers/history.rs:235-296` currently:

```rust
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp();
        let title = self.format_timestamp_title(timestamp);

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &file_name,
                timestamp,
                false,
                &title,
                &transcription_text,
                &post_processed_text,
                &post_process_prompt,
                post_process_requested,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid(),
            file_name,
            timestamp,
            saved: false,
            title,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
        };
```

Change to:

```rust
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
    ) -> Result<HistoryEntry> {
        let timestamp = Utc::now().timestamp();
        let title = self.format_timestamp_title(timestamp);
        // Reads the mode of the capture currently in flight rather than
        // taking a parameter, per the fork's mode-cell design — see
        // src-tauri/src/shorthand/mode.rs.
        let source = source_for_mode(active(&self.app_handle));

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &file_name,
                timestamp,
                false,
                &title,
                &transcription_text,
                &post_processed_text,
                &post_process_prompt,
                post_process_requested,
                source,
            ],
        )?;

        let entry = HistoryEntry {
            id: conn.last_insert_rowid(),
            file_name,
            timestamp,
            saved: false,
            title,
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
            source: source.to_string(),
        };
```

(The rest of `save_entry` — the debug log, `cleanup_old_entries()` call, event
emission, and `Ok(entry)` — is unchanged.)

- [ ] **Step 6: Add `source` to the five remaining `SELECT` statements**

`src-tauri/src/managers/history.rs:327-328`, inside `update_transcription`,
currently:

```rust
                "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested
                 FROM transcription_history WHERE id = ?1",
```

Change to:

```rust
                "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, source
                 FROM transcription_history WHERE id = ?1",
```

`src-tauri/src/managers/history.rs:479-480`, `493-494`, and `505-506`, inside
`get_history_entries` (three near-identical `SELECT`s — cursor+limit,
limit-only, unbounded), each currently starts:

```rust
                    "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested
                     FROM transcription_history
```

Change each occurrence to:

```rust
                    "SELECT id, file_name, timestamp, saved, title, transcription_text, post_processed_text, post_process_prompt, post_process_requested, source
                     FROM transcription_history
```

`src-tauri/src/managers/history.rs:527-537`, the `#[cfg(test)]`
`get_latest_entry_with_conn` helper, currently:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;
```

Change to:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;
```

`src-tauri/src/managers/history.rs:554-564`,
`get_latest_completed_entry_with_conn`, same shape:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
             FROM transcription_history
             WHERE transcription_text != ''
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;
```

Change to:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source
             FROM transcription_history
             WHERE transcription_text != ''
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;
```

`src-tauri/src/managers/history.rs:608-618`, `get_entry_by_id`, same shape:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
             FROM transcription_history
             WHERE id = ?1",
        )?;
```

Change to:

```rust
        let mut stmt = conn.prepare(
            "SELECT
                id,
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source
             FROM transcription_history
             WHERE id = ?1",
        )?;
```

- [ ] **Step 7: Update the tests module — schema, helpers, and the migration/round-trip tests**

`src-tauri/src/managers/history.rs:677-720` currently:

```rust
    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("handy-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
            ],
        )
        .expect("insert history entry");
    }
```

Change to:

```rust
    fn setup_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'meeting'
            );",
        )
        .expect("create transcription_history table");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        // Relies on the column's own DEFAULT 'meeting' — exercising that
        // default is the point of source_round_trips_through_the_source_column
        // below, not something to duplicate here.
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("handy-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
            ],
        )
        .expect("insert history entry");
    }

    fn insert_entry_with_source(conn: &Connection, timestamp: i64, text: &str, source: &str) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                format!("handy-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                Option::<String>::None,
                Option::<String>::None,
                false,
                source,
            ],
        )
        .expect("insert history entry with source");
    }
```

Then, inside the same `mod tests` block, add three new tests (placed after
the existing `recording_file_to_delete_joins_non_empty_file_name` test, at
the end of the file, before the closing `}` at line 775):

```rust
    #[test]
    fn source_for_mode_maps_meeting_and_dictation() {
        assert_eq!(source_for_mode(Mode::Meeting), "meeting");
        assert_eq!(source_for_mode(Mode::Dictation), "dictation");
    }

    #[test]
    fn source_round_trips_through_the_source_column() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "meeting text", None);
        insert_entry_with_source(&conn, 200, "dictated text", "dictation");

        let latest = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");
        assert_eq!(latest.timestamp, 200);
        assert_eq!(latest.source, "dictation");

        let meeting_source: String = conn
            .query_row(
                "SELECT source FROM transcription_history WHERE timestamp = 100",
                [],
                |row| row.get(0),
            )
            .expect("read source column");
        assert_eq!(meeting_source, "meeting");
    }

    #[test]
    fn migrations_add_source_column_with_meeting_default() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        let migrations = Migrations::new(MIGRATIONS.to_vec());
        migrations.to_latest(&mut conn).expect("run migrations");

        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text)
             VALUES ('', 1, 'title', 'text')",
            [],
        )
        .expect("insert without specifying source");

        let source: String = conn
            .query_row(
                "SELECT source FROM transcription_history WHERE timestamp = 1",
                [],
                |row| row.get(0),
            )
            .expect("read source column");
        assert_eq!(source, "meeting");
    }
```

- [ ] **Step 8: Run the Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml managers::history::tests`
Expected: all tests pass, including the three new ones and the four
pre-existing ones (`get_latest_entry_returns_none_when_empty`,
`get_latest_entry_returns_newest_entry`,
`get_latest_completed_entry_skips_empty_entries`,
`recording_file_to_delete_skips_empty_file_name`,
`recording_file_to_delete_joins_non_empty_file_name`) — none of those needed
edits since `insert_entry` relies on the column default and none of them
construct a `HistoryEntry` by hand.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: no new warnings in `history.rs`.

- [ ] **Step 9: Regenerate `bindings.ts` and commit it**

Run a debug build (`bun run tauri dev`, then stop it once the window opens —
the `#[cfg(debug_assertions)]` export runs on startup) so `src/bindings.ts`
picks up `HistoryEntry.source: string`.

Verification: `grep "source: string" src/bindings.ts` inside the
`HistoryEntry` type definition succeeds.

- [ ] **Step 10: Render the source tag in History**

`src/components/settings/history/HistorySettings.tsx:369-370` currently:

```tsx
      <div className="flex justify-between items-center">
        <p className="text-sm font-medium">{formattedDate}</p>
```

Change to:

```tsx
      <div className="flex justify-between items-center">
        <div className="flex items-center gap-2">
          <p className="text-sm font-medium">{formattedDate}</p>
          <span className="text-xs px-1.5 py-0.5 rounded-full bg-mid-gray/10 text-mid-gray uppercase tracking-wide">
            {entry.source === "dictation"
              ? t("settings.history.source.dictation")
              : t("settings.history.source.meeting")}
          </span>
        </div>
```

This closes one extra `<div>` that must be balanced by the existing closing
tag before the icon-button row — the icon-button `<div className="flex
items-center">` two lines below already opens its own sibling `<div>`, so no
other line in the surrounding JSX needs to change; only this opening pair
does.

- [ ] **Step 11: Verify**

Run: `bun run lint`
Expected: no errors.

Manual (debug build): record once with meeting mode's shortcut and once with
dictation's shortcut (both with save-transcripts on). Open History: both
entries show a tag ("Meeting" / "Dictation" — English text until Task 11
adds the keys, meanwhile shown as raw key paths, same caveat as Task 8/9).
Existing entries recorded before this migration ran show "Meeting" (the
column default), which is correct — every entry before this feature existed
was a meeting capture.

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/managers/history.rs \
        src/bindings.ts \
        src/components/settings/history/HistorySettings.tsx
git commit -m "$(cat <<'EOF'
feat(shorthand): tag history entries with their capture mode

History existed to let a user review and purge meeting captures containing
other people's voices; letting dictation save into the same undifferentiated
list blunts that. source resolves from the fork's mode cell in save_entry
rather than growing a new parameter, matching how the cell already resolves
paste/overlay/save behaviour elsewhere.
EOF
)"
```

---

### Task 11: i18n and lint pass

Adds every key referenced by Tasks 8–10 to **all 24** locale files
(`ar bg cs da de en es fr he hi it ja ko ne nl pl pt ru sv tr uk vi zh
zh-TW`), English text as the value in every locale, and does a final
`bun run check:translations` / `bun run lint` pass across the whole feature.

**On reuse:** the following keys already exist and are reused as-is (no new
key, confirmed present in `src/i18n/locales/en/translation.json`):
`settings.general.pushToTalk.label` / `.description`;
`settings.advanced.pasteMethod.title` / `.description` /
`.options.{clipboard,direct,none,clipboardCtrlShiftV,clipboardShiftInsert}`;
`settings.advanced.clipboardHandling.title` / `.description` /
`.options.{dontModify,copyToClipboard}`;
`settings.advanced.autoSubmit.title` / `.description` /
`.options.{off,enter,cmdEnter,superEnter,ctrlEnter}`;
`settings.advanced.typingTool.title` / `.description` / `.options.auto`;
`settings.advanced.overlay.style.title` / `.description` /
`.options.{none,minimal,live}`; `settings.advanced.overlay.position.title` /
`.options.{bottom,top}`; `settings.advanced.groups.output`;
`settings.debug.appendTrailingSpace.label` / `.description`;
`settings.debug.postProcessingToggle.label` / `.description`;
`settings.postProcessing.prompts.selectedPrompt.title` / `.description`;
`settings.postProcessing.prompts.noPrompts` / `.selectPrompt`;
`accessibility.permissionsDescription` / `.openSettings` (used unmodified by
the reused `AccessibilityPermissions` component).

**Flagged, not reused:** `SaveRecordings.tsx` and `SaveTranscripts.tsx`
reference `settings.privacy.saveRecordings.label` /
`settings.privacy.saveTranscripts.label`, and `src/shorthand/CaptureSettings.tsx`
renders `t("settings.privacy.title")` as its Privacy group heading — none of
these three keys exist anywhere in `en/translation.json` (confirmed: the
top-level `settings.privacy` object is entirely absent). These are
pre-existing gaps, not something this plan's tasks introduced, and they are
out of scope here — fixing them would mean editing upstream's
`SaveRecordings.tsx`/`SaveTranscripts.tsx` display behavior, which is not
part of Tasks 8–11. Task 9's `DictationToggleField` calls for
`save_recordings`/`save_transcripts` pass **new**, dictation-scoped keys
(`settings.dictation.privacy.*`) rather than the broken shared path, so
dictation's own UI is unaffected by the gap. Worth a follow-up outside this
plan.

**Note on the `transcribe` relabel:** Step 1 changes the value of an
_existing_ key, `settings.general.shortcut.bindings.transcribe.name`, across
all 24 locales. `check:translations` only checks that every key is present in
every locale — it cannot tell you that one locale still says "Transcribe
Shortcut". Change all 24 by hand and check the diff shows 24 files.

**Files:**

- Modify: `src/i18n/locales/{ar,bg,cs,da,de,en,es,fr,he,hi,it,ja,ko,ne,nl,pl,pt,ru,sv,tr,uk,vi,zh,zh-TW}/translation.json`

**Interfaces:**

- Consumes: every `t("...")` call added in Tasks 8–10.
- Produces: a complete, `check:translations`-clean key set; nothing further
  consumes this task's output within this plan.

**New keys** (19 leaf keys; English value shown — this is also the value
written into all 24 locale files per the "English text as the value in every
locale" convention):

| Key                                                                        | Value                                                                                                                                     |
| -------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `sidebar.dictation`                                                        | `Dictation`                                                                                                                               |
| `settings.dictation.enable.label`                                          | `Enable Dictation`                                                                                                                        |
| `settings.dictation.enable.description`                                    | `Turn on a separate dictation mode, with its own shortcut, that pastes text into whatever window has focus.`                              |
| `settings.dictation.enable.shortcutConflict`                               | `Could not enable dictation: another application is already using one of its shortcuts. Choose a different shortcut below and try again.` |
| `settings.dictation.groups.shortcut`                                       | `Shortcut`                                                                                                                                |
| `settings.dictation.groups.aiCleanup`                                      | `AI Cleanup`                                                                                                                              |
| `settings.dictation.groups.privacy`                                        | `Privacy`                                                                                                                                 |
| `settings.dictation.postProcessing.hint`                                   | `Configure providers, API keys and models in the Post-Processing section.`                                                                |
| `settings.dictation.privacy.saveRecordings.label`                          | `Save Recordings`                                                                                                                         |
| `settings.dictation.privacy.saveRecordings.description`                    | `Keep the audio recording for each dictation.`                                                                                            |
| `settings.dictation.privacy.saveTranscripts.label`                         | `Save Transcripts`                                                                                                                        |
| `settings.dictation.privacy.saveTranscripts.description`                   | `Keep the transcript text for each dictation.`                                                                                            |
| `settings.dictation.overlayPosition.sharedDescription`                     | `Where the overlay appears on screen. Shared with meeting mode — this isn't a per-mode setting.`                                          |
| `settings.dictation.footer`                                                | `Microphone, model and language come from the Capture and Transcription sections.`                                                        |
| `settings.general.shortcut.bindings.dictate.name`                          | `Dictation Shortcut`                                                                                                                      |
| `settings.general.shortcut.bindings.dictate.description`                   | `The keyboard shortcut to start and stop dictation.`                                                                                      |
| `settings.general.shortcut.bindings.dictate_with_post_process.name`        | `Dictation AI Cleanup Hotkey`                                                                                                             |
| `settings.general.shortcut.bindings.dictate_with_post_process.description` | `Optional: a dedicated hotkey that always applies AI cleanup to your dictation.`                                                          |
| `settings.history.source.meeting`                                          | `Meeting`                                                                                                                                 |
| `settings.history.source.dictation`                                        | `Dictation`                                                                                                                               |

- [ ] **Step 1: Add the sidebar and shortcut-binding keys to `en/translation.json`**

In the `sidebar` object (alongside the existing `capture`/`transcription`/`app`
entries added by the prior settings-UI plan), add:

```json
    "dictation": "Dictation",
```

In `settings.general.shortcut.bindings` (alongside the existing `transcribe`,
`cancel`, `transcribe_with_post_process` entries), add:

```json
      "dictate": {
        "name": "Dictation Shortcut",
        "description": "The keyboard shortcut to start and stop dictation."
      },
      "dictate_with_post_process": {
        "name": "Dictation AI Cleanup Hotkey",
        "description": "Optional: a dedicated hotkey that always applies AI cleanup to your dictation."
      }
```

In the same `bindings` object, change the **existing** `transcribe` entry's
value. With two transcribe-style shortcuts on screen, "Transcribe Shortcut"
no longer distinguishes anything — the spec relabels it to match the Capture
section it lives in. This is a copy change to an existing key, not a new key,
so `check:translations` will not catch it if you miss a locale:

```json
      "transcribe": {
        "name": "Capture Shortcut",
        "description": "The keyboard shortcut to record and transcribe a meeting or note."
      },
```

- [ ] **Step 2: Add the new `settings.dictation` object to `en/translation.json`**

Alongside the other top-level entries under `settings` (e.g. next to
`postProcessing`), add:

```json
    "dictation": {
      "enable": {
        "label": "Enable Dictation",
        "description": "Turn on a separate dictation mode, with its own shortcut, that pastes text into whatever window has focus.",
        "shortcutConflict": "Could not enable dictation: another application is already using one of its shortcuts. Choose a different shortcut below and try again."
      },
      "groups": {
        "shortcut": "Shortcut",
        "aiCleanup": "AI Cleanup",
        "privacy": "Privacy"
      },
      "postProcessing": {
        "hint": "Configure providers, API keys and models in the Post-Processing section."
      },
      "privacy": {
        "saveRecordings": {
          "label": "Save Recordings",
          "description": "Keep the audio recording for each dictation."
        },
        "saveTranscripts": {
          "label": "Save Transcripts",
          "description": "Keep the transcript text for each dictation."
        }
      },
      "overlayPosition": {
        "sharedDescription": "Where the overlay appears on screen. Shared with meeting mode — this isn't a per-mode setting."
      },
      "footer": "Microphone, model and language come from the Capture and Transcription sections."
    },
```

- [ ] **Step 3: Add the history source-tag keys to `en/translation.json`**

In `settings.history` (alongside the existing `loading`, `empty`, `save`,
etc. entries), add:

```json
      "source": {
        "meeting": "Meeting",
        "dictation": "Dictation"
      },
```

- [ ] **Step 4: Repeat Steps 1–3 for the other 23 locale files**

For each of `ar bg cs da de es fr he hi it ja ko ne nl pl pt ru sv tr uk vi
zh zh-TW`, add the identical key paths from Steps 1–3 with the same English
text as the value (per the hard requirement: new keys carry the English
string as the value in every locale — this is not a translation pass).
Insert at the same structural location as `en` so the files stay easy to
diff against each other; do not reorder or reformat any existing key in any
of the 24 files.

- [ ] **Step 5: Verify translation completeness**

Run: `bun run check:translations`
Expected: `✓ All 24 languages have complete translations!` — every key added
in Steps 1–4 is present in all 24 files with no extras and no gaps.

- [ ] **Step 6: Full lint and build pass**

Run: `bun run lint`
Expected: no errors across all files touched in Tasks 8–11.

Run: `bun run build`
Expected: succeeds.

Run: `bun run format:check`
Expected: no diffs — all 24 JSON files and the new TSX files match Prettier's
formatting.

- [ ] **Step 7: Manual smoke test of the fully-translated section**

Debug build (`bun run tauri dev`): open Settings → Dictation. Every label,
description, group heading, and the footer line now render real text instead
of raw key paths (the visible regression from Tasks 8–10's manual-check notes
is gone). Open History and confirm the "Meeting"/"Dictation" tags read as
real words. Switch the app's display language (`app_language` setting) to
any non-English locale and confirm the Dictation section still renders (its
strings will be the English fallback text, per this task's key values, not a
missing-key error) — this is the check that would catch a locale file where
Step 4 was skipped or malformed.

- [ ] **Step 8: Commit**

```bash
git add src/i18n/locales/*/translation.json
git commit -m "$(cat <<'EOF'
feat(shorthand): add i18n keys for the Dictation settings section

Adds every key Tasks 8-10 reference across all 24 locale files, with the
English string as the value in every locale per the project's translation
convention. Closes out the Dictation UI work: bun run check:translations and
bun run lint both pass clean.
EOF
)"
```
