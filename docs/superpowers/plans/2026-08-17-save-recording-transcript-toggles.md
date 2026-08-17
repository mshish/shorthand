# Plan: privacy toggles for saving recordings and transcripts

## Context

Shorthand currently persists **both** artifacts of every transcription, unconditionally:

- the WAV, written to `<app data>/recordings/` (`src-tauri/src/actions.rs`, the
  `spawn_blocking(save_wav_file)` around line 769)
- a row in `history.db` holding the transcript text plus the WAV's file name
  (`hm.save_entry(...)`, around line 889, and again on the transcription-error
  path around line 967)

The only existing controls are *retention* — `history_limit` and
`recording_retention_period` — which prune after the fact. Note that
`recording_retention_period: "never"` means "never **delete**", not "never
save"; it is unrelated to this work, and the new labels must not read as
duplicates of it.

Neither "don't save the recording" nor "don't save the transcript" exists.
Both are added here, defaulting to **disabled**, as privacy controls.

## Spec

Two independent boolean settings, both defaulting to `false`:

| Setting            | `false` (default)                                | `true`                            |
| ------------------ | ------------------------------------------------ | --------------------------------- |
| `save_recordings`  | no WAV file is written to disk                   | WAV written as today              |
| `save_transcripts` | no transcript text is stored in `history.db`     | transcript stored as today        |

They are independent: any of the four combinations is legal. In all four, the
transcript is still delivered normally (paste / clipboard / follow-stream) —
these settings govern *persistence only*, never delivery.

### The history row is the index for both artifacts

A history row is written when **either** artifact is kept, and its two fields
carry which:

- `file_name` is the WAV's name when a WAV was saved, otherwise `""`
- `transcription_text` is the transcript when transcripts are on, otherwise `""`

So the row is written iff `wav_saved || save_transcripts`.

This matters for correctness, not tidiness: cleanup
(`HistoryManager::cleanup_*`) walks DB rows to find files to delete. A WAV with
no row would be invisible to the UI *and* to every retention policy — it would
accumulate forever, which is the exact failure a privacy control must not have.

## Global Constraints

- Field names are exactly `save_recordings` and `save_transcripts`.
- Both default to `false` everywhere: the serde default, `get_default_settings()`,
  and the TypeScript bindings.
- **No settings-schema migration and no `CURRENT_SETTINGS_SCHEMA_VERSION` bump.**
  An absent key already deserializes to `false`, which *is* the wanted default,
  for existing stores as well as fresh ones. Bumping the version would put an
  unnecessary rewrite through `apply_settings_migrations` and would break the
  blast-radius assertion in `frozen_v0_9_store_parses_strictly_and_migrates_only_paste_method`.
- Locale files under `src/i18n/locales/` stay **byte-identical to upstream**.
  Every new user-facing string goes in `FORK_ONLY_STRINGS` in
  `src/shorthand/branding.ts` — see that file's header for why. Adding keys to
  `en/translation.json` would break `bun run check:translations`.
- No hardcoded strings in JSX; ESLint enforces i18next usage.
- Follow the existing patterns exactly: boolean setting commands look like
  `change_mute_while_recording_setting` in `src-tauri/src/shortcut/mod.rs`;
  toggle components look like `src/components/settings/MuteWhileRecording.tsx`.
- Run `cargo fmt`, `cargo clippy`, and `bun run lint` before committing.

---

## Task 1 — Backend: settings fields and their commands

**Files:** `src-tauri/src/settings.rs`, `src-tauri/src/shortcut/mod.rs`, `src-tauri/src/lib.rs`

1. In `settings.rs`, add two fields to `AppSettings`, placed immediately after
   `recording_retention_period` so the persistence-related settings stay together:

   ```rust
   /// Whether the WAV of each transcription is written to the recordings
   /// directory. Off by default: Shorthand treats stored audio as opt-in.
   #[serde(default)]
   pub save_recordings: bool,
   /// Whether the transcript text of each transcription is stored in
   /// history.db. Off by default, for the same reason.
   #[serde(default)]
   pub save_transcripts: bool,
   ```

   Add `save_recordings: false,` and `save_transcripts: false,` to
   `get_default_settings()` in the matching position. Do **not** add
   `default_*()` helper fns — `#[serde(default)]` on a `bool` already yields
   `false`, and the codebase only writes helpers for non-`Default` values.

