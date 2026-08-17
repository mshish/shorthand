# Shorthand: settings UI restructure

Status: proposed (revision 2, after design review)
Date: 2026-08-17

## Context

`mshish/shorthand` is a private fork of `cjpais/Handy`. Handy is a
push-to-talk dictation app: hold a shortcut, it transcribes, and it pastes the
text into whatever app has focus.

Shorthand keeps Handy's capture and transcription engine but changes the
product. It captures microphone audio, system audio, or both, and streams live
speaker-labelled transcripts over a local socket to follower processes. The
first follower is an Obsidian plugin that turns those transcripts into meeting
notes. Delivering text into the focused application is not part of the
product.

Push-to-talk is deliberately retained: a user may want to talk to their notes
directly rather than capture a meeting. That dictation still reaches the user
through the transcript stream, not through the clipboard.

### Repository layout

| Branch | Role |
| --- | --- |
| `main` | Byte-identical to `upstream/main`. Never committed to directly. |
| `feat/*` | Cut from `main`, upstream-clean. These become pull requests to Handy. |
| `shorthand` | Default branch. The product line. Fork-only work lives here. |

Remotes follow the standard fork convention: `origin` is the private fork,
`upstream` is `cjpais/Handy`.

This spec is fork-only work. It belongs on `shorthand` and must never appear
on a `feat/*` branch.

## The constraint that shapes everything

The fork must keep merging from upstream indefinitely, and some of its commits
must eventually become pull requests back to Handy. Every line this work
changes in a file upstream also changes is a merge conflict, forever.

The goal is therefore not "the smallest UI" but **the smallest diff against
upstream that produces the intended UI**. Those targets disagree in one place:
deleting things is the obvious route to a small UI and the worst route to a
small diff.

Three rules follow.

1. **Delete nothing.** No component file is removed. Hiding is a filter on
   which sections render.
2. **Move nothing.** Components stay at the paths upstream knows. Fork-only
   sections import them from where they already live. `git mv` on
   `PushToTalk.tsx` would conflict with every upstream change to it; importing
   it never conflicts.
3. **Concentrate the divergence.** Fork-only decisions live in new files
   upstream will never touch. Edits to upstream files are counted and
   justified individually below.

## Design

### Sections, not rows

Hiding operates at section granularity only.

Row-level hiding was considered and rejected. `SettingContainer`
(`src/components/ui/SettingContainer.tsx`) has no identity prop, and
`SettingsGroup` renders bare children, so rows have nothing stable to key on.
Filtering them would mean wrapping ~25 JSX lines in
`advanced/AdvancedSettings.tsx` in guards — precisely the per-line conflict
surface rule 3 exists to prevent.

Instead, upstream sections whose contents we are reorganising are **hidden
whole**, and fork-only replacement sections render the subset we want. What is
"hidden" is then a property of what the new components choose to render, not a
runtime filter. Giving rows real identity (a `settingKey` prop on
`SettingContainer`) is a reasonable upstream contribution but is out of scope
here.

### The visibility registry

One new fork-only module is the single source of truth:

`src/shorthand/visibility.ts`

It exports two section-id sets: the sections shown when `show_all_settings` is
false, and the fork-only sections hidden when it is true. `Sidebar.tsx`
consults it; nothing else encodes this knowledge.

No build config change is needed — the `@/` alias already resolves `src/*`.

### The escape hatch

A new fork-only boolean setting, `show_all_settings`, default `false`.

- `false` (the product): the simplified sidebar below.
- `true`: upstream's seven sections exactly, and the three fork-only sections
  hidden.

That symmetry matters. Without hiding the fork-only sections in `true` mode
the escape hatch would show nine sections and two competing versions of the
same settings.

The flag lives in the **About** section. About renders in both modes, so the
hatch can always be switched back off. Putting it in a fork-only section would
make it unreachable once enabled.

The flag exists because hiding is a judgement made before the product has
users. A hidden setting that turns out to matter must be recoverable at
runtime, not by a rebuild. It is a new flag rather than a reuse of Handy's
`experimental_enabled`, which already means something else upstream.

### Sidebar

