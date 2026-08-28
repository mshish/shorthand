# Assisted Notes Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third fixed capture mode, **Assisted Notes** — "Meeting, but solo". It streams to `--follow-stream` followers exactly as Meeting does (so the Obsidian plugin's enhancement pipeline fills the note live), captures **no** system audio, and never pastes into the focused window. The Obsidian plugin gets a command that starts it.

**Architecture:** A third `Mode` variant, a new per-mode settings struct, one new CLI flag, one widened `ControlSignal` member, and one new plugin command. The sink selects an _app-owned mode by name_; it never supplies settings values. System audio and paste delivery are mode invariants, not settings. Follow-stream publication remains per-mode, while one additive lifecycle helper keeps the shared listener running whenever any enabled mode needs it. The follower `hello` advertises the new control capability, and the plugin requires that capability plus a bounded `begin` acknowledgement before it calls the capture started. Three repos are touched in a fixed order after two app prerequisites.

**Tech Stack:** Rust/Tauri 2 + React/TypeScript (`shorthand-app`), TypeScript + Bun (`shorthand-core`), TypeScript + Bun + esbuild (`obsidian-shorthand`).

**Specs this builds on:**

- `docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md` — the per-mode settings mechanism (`apply_mode`, the mode cell, the five registration guards). Assisted Notes is a second instance of that mechanism; read it before writing any Rust.
- `docs/superpowers/specs/2026-08-23-shorthand-brand-ux-redesign.md` Part 2 — the Modes pane's membership rule.
- `shorthand-core/AGENTS.md` § "A change here is not done when it is tagged" — the authority on cross-repo ordering.

---

## Decisions already taken — do not relitigate

1. **A third fixed mode, not a generic profile system.** Generic user-defined profiles and sink-defined profiles with CLI setting-overrides were both considered and rejected. Three reasons, and no part of this plan may undermine them:
   - the app's own settings UI must always truthfully describe a running capture;
   - consent settings must not be settable from outside the app;
   - the CLI's small, stable flag surface must not become a settings-injection API.
2. **Information architecture:** the Modes pane gets a **Notetaking** group holding **Meetings** and **Assisted notes**, with **Dictation** as a separate peer. Meetings and Assisted notes are grouped because they share a _destination_ (follower processes); Dictation is separate because it delivers to the focused window.
3. **The sink selects an app-owned mode.** The plugin picks _which_ mode to start. It never supplies a settings value.
4. **`save_recordings` / `save_transcripts` default to `true`** for Assisted Notes, **and Dictation's existing `false` defaults flip to `true` in the same change.** Meeting mode's top-level `AppSettings` defaults are **not** touched. This overrides the "Consent, not preference — stays opt-in" comment currently in `dictation.rs`; that comment is now false and must be replaced (Task 3).
5. **No system audio means no system-audio setting.** Assisted Notes always resolves `system_audio_enabled` to `false`. A default-off toggle would contradict the mode's promise as soon as a user switched it on.
6. **A per-mode publication toggle must also participate in listener lifetime.** `follow_stream_enabled` decides whether a capture calls `hub.begin()`. A shared listener runs whenever Meeting, enabled Dictation, or enabled Assisted Notes wants publication. No mode's preference doubles as a global server switch after this change.
7. **AI cleanup is Advanced for both notetaking modes.** Meetings already puts its cleanup toggle and dependent binding behind `<AdvancedOnly>`. Assisted Notes does the same. Dictation alone keeps cleanup in the default view, matching the warning that enabling it for notetaking is advanced and not recommended.

---

## Ordering across the three repos

Two reviewed app plans land first:

1. `2026-08-26-fork-only-translation-catalogues.md`, so new fork strings have one canonical home and the new unit/parity gates exist.
2. `2026-08-26-meeting-minimal-overlay-diagnosis.md`, so the active capture owns waveform/readiness emission and Assisted Notes does not inherit Dictation's flat-waveform bug.

`shorthand-core/AGENTS.md` § "A change here is not done when it is tagged" governs the core→plugin half. The app feature is sequenced before core for a separate reason, given below.

| #   | Repo                 | What lands                                                                                                                                                                                                            | Gate that must be green before moving on                                                                                                                                                                                                       |
| --- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `shorthand-app`      | The mode itself: `Mode::AssistedNotes`, `AssistedNotesSettings`, listener reconciliation, two bindings, the `--toggle-assisted-notes` flag, the advertised follower capability, the Modes UI, docs. Tasks 1–11.       | `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`, `bun run test:unit`, `bun run lint`, `bun run build`, `bun run check:translations`, `bun run check:fork-translations`, `bun run check:branding`, `bun run check:settings` |
| 2   | `shorthand-core`     | `ControlSignal` gains `"toggle-assisted-notes"`; `WireEvent.hello` preserves advertised capabilities; the app-parser comment and tests cover both. Task 12. Then **commit, push, `git tag -a 0.12.0`, push the tag.** | `bun test`, `bun run typecheck`, `bun run build`, `bun run test:e2e` — all four, per `AGENTS.md` ("`bun test` transpiles without typechecking")                                                                                                |
| 3   | `obsidian-shorthand` | **First step: bump the core pin to `0.12.0` and prove the install actually moved.** Then the new command and the recorder wiring. Tasks 13–15.                                                                        | `npm run build` (tsc --noEmit + esbuild), `npm test` (unit + bundle-load smoke)                                                                                                                                                                |

**Why the app source goes first, ahead of core.** Core's new `ControlSignal` member is a promise that a compatible `shorthand --toggle-assisted-notes` parses. Landing the app commit first makes the source-level promise true before core publishes the type. It does **not** prove the user installed that binary: against an older installed app the control child exits non-zero on clap's `unexpected argument`, and `ShorthandControl.send()` maps that close at `shorthand-core/src/stream/control.ts:109-116` to `{status: "error", message: stderr}`.

**How install ordering is enforced at runtime: follower capability negotiation.** The app adds `capabilities: ["toggle-assisted-notes"]` to `hello`; core preserves the optional string array; the plugin will not send the new flag until its own follower has received a `hello` containing that capability. An older binary therefore gets a direct update notice and no unsupported control spawn. This is better than a minimum version comparison: it checks the exact contract needed, works for custom builds, and does not guess that every binary bearing a particular version has the same feature set. It is an additive protocol-1 field under `FOLLOW_STREAM.md`'s existing unknown-field rule, so no protocol bump is needed.

**Why the tag is not the end.** Widening `ControlSignal` is exactly the shape of change `shorthand-core/AGENTS.md` names as breaking-by-construction (an exhaustive switch elsewhere that quietly stops being exhaustive). Step 3 is part of this unit of work, not a follow-up. If it cannot be reached, say so plainly and name what is left undone.

**Version choice for core: `0.12.0`, the minor slot.** On the `0.x` line minor is the breaking slot, and although adding a union member is additive for _producers_ (which is all the plugin is), it is breaking for any consumer that switches on it. Taking the minor costs nothing and keeps the rule mechanical. Do not reach for `0.11.3`.

---

## Global constraints

- **Keep the diff mergeable** (`AGENTS.md` § "Keep the diff mergeable"). Prefer new files. Where an upstream file must be edited, keep the edit small and local; do not reformat, reorder imports, or tidy neighbouring code.
- **Never write to `src/i18n/locales/`.** All new user-facing strings go in `src/shorthand/locales/en.json`. The reviewed fork-catalogue plan is a prerequisite, not a conditional branch in this plan.
- **No hardcoded strings in JSX.** ESLint enforces i18next usage.
- **`src/bindings.ts` is generated.** Use the repository's tauri-specta path: run a debug `bun run tauri dev`, wait for export, stop it, and review the generated diff. Do not hand-edit generated output.
- **`apply_mode` stays pure** and unit-tested. Every new behaviour that can be expressed there rather than at an `AppHandle`-holding call site, should be.
- **The mode cell's doc comments are load-bearing.** `mode.rs` explains precisely why the cell is process-wide and why it is never cleared. Both reasons survive the `AtomicBool` → `AtomicU8` change unchanged; carry the comments across, do not rewrite them into something vaguer.
- **Comments record the real reason and name the failure they prevent.** Never restate the code.
- Conventional commit prefixes; the message explains _why_.

---

## Binding ids — the answer, and the evidence

Read `src-tauri/src/shorthand/mode.rs::mode_for_binding`, `src-tauri/src/transcription_coordinator.rs::is_transcribe_binding`, `src-tauri/src/actions.rs::ACTION_MAP`, and the five registration-guard sites (below). The existing pattern is `<verb>` and `<verb>_with_post_process`.

**New ids: `assisted_notes` and `assisted_notes_with_post_process`.**