2. In `shortcut/mod.rs`, add two commands modelled exactly on
   `change_mute_while_recording_setting`:

   ```rust
   #[tauri::command]
   #[specta::specta]
   pub fn change_save_recordings_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
       let mut settings = settings::get_settings(&app);
       settings.save_recordings = enabled;
       settings::write_settings(&app, settings);
       Ok(())
   }
   ```

   and the same shape for `change_save_transcripts_setting`.

3. Register both in `collect_commands![...]` in `lib.rs`, next to
   `shortcut::change_show_all_settings_setting`.

**Tests** (in `settings.rs`'s existing `mod tests`):

- `default_settings_disable_saving_recordings_and_transcripts` — asserts
  `get_default_settings()` has both `false`.
- Extend `empty_store_parses_with_defaults` with assertions that both fields
  are `false` when parsed from `json!({})`.
- Add a test that a store which explicitly stores `true` for both keeps them
  `true` through `apply_settings_migrations` (they are opt-in, and nothing may
  silently turn them back off).

`frozen_v0_9_store_parses_strictly_and_migrates_only_paste_method` must keep
passing **unchanged** — do not edit that test or its fixture. If it fails, the
change violated the no-migration constraint above.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml settings`

---

## Task 2 — Backend: honour the toggles in the transcription pipeline

**Files:** `src-tauri/src/actions.rs`, `src-tauri/src/commands/history.rs`,
`src-tauri/src/managers/history.rs`

1. In `actions.rs`'s `TranscribeAction::stop` async task, read both flags once
   before the WAV work begins (there is already a `get_settings(&ah)` call
   further down inside a closure — read the flags separately near the top of
   the `else` branch that handles non-empty samples; do not move the existing
   call).

2. Gate the WAV write. Today the code unconditionally builds `file_name`,
   spawns `save_wav_file`, then awaits and verifies it into a `wav_saved: bool`.
   When `save_recordings` is `false`, skip all of that: no `spawn_blocking`, no
   verify, `wav_saved = false`, and the file name used for the history row is
   `String::new()`. Keep the existing success/verify/error handling exactly as
   it is for the `true` case, including the `error!` logs.

3. Gate the history row. Both `hm.save_entry(...)` call sites — the success
   path and the transcription-error path — currently run `if wav_saved`. They
   become `if wav_saved || save_transcripts`, passing:
   - the real `file_name` when `wav_saved`, else `String::new()`
   - the real transcription text when `save_transcripts`, else `String::new()`

   (On the error path the text argument is already `String::new()`; that call
   site only needs its guard widened and its file-name argument adjusted.)

4. In `commands/history.rs`, `retry_history_entry_transcription` currently
   joins `entry.file_name` onto the recordings dir and reads the WAV. Add an
   early guard, before the existing `merged_transcript_retry_error_for_app`
   check, returning `Err` when `entry.file_name.is_empty()` — there is no audio
   to re-transcribe. Use a plain English message consistent with the
   neighbouring errors, e.g. `"This entry has no saved recording to re-transcribe"`.

5. In `managers/history.rs`, both file-deleting paths must skip empty file
   names: `delete_entries_and_files` and `delete_entry` build
   `recordings_dir.join(&file_name)`, and joining `""` yields the recordings
   **directory** — which exists, so `fs::remove_file` is attempted on a
   directory and logs a spurious error on every cleanup. Guard both with
   `if !file_name.is_empty()`. The DB row must still be deleted either way.

**Tests:**

- `managers/history.rs` — a unit test that `recordings_dir.join("")` is not
  treated as a deletable file. Follow the existing in-memory-SQLite test style
  in that module's `mod tests`; if the guard cannot be reached without an
  `AppHandle`, extract the "should this file be deleted" decision into a small
  free function (e.g. `fn recording_file_to_delete(dir: &Path, file_name: &str) -> Option<PathBuf>`)
  and unit-test that instead. Do not add a test that asserts nothing.
- `commands/history.rs` — if the empty-`file_name` guard can be reached without
  an `AppHandle`, unit-test it; otherwise state in the report why not, and do
  not fabricate a test around a mock.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml`, then
`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` and
`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`.

---

## Task 3 — Frontend: toggles, wiring, and audio-less history entries

**Files:** `src/bindings.ts`, `src/stores/settingsStore.ts`,
`src/components/settings/SaveRecordings.tsx`,
`src/components/settings/SaveTranscripts.tsx`,
`src/shorthand/branding.ts`, `src/shorthand/CaptureSettings.tsx`,
`src/components/settings/advanced/AdvancedSettings.tsx`,
`src/components/settings/history/HistorySettings.tsx`

1. **`src/bindings.ts`** is generated by tauri-specta, but only during a debug
   `tauri dev` run (`lib.rs`, `#[cfg(debug_assertions)]`), which is not
   available here. Hand-edit it to match exactly what specta would emit:
   - two `commands` wrappers, `changeSaveRecordingsSetting` and
     `changeSaveTranscriptsSetting`, copied from the shape of
     `changeShowAllSettingsSetting` (same `TAURI_INVOKE` + error-shape handling)
   - `save_recordings?: boolean; save_transcripts?: boolean;` added to the
     `AppSettings` type in the same position as the Rust struct fields
     (immediately after `recording_retention_period`)

2. **`src/stores/settingsStore.ts`** — add two entries to the setting-updater
   map, matching the surrounding style:
   ```ts
   save_recordings: (value) => commands.changeSaveRecordingsSetting(value as boolean),
   save_transcripts: (value) => commands.changeSaveTranscriptsSetting(value as boolean),
   ```

3. **Two toggle components**, `SaveRecordings.tsx` and `SaveTranscripts.tsx`,
   copied structurally from `MuteWhileRecording.tsx`: `React.memo`, the
   `{ descriptionMode, grouped }` props with the same defaults, `useSettings`,
   and a `ToggleSwitch`. No cross-setting `disabled` logic — these two are
   independent.

4. **Strings** go in `FORK_ONLY_STRINGS` in `src/shorthand/branding.ts`, not in
   any locale file:
   ```ts
   "settings.privacy.title": "Privacy",
   "settings.privacy.saveRecordings.label": "Save recordings",
   "settings.privacy.saveRecordings.description":
     "Keep the audio of each transcription on disk so you can play it back or re-transcribe it. Off by default; nothing you say is written to disk.",
   "settings.privacy.saveTranscripts.label": "Save transcripts",
   "settings.privacy.saveTranscripts.description":
     "Keep the text of each transcription in your local history. Off by default; transcripts are delivered and then discarded.",
   "settings.history.transcriptNotSaved": "Transcript not saved.",
   ```
   Wording must not collide with Recording Retention, which controls how long
   saved items are *kept*.

5. **Placement.** Both toggles render in a `SettingsGroup` titled
   `t("settings.privacy.title")`:
   - in `src/shorthand/CaptureSettings.tsx` — the section Shorthand opens on by
     default, so the privacy controls are in the main settings pane
   - in `AdvancedSettings.tsx`'s existing history `SettingsGroup`, so they
     remain reachable when `show_all_settings` hides the fork-only sections
     (see `src/shorthand/visibility.ts`)

6. **`HistorySettings.tsx`** must handle rows where an artifact is absent:
   - `entry.file_name === ""` → do not render the `AudioPlayer`, and disable the
     re-transcribe button (there is no audio; the backend rejects it too)
   - `entry.transcription_text` empty → the existing copy claims transcription
     *failed*. When the current `save_transcripts` setting is `false`, show
     `t("settings.history.transcriptNotSaved")` instead. This reads the live
     setting rather than per-entry state, which is right for the common case
     and cannot be made exact without a new DB column — a deliberate trade.
   - the copy-to-clipboard button already gates on non-empty text; leave it.

**Verify:** `bun run lint`, `bun run build` (TypeScript must compile), and
`bun run check:translations` (must still pass — proof the locale files were not
touched).