Simplified mode (`show_all_settings` false) renders six sections:

| Section | Origin | Contents |
| --- | --- | --- |
| **Capture** | fork-only, new | transcribe + cancel shortcuts, push-to-talk, microphone, channel, system audio + device, mute while recording, VAD, overlay, follow-stream output |
| **Transcription** | fork-only, new | model catalog (`ModelsSettings`), `ModelSettingsCard` (language + translate, self-hiding by model capability), custom words, filler word removal |
| **App** | fork-only, new | autostart, start hidden, tray icon, audio feedback, volume, output device |
| **History** | upstream, unchanged | as upstream |
| **Debug** | upstream, unchanged | as upstream |
| **About** | upstream, + one row | as upstream, plus `show_all_settings` |

Hidden in simplified mode: upstream's `general`, `models`, `advanced`,
`postProcessing`.

Hidden in `show_all_settings` mode: `capture`, `transcription`, `app`.

**History and Debug stay visible.** Both were originally slated for hiding;
review showed both are load-bearing for this product specifically.

- Debug is the only home for `LogLevelSelector`, `LiveLogViewer` and
  `KeyboardDiagnostic` — the diagnostics needed first when a follower
  misbehaves on a live socket. It is already gated on `debug_mode` (default
  false), so it costs nothing in the shipped state, and leaving it registered
  preserves the Ctrl/Cmd+Shift+D toggle in `App.tsx`.
- History is the only UI for viewing or deleting stored recordings, and the
  retention logic in `managers/history.rs` runs regardless of whether the
  section renders. For a product that records meetings including other
  people's system audio, shipping with no route to review or purge captures
  would be a privacy failure, not a simplification.

Post-processing is hidden, and is already unreachable: its only toggle sits
inside the `experimental_enabled` group in `AdvancedSettings.tsx`, which
simplified mode does not render. Hiding the section reflects that rather than
causing it. Independently, `shorthand-core` already runs a Claude enhancement
pass over the transcript, so Handy's own LLM post-processing would be a second
pass over the same text.

Not carried into any fork-only section, deliberately: paste method, typing
tool, clipboard handling, auto-submit, append trailing space, paste delays,
acceleration selector, model unload timeout, lazy stream close, keyboard
implementation selector, history limit, recording retention period,
experimental toggle. All remain reachable via the escape hatch.

### Stopping the paste

This spec is not visibility-only. One default must change with it.

`src-tauri/src/actions.rs` calls `utils::paste(final_text, …)` unconditionally
when a transcription completes with non-empty text; the only guards are
cancellation and empty text. What that call does is decided by `paste_method`,
and `PasteMethod::None` is the only value that skips keystroke injection.
`impl Default for PasteMethod` currently yields `CtrlV` on Windows and macOS,
`Direct` on Linux.

So hiding the paste settings without changing the default would ship a product
that synthesizes Ctrl+V of a meeting transcript into whatever window has
focus, with the off-switch removed from the UI. That is strictly worse than
leaving the setting exposed.

**`impl Default for PasteMethod` returns `PasteMethod::None` on all
platforms.** `auto_submit` is already suppressed under `None`, so this
disables the whole text-delivery family in one change.

This affects new installs and any settings file missing the field. Existing
stored values are preserved by serde, so a user who already chose a paste
method keeps it.

Users who do want dictation-into-an-app can re-enable it through the escape
hatch. Push-to-talk itself is unaffected: it still produces transcripts, which
reach the user through the stream.

### Adding the setting

`src/bindings.ts` is generated by tauri-specta and carries a do-not-edit
header; it is emitted from `lib.rs` under `#[cfg(debug_assertions)]`. It is a
committed build artifact that must be **regenerated by a debug build**, not
hand-edited.

The complete sequence, traced from how `show_tray_icon` is threaded:

1. `src-tauri/src/settings.rs` — add the struct field.
2. `src-tauri/src/settings.rs` — add it to the `get_default_settings()`
   literal.
3. `src-tauri/src/shortcut/mod.rs` — add a `#[tauri::command] #[specta::specta]`
   setter.