**Is the `_with_post_process` variant needed? Yes — it is the only way the mode can ever run AI cleanup.** `TranscribeAction` carries `post_process: bool` as a field of the action, set at `ACTION_MAP` construction from _which binding fired_ (`actions.rs:1173-1188`). `process_transcription_output` reads that flag and nothing else — it never consults `settings.post_process_enabled`, which gates only _registration_ of the `_with_post_process` binding and the UI rows. So without a second binding, `assisted_notes.post_process_enabled` and `assisted_notes.post_process_selected_prompt_id` would be permanently dead fields: settings the UI offers that cannot affect any capture. That is precisely the untruthfulness decision 1 exists to prevent.

Default combos (all must remain pairwise distinct on each platform — `default_bindings_have_distinct_shortcuts_on_this_platform` in `settings.rs` covers only the host platform, so the other two are a review-time reading of the `cfg` branches):

| Platform | `assisted_notes` | `assisted_notes_with_post_process` |
| -------- | ---------------- | ---------------------------------- |
| Windows  | `ctrl+alt+n`     | `ctrl+alt+shift+n`                 |
| macOS    | `ctrl+option+n`  | `ctrl+shift+option+n`              |
| Linux    | `ctrl+alt+n`     | `ctrl+alt+shift+n`                 |
| other    | `alt+n`          | `alt+shift+n`                      |

`space` is exhausted: the four existing transcribe/dictate combos use every ctrl/alt/shift permutation of it on Windows and Linux, and `alt+space`/`shift+space`/`win+space` all belong to the OS. `n` for "notes" is the first free letter that reads as a mnemonic. `change_binding` rejects an empty binding (`shortcut/mod.rs:118`), so "ship it unbound" is not available.

---

## The settings-schema question — definitive

The new `assisted_notes` key defaults in through `#[serde(default)]`, so no migration or schema-version bump is needed. There is exactly one user and no installed base to protect; stored values win over defaults, which is fine.

---

## The locale question — definitive

There are **24** directories under `src/i18n/locales/` (`ar bg cs da de en es fr he hi it ja ko ne nl pl pt ru sv tr uk vi zh zh-TW`), not 25.

**None of them need an entry. Every new string goes in `src/shorthand/locales/en.json`.**

Why, given that the dictation branch did add binding keys to all 24 files:

- The prerequisite fork-catalogue plan keeps upstream catalogues byte-identical and gives translatable fork content its own locale-aware catalogue. Assisted Notes strings are genuinely fork-only, so they belong in that catalogue rather than `english-copy.json`.
- `check:translations` compares key parity between `en` and the other 23 catalogues **on disk**. Fork-only strings never reach disk, so it cannot fail on them. Adding keys to `en/translation.json` alone _would_ fail it; adding them to all 24 is 24 files of churn in the files upstream touches most.
- `check:branding` still sees the exported union and asserts each deliberate fork key survives the merge. `check:fork-translations` checks raw fork catalogues when translated files are added.

Only `en.json` exists today, so other locales fall back to English until contributors add matching fork catalogues.

---

## `AssistedNotesSettings` — the field-by-field default table

Included fields, and how each default is justified against Meeting's (`AppSettings`) and Dictation's:

| Field                             | Assisted Notes                       | Meeting                                    | Dictation                         | Justification                                                                                                                                                                                                                                                                                                                  |
| --------------------------------- | ------------------------------------ | ------------------------------------------ | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `enabled`                         | `false`                              | n/a — Meeting cannot be switched off       | `false`                           | Enabling registers two global shortcuts, which can collide with another app. Fork-only features ship off (`AGENTS.md` § "Give fork-only features a boundary").                                                                                                                                                                 |
| `push_to_talk`                    | `false`                              | `false`                                    | `true`                            | Meeting's reasoning applies unchanged: a note-taking session runs as long as the thinking does, and nobody holds a key for that. Only Dictation, which is seconds long, is held.                                                                                                                                               |
| `clipboard_handling`              | `ClipboardHandling::default()`       | user's value                               | `ClipboardHandling::default()`    | **Per-mode despite the mode never pasting.** `clipboard::paste()` runs its tail regardless of paste method: the `CopyToClipboard` branch at `clipboard.rs:808` writes the transcript to the clipboard even under `PasteMethod::None`. Omitting this field would let Meeting's value silently govern an Assisted Notes capture. |
| `append_trailing_space`           | `false`                              | `false`                                    | `false`                           | Live for the same reason: the appended text is what `write_text_to_clipboard` receives (`clipboard.rs:730` then `:808`). Off, matching both modes.                                                                                                                                                                             |
| `overlay_style`                   | `OverlayStyle::Minimal`              | `Minimal`                                  | `Minimal`                         | The compact pill. The Live panel would sit on top of the note being filled in — the exact window the user is watching. Same reasoning as Meeting's `default_overlay_style_is_minimal`, and stronger here, because the enhanced note _is_ the live view.                                                                        |
| `save_recordings`                 | **`true`**                           | `false`                                    | **`true`** (flipped by this plan) | Decided by the owner. See Task 3 for the comment that must replace the current one.                                                                                                                                                                                                                                            |
| `save_transcripts`                | **`true`**                           | `false`                                    | **`true`** (flipped by this plan) | Same.                                                                                                                                                                                                                                                                                                                          |
| `post_process_enabled`            | `false`                              | `false` (`default_post_process_enabled()`) | `false`                           | Cleanup needs a configured provider and API key. Nothing that calls a remote endpoint can ship on.                                                                                                                                                                                                                             |
| `post_process_selected_prompt_id` | `None`                               | `None`                                     | `None`                            | Falls through to the shared prompt library's default.                                                                                                                                                                                                                                                                          |
| `follow_stream_enabled`           | **`true`**                           | `true`                                     | `false`                           | **The defining similarity to Meeting.** A follower filling the note is the entire reason this mode exists. Dictation's text has already arrived where it was wanted.                                                                                                                                                           |
| `post_process_provider_id`        | `default_post_process_provider_id()` | same                                       | same                              | Same provider until someone chooses otherwise, so behaviour is unchanged for anyone who never sets one.                                                                                                                                                                                                                        |
| `post_process_model`              | `None`                               | n/a (the shared `post_process_models` map) | `None`                            | `None` leaves the shared provider→model map exactly as it was.                                                                                                                                                                                                                                                                 |

**Deliberately absent from the struct**, with what governs them instead:

| Not a field                      | What happens instead                                                 | Why                                                                                                                                                                                                                                                                                                                                                             |
| -------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `paste_method`                   | `apply_mode` hardcodes `PasteMethod::None` for `Mode::AssistedNotes` | "Never types into the focused window" is the mode's _definition_, not a preference inside it. The top-level value is Meeting's Advanced escape hatch; a user who flipped it there must not thereby make Assisted Notes paste. Making it a field would also make it settable through `change_assisted_notes_settings` — a settings surface for a mode invariant. |
| `system_audio_enabled`           | `apply_mode` hardcodes `false` for `Mode::AssistedNotes`             | "Records only your microphone" is also part of the mode's definition. A default-off field and a Windows toggle would let the UI break that promise. The selected loopback device remains shared but unreachable in this mode.                                                                                                                                   |
| `typing_tool`                    | inherits Meeting's; unreachable                                      | Only read inside `paste_direct` on Linux, which `PasteMethod::None` never reaches.                                                                                                                                                                                                                                                                              |
| `auto_submit`, `auto_submit_key` | inherit Meeting's; unreachable                                       | `should_send_auto_submit(auto_submit, PasteMethod::None)` returns `false` unconditionally (`clipboard.rs:721`, pinned by `clipboard.rs:905`).                                                                                                                                                                                                                   |

Placing four fields on a struct that provably cannot affect anything would be four settings the UI would then have to either show (untruthfully) or hide (unreachably). Both are worse than the asymmetry.

---

## Repo 1 — `shorthand-app`

### Task 1: Three modes in the mode cell

**Files:** `src-tauri/src/shorthand/mode.rs`

**Interfaces:** produces `Mode::AssistedNotes` and the two new binding ids, consumed by every later task in this repo.

- [ ] **Step 1: extend `Mode` and replace the `AtomicBool`.**

The module doc comment and the `static`'s doc comment both stay — their reasoning (one capture at a time, so a process-wide cell is unambiguous; never cleared, because "the mode of the most recently started capture" is the right answer for async work that outlives the recording) is unaffected by there being three modes. Carry them across verbatim, adjusting only the sentence that says "flag".

```rust
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Meeting,
    Dictation,
    AssistedNotes,
}

impl Mode {
    /// The cell's stored representation. Written out rather than derived from
    /// a `#[repr(u8)]` cast, so reordering the variants can never silently
    /// reinterpret a value already sitting in the cell.
    const fn as_repr(self) -> u8 {
        match self {
            Mode::Meeting => 0,
            Mode::Dictation => 1,
            Mode::AssistedNotes => 2,
        }
    }

    /// Unknown values fall back to `Meeting`, for the same reason `active`
    /// does: meeting behaviour is what every code path did before this module
    /// existed.
    const fn from_repr(value: u8) -> Mode {
        match value {
            1 => Mode::Dictation,
            2 => Mode::AssistedNotes,
            _ => Mode::Meeting,
        }
    }
}

