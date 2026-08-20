# Shorthand: dictation mode alongside meeting transcription

Status: approved
Date: 2026-08-20

## Context

Shorthand is a private fork of [cjpais/Handy](https://github.com/cjpais/Handy),
repurposed for meeting transcription and note-taking. It captures microphone
audio, system audio, or both, and streams live speaker-labelled transcripts over
a local socket to follower processes. The first follower is an Obsidian plugin.

Handy's own product is different: hold a shortcut, speak, and the transcript is
pasted into whatever window has focus. The fork turned that off
(`PasteMethod::None` on every platform) and hid the settings that control it —
see [2026-08-17-shorthand-settings-ui-design.md](2026-08-17-shorthand-settings-ui-design.md).

Some users want both: Shorthand for meetings *and* plain dictation, without
installing and running Handy alongside it. Everything dictation needs is already
in the binary; only the settings and the wiring are absent.

This spec adds an opt-in **dictation mode** that runs alongside meeting mode,
with its own shortcuts and its own settings, without changing meeting mode's
defaults or its settings screens.

There is no coupling to a Handy installation. No settings file is shared, no
process is detected, nothing is imported. The only concession to Handy's
existence is that meeting mode's default shortcut moves off Handy's, so the two
apps can run side by side during a transition.

## The constraint that shapes everything

Unchanged from the prior spec, and it governs every decision below: the fork
merges from upstream indefinitely, and some commits may become pull requests
back to Handy. Every line this work changes in a file upstream also changes is a
merge conflict, forever.

So: prefer new files, keep unavoidable edits to upstream files small and local,
and give the feature a boundary — `follow_stream/` is the model.

## Design

### The active-mode cell

Four per-mode concerns need to know which mode a capture belongs to, and they
are resolved in four different places:

| Concern | Resolved in |
| --- | --- |
| Paste method, clipboard, auto-submit, trailing space | `clipboard::paste()` |
| Overlay style | `overlay::show_overlay_state()` and `actions.rs` |
| Save recordings / save transcripts | `actions.rs`, `HistoryManager::save_entry()` |
| Post-processing prompt | `actions.rs::process_transcription_output()` |

Threading a `binding_id` parameter into all four means four upstream signature
changes plus their call sites. Instead, one fork-only cell records the mode of
the capture currently in flight:

```rust
// src-tauri/src/shorthand/mode.rs
pub enum Mode { Meeting, Dictation }

pub fn set_active(app: &AppHandle, binding_id: &str);
pub fn active(app: &AppHandle) -> Mode;
```

`TranscribeAction::start` calls `set_active` once, deriving the mode from
`binding_id`. Every resolver reads it.

This is sound because only one capture runs at a time —
`AudioRecordingManager` tracks a single `is_recording` flag, and
`TranscriptionCoordinator`'s `Stage` state machine serialises transcribe
bindings. The cell is **set on every start and never cleared**: "the mode of the
most recently started capture" is always the correct answer for work belonging
to that capture, including async work that outlives the recording itself. A
cleared cell would introduce a race that an uncleared one does not have.

It defaults to `Meeting`, so any code path reached before the first capture
behaves exactly as it does today.

The payoff is that **no upstream function signature changes**. `paste()` keeps
its exact signature; only its first line changes, from `get_settings(...)` to
the fork's resolver. The same holds for `show_overlay_state` and `save_entry`.

### Settings data model

One new field on `AppSettings`, appended at the end of the struct so future
upstream additions never have to reflow around it:

```rust
#[serde(default)]
pub dictation: crate::shorthand::dictation::DictationSettings,
```

`DictationSettings` lives in a new fork-only module:

```rust
// src-tauri/src/shorthand/dictation.rs
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
```

**Nested, not flat.** `settingsStore.ts` needs one `settingUpdaters` entry per
top-level key, and each entry needs a Rust command and a `collect_commands!`
line — all inside files upstream owns. Thirteen flat `dictation_*` fields would
be thirteen permanent lines of conflict surface. Nested is one field, one
command, one updater entry.

`useSettings`'s `getSetting`/`updateSetting` are `keyof Settings` only and
cannot address a nested path. They do not need to: a component reads the current
sub-struct, spreads it, overrides one key, and writes the whole object back.
The post-processing settings already work this way.

`AppSettings` carries a container-level `#[serde(default)]`, so the frozen
v0.9.0 fixture test and `empty_store_parses_with_defaults` both pass with **no
edits** — `dictation` is simply absent and falls back to its default.

#### The `PasteMethod` default trap

`impl Default for PasteMethod` returns `None` on every platform, because the
prior spec changed it to stop meeting transcripts being typed into the focused
window. `DictationSettings` must **not** inherit that. Its default must be
Handy's original per-platform choice — `CtrlV` on Windows and macOS, `Direct`
on Linux — or enabling dictation records, transcribes, and then delivers
nothing. A feature that silently does nothing when switched on is the worst
possible first run.

This must be an explicit `Default` impl with a comment saying why, so a later
upstream merge does not "tidy" it back to `#[default]`.

#### No per-mode external script

`external_script_path` stays global and is **not** in `DictationSettings`, so
dictation does not offer the `ExternalScript` paste method at all. Offering it
while sharing the path would reintroduce exactly the cross-mode bleed this
design exists to prevent. Adding a per-mode path is a later change if anyone
asks for it.

### Per-mode and shared settings

Per-mode, in `DictationSettings`:

| Setting | Dictation default | Why not shared |
| --- | --- | --- |
| `paste_method` | `CtrlV` / `Direct` | The entire point. Meeting mode stays `None`. |
| `push_to_talk` | `true` | Meetings run an hour and are toggled; dictation is seconds and is held. |
| `clipboard_handling`, `auto_submit`, `auto_submit_key`, `append_trailing_space`, `typing_tool` | Handy's defaults | Subordinate to `paste_method`; they follow it. |
| `overlay_style` | `Minimal` | Dictation wants the compact pill, not a live-transcript panel over the text field. |
| `save_recordings`, `save_transcripts` | `false` | Consent, not preference. Meeting captures contain other people's voices; dictation contains only the user's. One checkbox for both conflates two different decisions. |
| `post_process_enabled`, `post_process_selected_prompt_id` | `false`, `None` | Dictation wants a cleanup prompt; a meeting wants a summary prompt, if anything. |

Shared with meeting mode, deliberately:

- **Model** (`selected_model`), and the streaming-only catalog filter in
  `modelVisibility.ts` stays in force. The app loads one model at a time. This
  is a known trade: a dictation user has less model choice than in plain Handy,
  and the streaming constraint that motivates the filter — the Obsidian
  follower needs `partial` events — does not technically apply to dictation,
  whose paste path only uses `final_text`. Accepted deliberately to avoid
  model-swap churn on every mode alternation. Revisit if it bites.
- **Post-processing providers, API keys and model choices.** These are
  credentials. Duplicating the provider and key UI would be worse than sharing
  it.
- Microphone, channel, VAD, mute-while-recording, audio feedback, sound theme,
  overlay *position*, custom words, filler-word removal, language, translation,
  acceleration, model unload timeout.

Not offered in dictation settings at all, because they are meeting concepts:
system audio capture and device, follow-stream output.

### Bindings and dispatch

Two new binding ids, mirroring upstream's existing pair:

- `dictate` → `TranscribeAction { post_process: false }`
- `dictate_with_post_process` → `TranscribeAction { post_process: true }`

`TranscribeAction` keeps its exact shape. Mode resolves from `binding_id`, which
is already threaded to every point that needs it.

`is_transcribe_binding` in `transcription_coordinator.rs` **must** learn both
new ids. This is not optional. `handle_shortcut_event` routes anything failing
that check straight to `ACTION_MAP`'s bare press/release path, bypassing the
coordinator's state machine — which would let a dictation press race a
meeting-mode `stop_recording` against the same `AudioRecordingManager`.

#### The five skip-guard sites

Registration is skipped for a binding whose mode is off. Upstream already does
this for `transcribe_with_post_process`, in **five** places:

- `shortcut/mod.rs` — `resume_all_shortcuts` and the implementation-switch path
- `shortcut/tauri_impl.rs` — `init_shortcuts`
- `shortcut/handy_keys.rs` — `init_shortcuts`
- `secure_input.rs`

Each grows to cover the dictation pair: `dictate` is skipped unless
`dictation.enabled`; `dictate_with_post_process` is skipped unless
`dictation.enabled && dictation.post_process_enabled`.

This is what makes "off by default" true rather than merely unreachable from the
UI. Each site is a one-line addition matching the existing pattern.

#### Registration failure is silent today

`register_shortcut` checks `is_registered` and returns `Err` on collision,
but `init_shortcuts` only `error!`-logs the result. A dictation shortcut that
collides with something the user already bound therefore fails to register with
no UI trace, and pressing it does nothing.

Two guards.

First, verify at review time that the five defaults do not collide with each
other on any platform.

Second, surface the failure when the user enables dictation — the case that
matters, because that is when they choose a combo another app may already own.
`change_dictation_settings` registers the bindings and **returns the error**
rather than discarding it. The store's `updateSetting` catches a rejected
update and reverts its optimistic write, so the toggle visibly springs back;
the Dictation section compares the requested value against the persisted one
and explains why. Startup registration stays log-only, as it is for every other
binding — fixing that generally is upstream-facing work.

### Shortcuts

Meeting mode moves off Handy's defaults so both apps can run side by side.
Dictation takes Handy's exact combos, so muscle memory transfers.

| Binding | Windows / Linux | macOS |
| --- | --- | --- |
| `transcribe` (meeting) | `ctrl+alt+space` | `ctrl+shift+space` |
| `transcribe_with_post_process` | `ctrl+alt+shift+space` | `ctrl+shift+option+space` |
| `dictate` | `ctrl+space` | `option+space` |
| `dictate_with_post_process` | `ctrl+shift+space` | `option+shift+space` |
| `cancel` | `escape` | `escape` |

macOS `ctrl+space` and `ctrl+option+space` are both bound by the OS to the
input-source switcher, and `cmd+space` is Spotlight. `ctrl+shift+space` avoids
all three.

**Cancel stays a single shared binding.** Only one capture runs at a time, so
there is no ambiguity for a second cancel key to resolve.

### Existing installs keep their shortcuts

`get_settings` merges default bindings into a loaded store only for **vacant**
keys. Every existing store already has a populated `transcribe` entry, so
changing the default string in `get_default_settings()` affects fresh installs
only — neither `default_binding` nor `current_binding` is touched for an
existing user.

**No migration, and no schema-version bump.** The prior spec's `PasteMethod`
migration actively reset a stored value because leaving it alone shipped a
privacy regression. A shortcut has no equivalent argument: resetting it breaks
muscle memory and buys no safety. The frozen v0.9.0 fixture keeps its `f13`
binding and needs no edit.

The two new bindings *do* reach existing stores through the vacant-key merge,
but stay unregistered until dictation is enabled.

### Follow-stream stays silent for dictation

A dictation run must not reach the Obsidian follower.

`hub.begin()` is the only call that populates an active session. Every terminal
call — `finish`, `no_speech`, `cancel`, `error` — routes through `finish_with`,
which starts `let Some(active) = state.active.take() else { return };`.
`partial()` does the same check.

So **skipping `begin` alone makes every later `hub.*` call for that run a silent
no-op.** One conditional at one call site in `TranscribeAction::start`, not a
gate on each of the seven hub call sites.

This behaviour is load-bearing and currently implicit. Pin it with a test in
`follow_stream/hub.rs` asserting that `finish`, `no_speech` and `partial` called
without a preceding `begin` broadcast nothing, so a later hub refactor cannot
break the gate silently.

### Overlay

Resolved through the mode cell, not through `actions.rs` alone.

`actions.rs` has two `overlay_style` reads where the binding is in scope. But
`overlay::show_overlay_state` reads `overlay_style` from global settings on
*every* state transition, purely to early-return on `None`. Without the cell,
setting dictation's overlay to `None` while meeting's is `Live` would still
flash a processing overlay mid-dictation. All three reads consult the resolver.

Overlay *position* stays shared: top-versus-bottom is a screen-layout
preference, not a mode one.

### History

`save_recordings` and `save_transcripts` resolve through the cell in
`actions.rs`.

With both modes able to save, History becomes an undifferentiated list — and
History exists precisely so a user can review and purge meeting captures
containing other people's voices. Mixing dictation into it blunts that.

The schema is migration-based, so this is additive:

```rust
M::up("ALTER TABLE transcription_history ADD COLUMN source TEXT NOT NULL DEFAULT 'meeting';"),
```

`HistoryEntry` gains a `source` field, and `save_entry` reads the mode cell
rather than growing a parameter. The History list renders a small source tag per
row.

This is the last increment and is severable: cutting it leaves per-mode save
toggles working, only the list is ambiguous.

### Post-processing

Dictation gets the full Handy shape: a second binding, its own enable flag, and
its own selected prompt. Providers, API keys and models stay shared.

`process_transcription_output` reads `post_process_selected_prompt_id` from
global settings; it resolves through the cell instead.

The provider/key/model configuration lives in upstream's `postprocessing`
sidebar section, which simplified mode hides and whose `enabled` predicate reads
`post_process_enabled` alone. It must also become visible when *dictation's*
post-processing is on — a one-line change to that predicate in `Sidebar.tsx`.

The Dictation section itself carries only the enable toggle and the prompt
picker. Duplicating the provider and API-key UI would be worse than sending the
user to the section that already owns it.

### macOS Accessibility

`PasteMethod::None` meant the fork requested a permission it no longer used.
Dictation needs it back.

- Onboarding's Accessibility step stays as it is.
- The Dictation section shows a live permission status row with a Grant button,
  on macOS, when dictation is enabled — reusing `AccessibilityPermissions.tsx`.

Without the second half, a user who revoked the permission or moved to a new Mac
gets a toggle that reads "on" and silently pastes nothing.

### Settings UI

A fourth fork-only sidebar section, **Dictation**, registered in
`src/shorthand/sections.ts` and added to `FORK_ONLY_SECTIONS` in
`src/shorthand/visibility.ts`.

The prior spec's groundwork means registration is free: `SHORTHAND_SECTIONS` is
already spread into `SECTIONS_CONFIG`, and `useVisibleSection.ts` resolves the
initial and fallback section generically. **Registering the section needs no
edit to `Sidebar.tsx`**, and `App.tsx` is untouched entirely. (`Sidebar.tsx`
does get one unrelated edit — the `postprocessing.enabled` predicate, below.)

Two alternatives were rejected:

- **A mode switcher** re-skinning Capture/Transcription/App puts dictation's
  render path inside the files that render meeting settings — the coupling this
  work exists to avoid — and composes badly with `show_all_settings`, which is
  already a second mode.
- **Folding dictation into Capture** makes every meeting-only user scroll past
  dictation chrome on their primary screen forever.

New file `src/shorthand/DictationSettings.tsx`, rows in order:

1. **Enable Dictation** — new fork-only toggle. Everything below is rendered but
   disabled when off, not hidden: an empty section reads as broken, and a
   disabled one previews what enabling buys.
2. *Shortcut* — `ShortcutInput shortcutId="dictate"`, `PushToTalk` (per-mode),
   `ShortcutInput shortcutId="dictate_with_post_process"`, macOS Accessibility
   status row. Both shortcuts sit together so the two keys can be read and
   compared at a glance.
3. *Output* — `PasteMethod`, `TypingTool` (Linux), `ClipboardHandling`,
   `AutoSubmit`, `AppendTrailingSpace`, `ShowOverlay`.
4. *AI cleanup* — enable toggle, prompt picker, and a link to the
   Post-processing section for provider and key setup.
5. *Privacy* — `SaveRecordings`, `SaveTranscripts`.
6. A footer line stating that microphone, model and language come from Capture
   and Transcription.

Rows in group 3 are exactly the inventory the prior spec listed as "not carried
into any fork-only section, deliberately". Dictation is the home they were
always going to need. Every one is an existing component, reused, pointed at
`settings.dictation.*`.

Settings shared with meeting mode say so in their tooltip copy. No new badge
component.

The shortcut row labels become **Capture shortcut** and **Dictation shortcut**.
This is copy only; the stored binding id `transcribe` is unchanged.

### Internationalisation

24 locale files. Every new string needs a key in all of them with the English
text as the value, because `check:translations` fails CI on any gap and
`eslint-plugin-i18next`'s `no-literal-string` rule forbids bare literals in the
new component.

Reuse existing keys wherever the label is identical — the paste-method,
clipboard-handling, auto-submit and overlay rows all have translated labels
already. Only genuinely new strings ("Dictation", "Enable dictation mode",
"Dictation shortcut", the shared-setting tooltips, the footer note) need new
keys.

## Complete list of upstream files edited

This is the full permanent conflict surface. Everything else is a new file under
`src-tauri/src/shorthand/` or `src/shorthand/`.

| File | Edit |
| --- | --- |
| `src-tauri/src/settings.rs` | one struct field; two `bindings.insert` entries; new shortcut default strings |
| `src-tauri/src/lib.rs` | `pub mod shorthand;`; one `collect_commands!` entry |
| `src-tauri/src/transcription_coordinator.rs` | `is_transcribe_binding` learns two ids |
| `src-tauri/src/actions.rs` | two `ACTION_MAP` inserts; `set_active` call; `hub.begin()` guard; three resolver swaps (overlay ×2, save toggles) |
| `src-tauri/src/clipboard.rs` | one line: `get_settings` → resolver |
| `src-tauri/src/overlay.rs` | one line: `get_settings` → resolver |
| `src-tauri/src/managers/history.rs` | one migration; one struct field; `save_entry` reads the cell |
| `src-tauri/src/shortcut/mod.rs` | one command; two skip-guard sites |
| `src-tauri/src/shortcut/tauri_impl.rs` | one skip-guard site |
| `src-tauri/src/shortcut/handy_keys.rs` | one skip-guard site |
| `src-tauri/src/secure_input.rs` | one skip-guard site |
| `src-tauri/src/shortcut/handler.rs` | `push_to_talk` resolves per binding |
| `src/components/Sidebar.tsx` | `postprocessing.enabled` also honours dictation |
| `src/stores/settingsStore.ts` | one updater entry |
| `src/components/settings/history/*` | render the source tag |
| `src/bindings.ts` | regenerated by a debug build, not edited |
| 24 × `src/i18n/locales/*/translation.json` | new keys |

## Verification

There is still no React test harness, and adding one would put devDependencies
in upstream's `package.json` — another permanent conflict surface. Backend work
is unit-tested; UI work is manual.

Automated (`cargo test`):

1. `DictationSettings::default().paste_method` is **not** `PasteMethod::None`.
2. The v0.9.0 fixture and `empty_store_parses_with_defaults` pass unmodified.
3. The delivery resolver returns meeting-mode fields when the cell is `Meeting`
   and never reads `settings.dictation.*`; and the mirror assertion for
   `Dictation`. This is the cross-talk guard.
4. `follow_stream/hub.rs`: `finish`, `no_speech` and `partial` without a
   preceding `begin` broadcast nothing.
5. The five default bindings in `get_default_settings()` have distinct
   `current_binding` values. `cfg` means this only covers the host platform, so
   the other two are a review-time reading of the `cfg` branches, not a test.

Manual, against a debug build:

6. Fresh profile: the sidebar shows Capture, Transcription, App, Dictation,
   History, Debug, About. Dictation's rows are disabled.
7. Dictation off: pressing `ctrl+space` / `option+space` does nothing, and the
   log shows no registration for `dictate`.
8. Enable dictation, dictate into a text editor: text is pasted. Meeting mode's
   shortcut still pastes nothing.
9. Set the two modes to *different* paste methods and different overlay styles.
   Confirm each mode uses its own. This is the scenario a shared-state bug
   produces.
10. Enable dictation post-processing: the Post-processing section appears, and
    the dictation LLM shortcut registers.
11. Turn dictation's save-transcripts on and meeting's off. Confirm only
    dictation runs reach History, tagged as such.
12. macOS: revoke Accessibility. The Dictation section reports it and offers
    Grant.
13. Restart. Every dictation setting persisted — this is what catches a missing
    `settingUpdaters` entry.
14. `bun run check:translations`, `bun run lint`, `cargo test`, `cargo clippy`.

## Increments

Each ends in a working app. 1–7 land no user-visible change and review on
`cargo test` alone, so a manual-testing bottleneck never blocks backend merges.

1. Mode cell + `DictationSettings` data model
2. Bindings and dispatch, gated off — two bindings, five skip-guard sites
3. Delivery resolver + follow-stream gate
4. Overlay resolver
5. Save-toggle resolver
6. Post-process resolver and prompt
7. Command and store wiring
8. UI: enable toggle, both shortcuts, Accessibility row
9. UI: output, overlay, save and post-process rows
10. History `source` column and tag — severable
11. i18n and lint pass

## Out of scope

- **Per-mode model choice.** Decided against; see the shared-settings section.
- **Concurrent capture.** One capture at a time remains the rule. The mode cell
  depends on it, and the coordinator already enforces it.
- **Per-mode microphone, VAD or audio feedback.** One physical setup.
- **Dictation onboarding.** Off by default, discovered in the sidebar.
- **Multiple dictation profiles or per-app paste overrides.** Handy does not
  have these either.
- **Surfacing shortcut-registration failures generally.** Only the dictation
  bindings report failure here. Doing it for every binding is an upstream-facing
  change deserving its own `feat/*` PR.
- **Removing the streaming-only model filter for dictation.** Recorded as a
  known trade, not addressed.