4. `src-tauri/src/lib.rs` — register the command in `collect_commands![…]`.
5. Run a debug build to regenerate `src/bindings.ts`.
6. `src/stores/settingsStore.ts` — add an entry to `settingUpdaters`.

Steps 1–4 must land before step 6 is meaningful, and **step 6 cannot be
skipped**: without an updater, `updateSetting` logs "No handler for setting"
and the toggle appears to work, then reverts on reload.

The v0.9.0 settings fixture test in `settings.rs` survives the addition,
because the struct carries `#[serde(default)]`.

### Internationalisation

There are **24** locales.

New strings must be added to **all 24 locale files**, with the English text as
the value in every one. This matches what the fork already did for the
system-audio and follow-stream settings.

English-only keys are not an option despite `fallbackLng: "en"` being
configured: `.github/workflows/code-quality.yml` runs `check:translations`,
which exits non-zero on any key missing from a non-`en` locale, on every pull
request — including future `feat/*` PRs to upstream.

`eslint-plugin-i18next`'s `no-literal-string` rule is enforced in the same
workflow, so fork-only components must not contain bare string literals.

### Complete list of upstream files edited

This is the full permanent conflict surface. Everything else is new files.

| File | Edit |
| --- | --- |
| `src/components/Sidebar.tsx` | register three fork-only sections; consult the registry |
| `src/components/settings/about/AboutSettings.tsx` | one row for `show_all_settings` |
| `src-tauri/src/settings.rs` | one struct field, one default entry, `PasteMethod::None` default |
| `src-tauri/src/shortcut/mod.rs` | one command |
| `src-tauri/src/lib.rs` | one `collect_commands!` entry |
| `src/stores/settingsStore.ts` | one updater entry |
| `src/bindings.ts` | regenerated, not edited |
| 24 × `src/i18n/locales/*/translation.json` | new section-title keys |

## Verification

There is no React unit-test harness in this repo — no vitest, jest, or
testing-library. The only frontend tests are two Playwright smoke checks that
run Vite without a Tauri backend, so they cannot reach the settings UI at all.
Adding a harness would add devDependencies to upstream's `package.json` and
`bun.lock`, another permanent conflict surface, and is not justified by this
change.

Verification is therefore manual, against a debug build:

1. Fresh profile, simplified mode: six sections render — Capture,
   Transcription, App, History, Debug, About. `general`, `models`, `advanced`,
   `postProcessing` do not.
2. Push-to-talk, microphone selector and follow-stream output all render in
   Capture. These are the carve-outs the product depends on.
3. `ModelSettingsCard` still self-hides in Transcription when the active model
   lacks the capability.
4. Toggle `show_all_settings` on in About: upstream's seven sections render
   and Capture/Transcription/App disappear. Toggle it off: back to six.
5. Restart the app. The toggle's value persisted — this is what catches a
   missing `settingUpdaters` entry.
6. Fresh profile: complete a transcription with a text editor focused. **No
   text is pasted.** Then enable the escape hatch, set a paste method, and
   confirm pasting works again.
7. `bun run check:translations` and `bun run lint` both pass.
8. `cargo test` passes, including the v0.9.0 settings fixture.

## Out of scope

- **Other default values.** Only `paste_method` changes here, because the UI
  change is unsafe without it. A broader defaults review is separate work.
- **Triggering note-taking from Handy's shortcut.** The protocol already emits
  `begin` when transcription starts, so a follower can react to it with no
  change on this side. The work is in the Obsidian plugin. The only
  requirement this spec carries is the negative one: do not hide the
  follow-stream setting.
- **Removing the paste code path.** Defaulting it off is sufficient and
  reversible; deleting it is a large upstream-conflicting change with no
  additional user-visible benefit.
- **macOS Accessibility onboarding.** `App.tsx` gates first launch on the
  Accessibility permission, which exists solely to synthesize keystrokes. With
  `PasteMethod::None` as the default, the fork requests a permission it no
  longer needs by default. Noted, not addressed here.
- **Row-level hiding.** Requires a `settingKey` prop on `SettingContainer`,
  which is an upstream-facing change deserving its own `feat/*` PR.