static ACTIVE_MODE: AtomicU8 = AtomicU8::new(Mode::Meeting.as_repr());
```

`set_active` becomes `ACTIVE_MODE.store(mode_for_binding(binding_id).as_repr(), Ordering::Release);` and `active` becomes `Mode::from_repr(ACTIVE_MODE.load(Ordering::Acquire))`. Keep both signatures — including the unused `_app: &AppHandle` — exactly as they are; every call site depends on them.

- [ ] **Step 2: extend `mode_for_binding` and its doc comment.**

```rust
/// "dictate*" ids are dictation, "assisted_notes*" ids are assisted notes;
/// every other binding id (including ones this module doesn't know about) is
/// meeting.
pub fn mode_for_binding(binding_id: &str) -> Mode {
    match binding_id {
        "dictate" | "dictate_with_post_process" => Mode::Dictation,
        "assisted_notes" | "assisted_notes_with_post_process" => Mode::AssistedNotes,
        _ => Mode::Meeting,
    }
}
```

- [ ] **Step 3: tests.**

Extend `mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting` — rename it to `mode_for_binding_maps_each_modes_ids_and_defaults_everything_else_to_meeting` and add the two new ids plus a re-assertion that `transcribe`, `cancel` and `unknown` are still `Meeting`.

Add `mode_repr_round_trips_every_variant`: for each of the three variants, `assert_eq!(Mode::from_repr(mode.as_repr()), mode)`, plus `assert_eq!(Mode::from_repr(200), Mode::Meeting)`. This is what makes the `AtomicBool` → `AtomicU8` swap safe; without it, a mis-numbered arm is invisible.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml mode`

---

### Task 2: `AssistedNotesSettings`

**Files:** create `src-tauri/src/shorthand/assisted_notes.rs`; modify `src-tauri/src/shorthand/mod.rs`, `src-tauri/src/settings.rs`

**Interfaces:** produces `pub struct AssistedNotesSettings` and `AppSettings::assisted_notes`, consumed by Tasks 3–11.

- [ ] **Step 1: the new module.** Its own file, mirroring the boundary `dictation.rs` has. Header comment: what the mode is, and that the per-mode _resolver_ deliberately stays in `dictation.rs` (see Task 3) so the seven upstream call sites of `dictation::resolve_settings` are not touched.

Fields in the order given in the default table above, each with `pub`, the struct deriving `Serialize, Deserialize, Debug, Clone, Type` and carrying `#[serde(default)]`. Write a hand-rolled `impl Default` (not a derive) so each default can carry the comment that justifies it; copy the _shape_ of `DictationSettings::default()`.

The two comments that must be present, because they are the mode's definition:

```rust
  // The defining similarity to a meeting: a follower process filling a note is
  // the entire reason this mode exists.
  follow_stream_enabled: true,
```

For `save_recordings` / `save_transcripts`, use the comment drafted in Task 3 Step 1 (the same reasoning applies to both structs; write it once in full in `dictation.rs` and point at it from here, or repeat it — do not paraphrase it into something weaker).

- [ ] **Step 2: register the module** in `src-tauri/src/shorthand/mod.rs` alongside `dictation` and `mode`.

- [ ] **Step 3: the `AppSettings` field.** Immediately after `pub dictation`, so the per-mode structs stay together:

```rust
/// Assisted-notes settings, applied over the equivalent fields above when
/// the capture in flight is `shorthand::mode::Mode::AssistedNotes`. See
/// `shorthand::dictation::apply_mode`.
#[serde(default)]
pub assisted_notes: crate::shorthand::assisted_notes::AssistedNotesSettings,
```

Add `assisted_notes: Default::default(),` in the matching position inside `get_default_settings()`. Do **not** add a `default_*()` helper — the struct's own `Default` is the single source.

- [ ] **Step 4: tests** in `settings.rs`'s `mod tests`:
  - extend `empty_store_parses_with_defaults` with `assert!(settings.assisted_notes.follow_stream_enabled)`, `assert!(settings.assisted_notes.save_recordings)`, and `assert!(settings.assisted_notes.save_transcripts)`;
  - extend `per_mode_defaults_differ_where_the_modes_differ` with the assisted-notes column: `follow_stream_enabled` true like Meeting, `push_to_talk` false like Meeting and unlike Dictation, provider id equal to the shared one, and `post_process_model` `None`;
  - extend `default_overlay_style_is_minimal` (non-Linux) with `settings.assisted_notes.overlay_style`;
  - run `frozen_v0_9_store_parses_strictly_and_migrates_only_paste_method` and confirm it stays green without edits.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml settings`

---

### Task 3: Flip Dictation's save defaults, and fix the comment that describes them

**Files:** `src-tauri/src/shorthand/dictation.rs`

This is a change to existing behaviour and is deliberately its own task so it can be reviewed alone.

- [ ] **Step 1: flip the defaults and replace the comment.** In `DictationSettings::default()`, `save_recordings` and `save_transcripts` become `true`. The comment above them currently reads "Consent, not preference — stays opt-in like meeting mode's equivalent toggles." That will be false the moment this lands, and a comment that contradicts the code is worse than none. Replace it with the reasoning that actually decides it:

```rust
// Dictation keeps its own audio and text by default. Meeting's top-level
// defaults stay off because a meeting recording can include other participants.
save_recordings: true,
save_transcripts: true,
```

- [ ] **Step 2: pin the new defaults.** In `dictation.rs`'s `mod tests`:

```rust
#[test]
fn note_producing_modes_save_by_default() {
    assert!(DictationSettings::default().save_recordings);
    assert!(DictationSettings::default().save_transcripts);
    assert!(AssistedNotesSettings::default().save_recordings);
    assert!(AssistedNotesSettings::default().save_transcripts);
    // Meeting is deliberately not part of this change.
    let settings = crate::settings::get_default_settings();
    assert!(!settings.save_recordings);
    assert!(!settings.save_transcripts);
}
```

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml`

---

### Task 4: `apply_mode`'s third arm

**Files:** `src-tauri/src/shorthand/dictation.rs`

`dictation.rs` keeps `apply_mode`, `resolve_settings` and `resolve_push_to_talk` for all three modes. Moving them to a neutrally-named module would be tidier, but `crate::shorthand::dictation::resolve_settings` appears at seven call sites in upstream-owned files (`actions.rs` ×4, `overlay.rs`, `clipboard`-adjacent paths) and renaming it would edit every one of them for no behavioural gain. Update the module header instead to say it is the per-mode resolver for every mode, and why the name did not change.

- [ ] **Step 1: `resolve_push_to_talk`.** Add the third arm. Its doc comment already explains why this resolver derives the mode from `binding_id` rather than the cell (it runs at dispatch time, before `set_active`); that reasoning is unchanged.

```rust
match mode::mode_for_binding(binding_id) {
    Mode::Dictation => settings.dictation.push_to_talk,
    Mode::AssistedNotes => settings.assisted_notes.push_to_talk,
    Mode::Meeting => settings.push_to_talk,
}
```

- [ ] **Step 2: `apply_mode`.** Add two arms after the dictation ones, keeping the defence-in-depth guard shape:

```rust
// Defence in depth, exactly as for dictation: a binding registered while the
// mode is off must still behave like meeting mode rather than half like a
// mode the user has not switched on.
Mode::AssistedNotes if !settings.assisted_notes.enabled => settings,
Mode::AssistedNotes => {
    let assisted = settings.assisted_notes.clone();
    AppSettings {
        push_to_talk: assisted.push_to_talk,
        // Not a field on AssistedNotesSettings. Delivering to follower
        // processes instead of the focused window is what *defines* this
        // mode, so "never paste" is an invariant of the mode rather than a
        // preference inside it. The top-level value is meeting mode's
        // Advanced escape hatch; a user who flipped it there must not
        // thereby make assisted notes type into whatever window has focus.
        paste_method: PasteMethod::None,
        clipboard_handling: assisted.clipboard_handling,
        append_trailing_space: assisted.append_trailing_space,
        overlay_style: assisted.overlay_style,
        save_recordings: assisted.save_recordings,
        save_transcripts: assisted.save_transcripts,
        post_process_enabled: assisted.post_process_enabled,
        post_process_selected_prompt_id: assisted.post_process_selected_prompt_id,
        // Solo capture is a mode invariant, not a preference. Meeting's
        // system-audio setting must never leak into Assisted Notes.
        system_audio_enabled: false,
        follow_stream_enabled: assisted.follow_stream_enabled,
        post_process_provider_id: assisted.post_process_provider_id.clone(),
        post_process_models: {
            let mut models = settings.post_process_models.clone();
            if let Some(model) = assisted.post_process_model.clone() {
                models.insert(assisted.post_process_provider_id.clone(), model);
            }
            models
        },
        ..settings
    }
}
```

- [ ] **Step 3: tests.** How the three existing tests change, and what is added:

| Existing test                                                      | Change                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default_paste_method_is_not_none`                                 | Unchanged. Add a sibling, `assisted_notes_never_pastes` (below), rather than extending it — the two assert opposite things for opposite reasons.                                                                                                                                                      |
| `resolve_push_to_talk_reads_the_matching_mode_field`               | Extend: set `settings.assisted_notes.push_to_talk = false` while `settings.push_to_talk = true`, and assert `!resolve_push_to_talk(&settings, "assisted_notes")` and the same for `assisted_notes_with_post_process`. Keep the existing four assertions.                                              |
| `apply_mode_leaves_every_field_unchanged_for_meeting`              | Extend: give every `settings.assisted_notes.*` field a value that differs from the matching top-level field, exactly as the test already does for `settings.dictation.*`, and add assertions that none of them reached the result. Without this the test would silently stop covering the new struct. |
| `apply_mode_overrides_every_per_mode_field_for_dictation`          | Extend by one line: also populate `settings.assisted_notes` with distinct values and assert they did **not** leak into the dictation result.                                                                                                                                                          |
| `apply_mode_leaves_settings_unchanged_for_dictation_when_disabled` | Unchanged.                                                                                                                                                                                                                                                                                            |

New tests:

- `apply_mode_overrides_every_per_mode_field_for_assisted_notes` — mirrors the dictation version field for field: set `assisted_notes.enabled = true`, give every per-mode field a value distinct from the top-level one, assert each one landed, assert the per-mode model override reached `post_process_models`, and assert a field `apply_mode` does not own (`selected_model`) survived.
- `assisted_notes_never_pastes` — set `settings.paste_method = PasteMethod::CtrlV` (the Advanced escape hatch) _and_ `assisted_notes.enabled = true`, then assert `apply_mode(settings, Mode::AssistedNotes).paste_method == PasteMethod::None`. This is the invariant; it must fail if someone turns `paste_method` into a field.
- `assisted_notes_never_captures_system_audio` — set the top-level Meeting field to `true`, enable Assisted Notes, and assert the resolved `system_audio_enabled` is `false`. This must fail if someone later turns the invariant into a setting.
- `apply_mode_leaves_settings_unchanged_for_assisted_notes_when_disabled` — the serialization-comparison shape of the dictation equivalent (`AppSettings` has no `PartialEq`, and adding one would ripple into upstream types).

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml dictation`

---

### Task 5: Follow-stream publication and listener lifetime

**Files:** create `src-tauri/src/follow_stream/lifecycle.rs`; modify `src-tauri/src/follow_stream/mod.rs`, `src-tauri/src/follow_stream/protocol.rs`, `src-tauri/src/follow_stream/hub.rs`, `src-tauri/src/actions.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/shortcut/mod.rs`, `src-tauri/src/lib.rs`, and `FOLLOW_STREAM.md`

Confirmed against the current tree: `actions.rs:600` gates `hub.begin()` on `mode::active(app) == Mode::Meeting`; `lib.rs:233-250` starts and rolls back the listener solely from the top-level Meeting field; and `commands/mod.rs:46-75` starts/stops it solely from `change_follow_stream_enabled_setting`. A mode can therefore resolve `follow_stream_enabled: true` while no follower can connect. Changing only `hub.begin()` would not make Dictation's switch live and would give Assisted Notes the same broken promise.

This plan chooses the **OR-of-publishing-enabled-modes** policy rather than running the transport unconditionally. It preserves the current off state when no mode can publish, while separating listener ownership from Meeting: Meeting requests it directly; Dictation and Assisted Notes request it only when that optional mode is enabled and its own publication field is on.

- [ ] **Step 1: read the resolved publication setting instead of the mode.**

```rust
// Which captures reach the follow-stream hub is the *resolved*
// `follow_stream_enabled` value, not the mode: that is the switch the Modes
// pane shows per mode, and gating on anything else makes the UI describe a
// capture it is not governing. Meeting ships true, dictation false, assisted
// notes true. Skipping `begin` alone is sufficient: every terminal hub call
// and `partial` check for an active session first and silently no-op without
// one (pinned in follow_stream::hub::tests).
if crate::shorthand::dictation::resolve_settings(app).follow_stream_enabled {
    if let Some(hub) = crate::follow_stream::hub(app) {
        hub.begin(model_supports_streaming);
    }
}
```

`set_active` has already run at line 550, so `resolve_settings` is correct here. Keep this local read rather than refactoring the surrounding start path.

- [ ] **Step 2: add a pure listener policy in the new lifecycle module.**

  ```rust
  pub fn listener_required(settings: &AppSettings) -> bool {
      settings.follow_stream_enabled
          || (settings.dictation.enabled && settings.dictation.follow_stream_enabled)
          || (settings.assisted_notes.enabled
              && settings.assisted_notes.follow_stream_enabled)
  }
  ```

  Meeting has no enable switch, so its term is the top-level field. Disabled optional modes do not keep the listener alive. Unit-test the full truth table, not three spot checks. Derive the three effective inputs — Meeting publication, `dictation.enabled && dictation.follow_stream_enabled`, and `assisted_notes.enabled && assisted_notes.follow_stream_enabled` — and cover all eight combinations:

  | Meeting | Dictation | Assisted Notes | Required |
  | ------- | --------- | -------------- | -------- |
  | off     | off       | off            | no       |
  | on      | off       | off            | yes      |
  | off     | on        | off            | yes      |
  | off     | off       | on             | yes      |
  | on      | on        | off            | yes      |
  | on      | off       | on             | yes      |
  | off     | on        | on             | yes      |
  | on      | on        | on             | yes      |

  Add separate cases proving an optional mode with publication on but `enabled: false` contributes `false`. Those cases guard the difference between a stored preference and a mode that can actually publish.

- [ ] **Step 3: add one async reconciler beside the policy.** It acquires `FollowStreamServer::lock_lifecycle()`, starts the server idempotently when `listener_required(candidate)` is true, and stops it otherwise. It accepts the complete candidate `AppSettings`; callers do not pass a boolean or duplicate the policy.

- [ ] **Step 4: use the reconciler in all three settings commands.**
  - `change_follow_stream_enabled_setting` treats the argument as Meeting's publication preference. Build candidate settings, reconcile, then persist. Turning Meeting off leaves the listener running when enabled Dictation or Assisted Notes still needs it.
  - Convert `change_dictation_settings` to `async`. After shortcut registration succeeds but before persistence, reconcile the candidate. If listener startup fails, restore the previous shortcut registrations, reconcile the previous settings, and return the error without writing.
  - `change_assisted_notes_settings` follows the same transaction shape in Task 8. Its generated TypeScript surface remains a promise, so making the Rust command async does not change frontend call sites.

- [ ] **Step 5: use the same policy at startup.** Replace both `initial_settings.follow_stream_enabled` checks in `lib.rs` with `listener_required`. If startup fails, log the error and leave the three stored preferences unchanged; there is no longer one owner toggle that can truthfully be rolled back. An interactive settings change still returns the error and keeps the prior settings.

- [ ] **Step 6: advertise the control capability in `hello`.** Add `capabilities` to `FollowEvent::Hello`, populate it with `"toggle-assisted-notes"` in `FollowStreamHub::subscribe`, and update the exact-wire tests in `protocol.rs`, `hub.rs`, `server.rs`, and `client.rs`. This advertises parser support, not whether the mode is enabled. It is additive under protocol 1; old consumers ignore it.

- [ ] **Step 7: update `FOLLOW_STREAM.md`.** Separate two concepts explicitly: per-mode publication (`follow_stream_enabled`) and process-wide listener lifetime (the OR policy above). Remove any statement that Meeting's toggle alone describes whether the socket exists. Update the `hello` examples and field description with `"capabilities":["toggle-assisted-notes"]`, state that capabilities name supported control flags rather than enabled settings, and preserve the protocol-1 additive-field rule.

**Behaviour changes to declare in the commit message:** the complete Task 5 makes Dictation's existing Advanced row work, and disabling Meeting publication no longer tears down the listener while another enabled mode needs it. Step 1 alone does neither while the listener remains Meeting-gated. The `hello` capability also lets downstream callers distinguish a compatible installed binary from merely compatible source ordering.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml`, plus the manual matrix in Task 11.

---

### Task 6: Bindings, action map, coordinator, history source

**Files:** `src-tauri/src/settings.rs`, `src-tauri/src/actions.rs`, `src-tauri/src/transcription_coordinator.rs`, `src-tauri/src/managers/history.rs`

- [ ] **Step 1: default bindings** in `get_default_settings()`, immediately after the two `dictate*` inserts, using the combos from the table above and the same `#[cfg(target_os = ...)]` ladder. `name`/`description` on `ShortcutBinding` are the un-i18n'd backend strings (upstream renders the i18n key when one exists) — follow the neighbours: `"Assisted Notes"` / `"Converts your speech into text and streams it to any process following the live transcript, without capturing system audio."` and `"Assisted Notes with Post-Processing"` / the same plus "and applies AI post-processing".

- [ ] **Step 2: `is_transcribe_binding`** in `transcription_coordinator.rs` gains both ids. Extend the existing test at line 294-296.

- [ ] **Step 3: `ACTION_MAP`** in `actions.rs` gains `assisted_notes` → `TranscribeAction { post_process: false }` and `assisted_notes_with_post_process` → `TranscribeAction { post_process: true }`.

- [ ] **Step 4: `source_for_mode`** in `managers/history.rs` gains `Mode::AssistedNotes => "assisted_notes"`. Extend `source_for_mode_maps_meeting_and_dictation` (rename to `source_for_mode_maps_every_mode`) with the third arm.

- [ ] **Step 5:** extend `default_bindings_have_distinct_shortcuts_on_this_platform` in `settings.rs` with the two new ids (the array grows from five to seven) and update its doc comment's "five default bindings" to "seven".

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml`

---

### Task 7: The five registration guards

**Files:** `src-tauri/src/shortcut/mod.rs` (two sites), `src-tauri/src/shortcut/tauri_impl.rs`, `src-tauri/src/shortcut/handy_keys.rs`, `src-tauri/src/secure_input.rs`

Each site already has the pair of dictation guards; add the matching pair, in the same shape, immediately after them:

```rust
if id == "assisted_notes" && !settings.assisted_notes.enabled {
    continue;
}
if id == "assisted_notes_with_post_process"
    && !(settings.assisted_notes.enabled && settings.assisted_notes.post_process_enabled)
{
    continue;
}
```

The five sites and the local name of the settings binding at each:

| File                          | Function                | Local              |
| ----------------------------- | ----------------------- | ------------------ |
| `shortcut/mod.rs` ~261        | `resume_all_shortcuts`  | `settings`         |
| `shortcut/mod.rs` ~461        | the validate/reset loop | `current_settings` |
| `shortcut/tauri_impl.rs` ~30  | init registration       | `user_settings`    |
| `shortcut/handy_keys.rs` ~440 | init registration       | `user_settings`    |
| `secure_input.rs` ~530        | `reconcile_fallback`    | `settings`         |

This is what makes "off by default" true rather than merely hidden: without these guards every init/resume/reconcile path would register both ids.

- [ ] **Step 1:** add the ten guards. Do not refactor the stack into a shared predicate in this change (see Concerns).

**Verify:** `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

---

### Task 8: `change_assisted_notes_settings`

**Files:** `src-tauri/src/shortcut/mod.rs`, `src-tauri/src/lib.rs`

- [ ] **Step 1:** copy the Task 5 version of `change_dictation_settings` into an async `change_assisted_notes_settings`, substituting the struct, field, and two binding ids. Preserve its transaction shape:
  - registration happens **before** `write_settings`, so a failed `register_shortcut` never reaches disk as `enabled = true`;
  - both halves roll back on partial failure, so the running process always matches disk;
  - follow-stream listener reconciliation runs against the candidate settings before persistence; failure restores the previous registrations and listener policy;
  - `describe_registration_failure` is reused, so a collision with `transcribe`/`dictate` is named rather than blamed on "another application";
  - `crate::secure_input::reconcile_fallback(&app)` runs on both the error and the success path.

  Its long comment explains why; adapt it rather than dropping it.

- [ ] **Step 2:** register the command in `collect_commands![...]` in `lib.rs`, next to `shortcut::change_dictation_settings`.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

---

### Task 9: The CLI flag

**Files:** `src-tauri/src/cli.rs`, `src-tauri/src/lib.rs`, `README.md`, `AGENTS.md`

- [ ] **Step 1: `cli.rs`.** Add the flag next to the other two toggles:

```rust
/// Toggle an Assisted Notes capture on/off (sent to running instance)
#[arg(long)]
pub toggle_assisted_notes: bool,
```

and add `"toggle_assisted_notes"` to `--follow-stream`'s `conflicts_with_all` list. That list is quoted verbatim in `shorthand-core/src/stream/control.ts`'s header; Task 12 updates it, and the two must not drift.

- [ ] **Step 2:** extend `follow_stream_argument_shapes_parse_as_documented` with a conflict case:

```rust
let error =
    CliArgs::try_parse_from(["handy", "--follow-stream", "--toggle-assisted-notes"]).unwrap_err();
assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
```

- [ ] **Step 3: `lib.rs` ~883**, in the `tauri_plugin_single_instance` callback, as a new `else if` before the final `else`:

```rust
} else if args.iter().any(|a| a == "--toggle-assisted-notes") {
    // Unlike the two meeting flags, this one names a mode the user can have
    // switched off. Firing it anyway would start a capture that `apply_mode`
    // resolves straight back to meeting settings — including meeting's
    // system-audio toggle — under a name that promises the opposite. The
    // forwarding process has already exited by the time this runs, so its
    // exit code cannot report this refusal. Raise the app as a courtesy; the
    // follower-side bounded acknowledgement is the actual failure signal.
    if settings::get_settings(app).assisted_notes.enabled {
        signal_handle::send_transcription_input(app, "assisted_notes", "CLI");
    } else {
        log::warn!("--toggle-assisted-notes ignored: Assisted Notes is not enabled");
        show_main_window(app);
    }
}
```

`show_main_window` is **not** treated as acknowledgement or sufficient guidance. It opens the existing settings window but does not navigate to Modes → Notetaking → Assisted notes. Task 14 therefore times out waiting for `begin`, tears the capture down, and shows that exact navigation path in Obsidian. The window raise remains useful context beside that explicit notice, but it is not the correctness mechanism.

There is deliberately **no** `--toggle-assisted-notes-post-process`. The CLI covers one binding per mode; `--toggle-post-process` likewise has no dictation counterpart. A second flag is a purely additive follow-up if it is ever wanted.

- [ ] **Step 4: docs.** Add the flag to the table in `AGENTS.md` § "CLI Parameters" and to `README.md` § "CLI Parameters". One row each; do not restructure the tables.

**Verify:** `cargo test --manifest-path src-tauri/Cargo.toml cli`

---

### Task 10: Frontend — bindings, store, components, strings

**Files:** generated `src/bindings.ts`, `src/stores/settingsStore.ts`, `src/shorthand/locales/en.json`, `src/shorthand/ui/OverlayRows.tsx`, `brand-preview/mock-tauri.ts`, and a new `src/shorthand/assisted-notes/` directory

- [ ] **Step 1: regenerate `src/bindings.ts`.** After the Rust struct and command are registered, run `bun run tauri dev`, wait for tauri-specta's debug export, stop the process, and review the diff. It must contain:
  - `changeAssistedNotesSettings(assistedNotes: AssistedNotesSettings)`, copied from `changeDictationSettings` (line 454);
  - `export type AssistedNotesSettings = { ... }` mirroring the Rust field list and order;
  - `assisted_notes?: AssistedNotesSettings` on the `AppSettings` type, immediately after `dictation?`.

- [ ] **Step 2: `src/stores/settingsStore.ts`** — an `assisted_notes` updater entry copied from the `dictation` one at line 211, including the `result.status === "error"` throw (the enable toggle depends on that rejection).

- [ ] **Step 3: components.** Create `src/shorthand/assisted-notes/` mirroring `src/shorthand/dictation/`:
  - `AssistedNotesEnableToggle.tsx` — copy of `DictationEnableToggle`, including the comment explaining why it compares the requested value against the persisted one after the update settles (`updateSetting` swallows the rejection and rolls back, so that is the only signal the store leaves).
  - `AssistedNotesToggleField.tsx` — copy of `DictationToggleField`, keyed to `keyof AssistedNotesSettings`.
  - `AssistedNotesPostProcessPrompt.tsx` — copy of `DictationPostProcessPrompt`.
  - `AssistedNotesClipboardHandling.tsx` — copy of `DictationClipboardHandling`.
  - In `src/shorthand/ui/OverlayRows.tsx`, add `AssistedNotesOverlayStyleRow` beside `DictationOverlayStyleRow`, following that file's existing pattern exactly.

  Duplication rather than a generic `ModeToggleField<M>` is deliberate here — see Concerns.

- [ ] **Step 4: `brand-preview/mock-tauri.ts`** — add `assisted_notes` and `assisted_notes_with_post_process` to the mocked bindings map (around line 112), or the brand preview renders a settings pane with missing rows.

- [ ] **Step 5: strings.** Add the following flat keys to `src/shorthand/locales/en.json`. **No file under `src/i18n/locales/` is touched.** Keep the catalogue's sentence-case rule and alphabetical/grouping convention.

```json
"settings.modes.tabs.notetaking": "Notetaking",
"settings.modes.tabs.notetakingLabel": "Notetaking mode",
"settings.modes.tabs.assistedNotes": "Assisted notes",
"settings.assistedNotes.enable.label": "Enable assisted notes",
"settings.assistedNotes.enable.description": "Turn on a solo note-taking mode. It streams what you say to whatever is following along, the same way Meetings does, but records only your microphone and never types into the window you are in.",
"settings.assistedNotes.enable.shortcutConflict": "Could not enable assisted notes: another application is already using one of its shortcuts. Choose a different shortcut below and try again.",
"settings.assistedNotes.privacy.saveRecordings.label": "Save recordings",
"settings.assistedNotes.privacy.saveRecordings.description": "Keep the audio of each assisted-notes session so you can play it back or re-transcribe it.",
"settings.assistedNotes.privacy.saveTranscripts.label": "Save transcripts",
"settings.assistedNotes.privacy.saveTranscripts.description": "Keep the text of each assisted-notes session in your local history.",
"settings.general.shortcut.bindings.assisted_notes.name": "Assisted notes shortcut",
"settings.general.shortcut.bindings.assisted_notes.description": "The keyboard shortcut to start and stop an assisted-notes session.",
"settings.general.shortcut.bindings.assisted_notes_with_post_process.name": "Assisted notes AI cleanup shortcut",
"settings.general.shortcut.bindings.assisted_notes_with_post_process.description": "Optional: a dedicated shortcut that always applies AI cleanup to an assisted-notes session.",
"settings.history.source.assistedNotes": "Assisted notes",
```

Also **update** the existing `settings.modes.description`, which describes two modes and would now be wrong:

```json
"settings.modes.description": "Meetings and Assisted notes both stream what they hear to whatever is following along. Meetings can include the other participants; Assisted notes records only you. Dictation types what you say into the focused window.",
```

- [ ] **Step 6: history badge.** `src/components/settings/history/HistorySettings.tsx:373-375` is a two-way ternary on `entry.source`. Widen it to three, keeping the change to those three lines:

```tsx
{
  entry.source === "dictation"
    ? t("settings.history.source.dictation")
    : entry.source === "assisted_notes"
      ? t("settings.history.source.assistedNotes")
      : t("settings.history.source.meeting");
}
```

**Verify:** `bun run test:unit && bun run lint && bun run build && bun run check:translations && bun run check:fork-translations && bun run check:branding`

---

### Task 11: The Modes pane — the Notetaking group

**Files:** `src/shorthand/settings/ModesSettings.tsx`

- [ ] **Step 1: two-level tabs.** Level 1 is `Notetaking | Dictation`; level 2, rendered only inside the Notetaking panel, is `Meetings | Assisted notes`. Both use the existing `Tabs`/`TabPanel` from `src/shorthand/ui/Tabs.tsx` **unchanged** — two independent tablists, each with its own `aria-label`, which keeps the WAI-ARIA tabs pattern intact. A single flat tablist of three cannot express a group without inventing markup inside `role="tablist"` that screen readers handle inconsistently.

  Defaults are `notetaking` then `meetings`, so the pane a user opens on is byte-for-byte the pane they open on today. Both selections stay component state, not settings, for the reason already recorded in the file's header: a view position is not a preference, and persisting it would open the app on a screen describing a mode the user may since have switched off.

```tsx
type ModeTab = "notetaking" | "dictation";
type NotetakingTab = "meetings" | "assisted";
```

- [ ] **Step 2: the Assisted notes panel.** Mirror the Dictation panel's structure — enable toggle first, everything else hidden (not greyed) while the mode is off, for the reason the file already records: a "disabled" `ShortcutInput` still registers a live global hotkey. Rows, in order:
  - `<ShortcutInput shortcutId="assisted_notes" descriptionMode="inline" grouped />`
  - `AssistedNotesToggleField field="push_to_talk"` (reusing `settings.general.pushToTalk.*`)
  - `AssistedNotesOverlayStyleRow descriptionMode="tooltip"` — tooltip, matching both other tabs, because that description outweighs every control around it when inline
  - `AssistedNotesToggleField field="save_recordings"` / `field="save_transcripts"` (the `settings.assistedNotes.privacy.*` keys)
  - `<AdvancedOnly>`:
    - `AssistedNotesToggleField field="post_process_enabled"`, with `<Dependents on={assistedPostProcessEnabled}>` wrapping `<ShortcutInput shortcutId="assisted_notes_with_post_process" />` and `<AssistedNotesPostProcessPrompt />`
    - `field="follow_stream_enabled"`, `AssistedNotesClipboardHandling`, `field="append_trailing_space"`

  Keep the cleanup toggle and both dependents inside the same `<AdvancedOnly>` boundary, matching Meetings. Do not mirror Dictation's default-view placement here: the AI-cleanup page warns that enabling cleanup for notetaking is an advanced setting and is not recommended, so both notetaking panels must make that sentence true.

  **No system-audio, paste-method, typing-tool, or auto-submit rows.** System audio and paste are fixed mode invariants; the other fields are unreachable because paste is `None`.

- [ ] **Step 3: update the file header.** Its membership rule currently reads "_a row is per-mode iff it has a `DictationSettings` counterpart or a mode-specific binding id_", and it names two modes. Restate it for three: _a row is per-mode iff it has a counterpart on the mode's own settings struct, or a mode-specific binding id_ — and add a fourth entry to the list of stated exceptions: `paste_method` is per-mode for Dictation and a fixed invariant for Assisted Notes, so it appears in the Dictation tab and nowhere else.

- [ ] **Step 4: `anyPushToTalk`.** The Cancel row's predicate hides it while _any_ mode has push-to-talk on. Extend it with the assisted-notes term, keeping the existing `enabled &&` guard shape — without it, a mode the user never switched on would suppress the row (that exact bug is recorded in the file).

**Verify:** `bun run lint && bun run build && bun run check:settings`, then a manual pass in `bun run tauri dev`:

1. Modes opens on Notetaking → Meetings, visually unchanged from before.
2. Enabling Assisted notes registers the shortcut and does not spring back.
3. An assisted-notes capture with `shorthand --follow-stream` attached emits `begin`/`partial`/`final`, does **not** paste into the focused window, and captures no system audio. There is no control that can enable system audio for this mode.
4. Turn Meeting publication off while Assisted Notes remains enabled with follow-stream on. A new follower can still attach and Assisted Notes still emits. Disable Assisted Notes publication too; with no enabled mode requesting publication, the listener stops.
5. A Dictation capture still emits nothing to the follower with its default settings, and does emit once its Advanced follow-stream row is switched on. With Meeting and Assisted Notes publication off, enabling that Dictation row starts the listener.
6. History rows from an assisted-notes capture show the "Assisted notes" badge.

---

## Repo 2 — `shorthand-core`

### Task 12: `ControlSignal` and follower capability negotiation

**Files:** `src/stream/control.ts`, `src/stream/client.ts`, `test/control.test.ts`, `test/client.test.ts`

- [ ] **Step 1:** widen the union and update the header comment, which quotes the app's `conflicts_with_all` list verbatim and would otherwise be a comment describing behaviour the code does not have:

```ts
/**
 * ...
 * Control must be its own short-lived spawn, never an extra argument on the
 * follower: Shorthand's parser declares `--follow-stream` as
 * `conflicts_with_all = ["toggle_transcription", "toggle_post_process", "cancel", "toggle_assisted_notes"]`,
 * so a combined invocation fails to parse.
 *
 * `toggle-assisted-notes` selects an app-owned capture mode by name. It carries
 * no settings values, deliberately: the app's own settings pane has to remain
 * the only description of how a running capture behaves, so this surface stays
 * a fixed list of mode selectors rather than an override channel.
 */
export type ControlSignal =
  | "toggle-transcription"
  | "toggle-post-process"
  | "toggle-assisted-notes"
  | "cancel";
```

Nothing else in `control.ts` changes: `#spawn` already builds `--${signal}`, and the flags are toggles, which is why the type is a plain union of flag names.

- [ ] **Step 2:** extend the `signals` array in `test/control.test.ts:87` with `"toggle-assisted-notes"`, so the "every signal spawns `--<signal>`" case covers it.

- [ ] **Step 3: preserve advertised capabilities from `hello`.** Widen the hello member of `WireEvent` in `src/stream/client.ts` with `capabilities?: string[]`. Add a small `stringArrayField` parser that accepts only an array whose every member is a string; copy the array into the parsed record. A missing field remains valid for older protocol-1 apps and means "no advertised optional capability." A malformed field is omitted, not trusted.

  Extend `test/client.test.ts` with:
  - a hello containing `capabilities: ["toggle-assisted-notes"]` parses with that exact array;
  - a hello with no capabilities still parses, proving old apps remain wire-compatible;
  - a malformed capabilities value is not treated as support;
  - the existing unknown-field behavior remains green.

- [ ] **Step 4:** `grep -rn "ControlSignal\|WireEvent" src bin test` and confirm nothing switches exhaustively on the widened types. As of writing, `ShorthandControl` only interpolates the signal and transcript ingestion ignores `hello`; do this check rather than trusting this sentence. `AGENTS.md` records that a widened union once made plugin enhancement fail silently forever with every check green.

- [ ] **Step 5:** commit, push, tag, push the tag:

```bash
git tag -a 0.12.0 -m "0.12.0 — advertise assisted-notes control support"
git push origin 0.12.0
git ls-remote --tags origin '0.12.0^{}'   # annotated tags: compare the peeled ref
```

**Verify (all four, per `AGENTS.md`):** `bun test`, `bun run typecheck`, `bun run build`, `bun run test:e2e`

---

## Repo 3 — `obsidian-shorthand`

### Task 13: Bump the pin — first, and prove it moved

**Files:** `package.json`, `package-lock.json`

This is the **first** step in this repo, so everything after it compiles against the real dependency and `main` stays buildable from a clean checkout at every commit.

- [ ] **Step 1:** change the pin to `github:mshish/shorthand-core#0.12.0` and run `npm install`.
- [ ] **Step 2: prove it.** `README.md` § "Bumping core" records the trap: npm can reuse a cached git resolution and leave `package-lock.json` naming the previous commit and `node_modules` holding the previous version, after which a green `tsc --noEmit` proves nothing — the old type still has three members and the new signal is a plain string literal that would fail to typecheck only against the _new_ type.
  - `git diff package-lock.json` must show the `resolved` commit actually change.
  - If it did not, re-run naming the tag explicitly: `npm install "shorthand-core@github:mshish/shorthand-core#0.12.0"`.
  - Confirm the installed copy, not the lockfile: `node -p "require('./node_modules/shorthand-core/package.json').version"`, and `grep toggle-assisted-notes node_modules/shorthand-core/src/stream/control.ts`.
- [ ] **Step 3:** commit `package.json` and `package-lock.json` together.

**Verify:** `npm run build && npm test`

---

### Task 14: The Assisted Notes capture command

**Hard prerequisite:** `2026-08-26-meeting-minimal-overlay-diagnosis.md` is implemented and its real-Tauri rapid stop/start check passes. `ShorthandRecorder::runStart` always sends `cancel` before the selected toggle (`src/recorder.ts:328-344`), which is the exact sequence that lets the old delayed hide erase the newly shown overlay about 300 ms later. Assisted Notes defaults to Minimal, so routing the new command through the recorder deterministically reproduces that bug without the prerequisite.

**Files:** `main.ts`, `src/recorder.ts`, `test/plugin-recorder.test.ts`, `README.md`

- [ ] **Step 1: parameterise the capture start.** `startCaptureOnActiveNote()` gains one argument and nothing else changes:

```ts
async startCaptureOnActiveNote(
  recordingSignal: ControlSignal = "toggle-transcription",
): Promise<void>
```

and passes it through to `new ShorthandRecorder({ recordingSignal, ... })` in place of the current literal.

- [ ] **Step 2: the new command**, beside `start-capture-this-note`, with the same `checkCallback` shape (Obsidian hides a command whose check returns false, which is its prescribed way to say "needs an open Markdown note"; the check runs on every palette render, so it must not fire a Notice):

```ts
this.addCommand({
  id: "start-assisted-notes-capture-this-note",
  name: "Start assisted notes capture on this note",
  checkCallback: (checking: boolean) => {
    if (!this.hasActiveMarkdownFile()) return false;
    if (checking) return true;
    void this.startCaptureOnActiveNote("toggle-assisted-notes");
    return true;
  },
});
```

Command names carry no plugin prefix and are sentence case, per the note already in `onload()`.

- [ ] **Step 3: make the follower handshake a capability gate.** The current `attached` promise resolves to `void` on any `hello` (`main.ts:322-367`). Preserve the parsed hello record instead, and give `ShorthandRecorder` an optional required capability for the selected signal. For `toggle-assisted-notes`, require `"toggle-assisted-notes"`; Meeting supplies no requirement and keeps its current attach-grace behavior.

  When a capability is required:
  - do not let `attachGraceMs` fall through to control signalling without a hello;
  - if hello is absent by the grace deadline, return a not-started outcome and show a notice that a compatible running Shorthand with live transcript following is required;
  - if hello arrives without the capability, return not-started **without sending either `cancel` or the unsupported flag**, stop the capture runtime, and tell the user to install the Shorthand build that supports Assisted Notes;
  - if the capability is present, continue with the existing sequential cancel-then-toggle path.

  This is the runtime install-order contract. Source landing order alone cannot prove which app binary the user installed, and `ShorthandControl.send()` would otherwise surface the older binary's clap stderr only after the plugin had already entered capturing state.

- [ ] **Step 4: add a bounded start acknowledgement.** The current `beginGraceMs` is referenced only inside `stop()` at `src/recorder.ts:391-397`; it is not a start backstop. A confirmed control child merely proves the flag reached the running app. In the disabled-mode path that child already exited 0 before the primary instance refuses the flag, so `#expectingSession` stays true indefinitely and Obsidian stays in its capturing state.

  Add an explicit start outcome and an Assisted-Notes acknowledgement budget to `ShorthandRecorder`:

  ```ts
  export type RecorderStartOutcome = "started" | "not-started" | "stopped";
  ```

  For the capability-gated Assisted Notes path, after the toggle returns `sent`, wait for the first session-scoped record (`begin`, or the existing accepted partial-as-begin path) for a bounded `startAcknowledgementMs`. Then:
  - acknowledgement → return `"started"`;
  - `requestStop()`/recall wins → return `"stopped"`;
  - timeout → send a **cancel** backstop, wait for that send to settle, mark the recorder idle, report a start error, and return `"not-started"`.

  Never send another toggle on timeout: if the app started slowly, a toggle could turn that late recording off or on ambiguously; cancel has only the safe direction. Keep Meeting's existing start semantics unless it opts into the acknowledgement option, so this change does not silently redesign the shipped command.

  In `main.ts`, retain the start promise and handle the outcome. On `not-started`, stop live enhancement, force-stop the follower, await its settled record, close the sidecar, call the existing runtime cleanup, and clear `#capture`/the status state. The notice must say: _"Assisted Notes did not start. In Shorthand, open Settings → Modes → Notetaking → Assisted notes, enable it, and try again."_ `show_main_window` may have raised the app, but it does not navigate there and is not a substitute for this notice. Emit the ordinary "capture started" notice for Assisted Notes only after acknowledgement.

- [ ] **Step 5: test the start contract** in `test/plugin-recorder.test.ts` with the existing fake clock/control:
  - required capability present → sequential `cancel`, assisted toggle, then `begin` resolves `started`;
  - required capability missing → `not-started`, no control signals;
  - no hello by attach deadline → `not-started`, no control signals;
  - toggle returns `{status: "error", message: <clap stderr>}` → the exact message is reported and the outcome is `not-started`;
  - toggle returns `sent` but no begin → acknowledgement timeout sends `cancel`, resolves `not-started`, and leaves `mayBeRecording === false`;
  - begin just before the deadline → `started` and no backstop cancel;
  - stop during the acknowledgement wait → the existing recall ordering wins and the outcome is `stopped`.

  Add a thin main/runtime test seam if needed so a `not-started` outcome is proven to clear the capture state; the recorder unit alone cannot prove Obsidian leaves its capturing UI.

- [ ] **Step 6: the manual toggle**, beside `toggle-shorthand-recording`:

```ts
this.addCommand({
  id: "toggle-shorthand-assisted-notes",
  name: "Toggle Shorthand assisted notes",
  callback: () => {
    this.fireControl("toggle-assisted-notes");
  },
});
```

This is not decoration. A manual recovery that names `"Toggle Shorthand recording"` would start a _Meeting_. The Assisted Notes recovery path has to select the same mode.

- [ ] **Step 7: make the not-running notice mode-aware.** Turn the `start` entry into a function of the signal, leaving the other four entries as they are:

```ts
const START_NOT_RUNNING = (signal: ControlSignal): string =>
  `Shorthand was not running, so this capture did not start a recording; Shorthand is starting now. Once it is up, start the recording with Shorthand's shortcut or "${signal === "toggle-assisted-notes" ? "Toggle Shorthand assisted notes" : "Toggle Shorthand recording"}" — the capture is already running and will pick it up.`;
```

`reportControl` takes the capture's signal for the `start` phase. This notice covers the ordinary app-not-running result. Capability failure and bounded-ack failure use the explicit notices above and end the runtime; they do not tell the user a capture is already running.

- [ ] **Step 8:** README — document the two new commands in the commands list and state that the capture command requires a Shorthand build whose follower hello advertises `toggle-assisted-notes`.

**Verify:** `npm run build && npm test`

---

### Task 15: Manual end-to-end

There is no CI in this repo; this gate is yours to run.

- [ ] Build with `OBSIDIAN_PLUGIN_DIR` set so the vault holds a build from committed code.
- [ ] In Obsidian, on a Markdown note, run **Start assisted notes capture on this note** with Shorthand running and Assisted Notes enabled. Confirm: the follower's `hello` advertises `toggle-assisted-notes`, `begin` acknowledges the start, the Minimal overlay stays visible for the entire capture **with a live waveform/readiness indicator**, the note fills, no text is pasted into Obsidian's editor by Shorthand, and the transcript sidecar (if enabled) is written.
- [ ] Repeat with **Assisted Notes disabled in the app**: confirm Shorthand's window comes to the front, no recording starts, the bounded acknowledgement expires, Obsidian names Settings → Modes → Notetaking → Assisted notes, and the plugin returns to non-capturing state automatically. Do not invoke Stop capture to make this pass.
- [ ] Repeat against an older installed Shorthand whose hello lacks the capability: confirm no `toggle-assisted-notes` child is spawned, the update notice is user-facing, and the plugin returns to non-capturing state.
- [ ] Simulate/observe a control error carrying clap stderr and confirm it is shown and the capture state is cleared rather than left waiting for `begin`.
- [ ] Repeat with **Shorthand not running**: confirm the not-running notice names _Toggle Shorthand assisted notes_, not _Toggle Shorthand recording_.

---

## Test plan — consolidated

### Existing tests that change

| Test                                                                          | File                                  | Change                                                                 |
| ----------------------------------------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------- |
| `mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting` | `shorthand/mode.rs`                   | Renamed and extended with the two new ids                              |
| `resolve_push_to_talk_reads_the_matching_mode_field`                          | `shorthand/dictation.rs`              | Extended with the two assisted-notes ids                               |
| `apply_mode_leaves_every_field_unchanged_for_meeting`                         | `shorthand/dictation.rs`              | Extended so `assisted_notes.*` values are set and asserted not to leak |
| `apply_mode_overrides_every_per_mode_field_for_dictation`                     | `shorthand/dictation.rs`              | Extended so `assisted_notes.*` values are asserted not to leak         |
| `empty_store_parses_with_defaults`                                            | `settings.rs`                         | New assertions for the assisted-notes defaults                         |
| `per_mode_defaults_differ_where_the_modes_differ`                             | `settings.rs`                         | Third column                                                           |
| `default_overlay_style_is_minimal`                                            | `settings.rs`                         | Third assertion                                                        |
| `default_bindings_have_distinct_shortcuts_on_this_platform`                   | `settings.rs`                         | Five ids → seven; doc comment updated                                  |
| `is_transcribe_binding` test                                                  | `transcription_coordinator.rs`        | Two new ids                                                            |
| `source_for_mode_maps_meeting_and_dictation`                                  | `managers/history.rs`                 | Renamed, third arm                                                     |
| `follow_stream_argument_shapes_parse_as_documented`                           | `cli.rs`                              | New conflict case                                                      |
| `signals` array                                                               | `shorthand-core/test/control.test.ts` | Fourth signal                                                          |

Tests that must pass **unedited**, and whose failure means a constraint was violated:
`frozen_v0_9_store_parses_strictly_and_migrates_only_paste_method`, `migration_preserves_explicitly_enabled_saving_recordings_and_transcripts`, `default_settings_disable_saving_recordings_and_transcripts`, `default_paste_method_is_not_none`, `apply_mode_leaves_settings_unchanged_for_dictation_when_disabled`, and every salvage test.

### New tests

| Test                                                                    | File                         | What it pins                                                                    |
| ----------------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------- |
| `mode_repr_round_trips_every_variant`                                   | `shorthand/mode.rs`          | The `AtomicU8` encoding, including the unknown-value fallback                   |
| `apply_mode_overrides_every_per_mode_field_for_assisted_notes`          | `shorthand/dictation.rs`     | Every per-mode field lands                                                      |
| `apply_mode_leaves_settings_unchanged_for_assisted_notes_when_disabled` | `shorthand/dictation.rs`     | The defence-in-depth guard                                                      |
| `assisted_notes_never_pastes`                                           | `shorthand/dictation.rs`     | The `PasteMethod::None` invariant survives Meeting's escape hatch               |
| `assisted_notes_never_captures_system_audio`                            | `shorthand/dictation.rs`     | The solo-capture invariant survives Meeting's system-audio setting              |
| `note_producing_modes_save_by_default`                                  | `shorthand/dictation.rs`     | The new defaults, and that Meeting's are unchanged                              |
| listener-policy matrix                                                  | `follow_stream/lifecycle.rs` | Meeting and enabled optional modes independently keep the shared listener alive |

### Commands, per repo

```sh
# shorthand-app
cargo fmt   --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml
bun run lint
bun run build
bun run test:unit
bun run check:translations
bun run check:fork-translations
bun run check:branding
bun run check:settings

# shorthand-core  (all four; bun test does not typecheck)
bun test
bun run typecheck
bun run build
bun run test:e2e

# obsidian-shorthand
npm run build      # tsc --noEmit, then the production esbuild bundle
npm test           # unit tests plus the bundle-load smoke
```

---

## Concerns

1. **`dictation.follow_stream_enabled` is currently inert, and Task 5 makes it live.** That is a fix, not a feature: the Advanced row promises to publish dictation to followers and today does nothing, because `actions.rs` gates on the mode rather than the setting. If that is unwanted, the alternative is to keep a mode-based gate and add a pure `Mode::publishes_to_followers()` predicate returning true for `Meeting | AssistedNotes` — same amount of code, but it leaves the untruthful row in place, which is the thing decision 1 exists to prevent.

2. **The registration-guard stack is now five call sites × five conditions, all copy-pasted, none unit-tested.** Extracting a pure `binding_is_registrable(&AppSettings, &str) -> bool` would collapse it into one testable function, and this change is the moment the duplication starts to hurt. It is deliberately _not_ done here, because three of the five conditions are upstream-owned lines and rewriting them turns a clean merge into a manual one. Worth doing as a separate, reviewable refactor.

3. **Four near-duplicate React components** (`AssistedNotes{EnableToggle,ToggleField,PostProcessPrompt,ClipboardHandling}`) mirror the dictation four. A generic `ModeToggleField<M>` keyed on the settings key would be strictly better and is all fork-only code, but it rewrites every existing `DictationToggleField` call site in `ModesSettings.tsx` while a working mode is being changed underneath it. Named as the follow-up rather than done here.

4. **`post_process_provider_id` and `post_process_model` have no UI in any mode.** They exist on `DictationSettings`, `apply_mode` honours them, and nothing can set them. `AssistedNotesSettings` inherits that state for parity. Either build the per-mode provider picker or delete the fields — but not in this change.

5. **The reviewed fork-catalogue and overlay plans are prerequisites.** Do not implement conditional paths for their pre-change layouts. Rebase after both land, put strings in `src/shorthand/locales/en.json`, and regenerate the reviewed branding golden hashes because the new keys deliberately change rendered output.

6. **`ctrl+alt+<letter>` is AltGr on several Windows keyboard layouts.** `ctrl+alt+n` may be unusable for users on those layouts, and the app cannot detect it. The existing `transcribe` default is already `ctrl+alt+space`, so this is not new, but `n` is a character AltGr actually produces on some layouts where `space` is not. If the shortcut proves flaky in testing, `ctrl+shift+alt+n` is not an escape (same modifier set) — a function key is.

7. **A `--toggle-assisted-notes` sent while the mode is disabled reports success to the plugin.** The forwarding process exits 0 before the running instance decides to refuse, so `ShorthandControl.send()` returns `{status: "sent"}` and `ShorthandRecorder` believes a recording started. It then waits `beginGraceMs`, sees no `begin`, and runs its normal backstop — the same path as a user who never pressed the hotkey, which is already handled. Showing the settings window is the only feedback the architecture allows. If that proves confusing in practice, the honest fix is a status query on the follow-stream socket, not a change to the exit-code contract.

8. **Nothing in this plan gives the plugin a way to _discover_ that Assisted Notes exists or is enabled.** It sends a flag and hopes. That is the intended trade — decisions 1 and 3 rule out the app answering questions about its own settings over the CLI — but it is the reason failure modes here are all soft and late rather than immediate.
