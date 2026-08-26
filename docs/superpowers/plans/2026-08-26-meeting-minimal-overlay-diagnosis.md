# Plan: keep the recording overlay visible across cancel-then-start

> **For agentic workers:** implement this plan task by task. Do not fold it into the Assisted Notes change; this bug exists with the two shipped modes and should land first.

**Goal:** Keep a newly shown recording overlay visible when an older fade-out finishes, and make waveform/readiness events follow the active capture's resolved overlay style rather than Meeting's stored style.

**Architecture:** Two independent backend defects produce missing or inert overlays. The delayed native hide has no generation and can hide a window that a later capture has re-shown. Separately, `OVERLAY_ENABLED` is cached from the top-level Meeting setting even when Dictation is active. Fix the race with a visibility epoch and execute the delayed native `hide()` on Tauri's main thread. Fix event emission by setting the cache from the resolved style of the capture being shown. No frontend change is needed.

**Tech Stack:** Rust, Tauri 2, atomics. No new dependencies.

## Review decisions

1. **The cancel-then-start race is confirmed from the code.** Implementation does not wait on another user experiment. The bare-toggle experiment remains a useful manual regression check, not a gate for deciding whether to fix the race.
2. **An epoch check on the sleeping thread is insufficient.** A show can land after that check and before `window.hide()`. The delayed closure must hop to the main thread, check the epoch there, and perform the native hide in that same closure. Show requests bump the epoch before they enqueue their main-thread work. That ordering leaves the most recent request in control.
3. **Fix `OVERLAY_ENABLED` in the same change.** Assisted Notes will add a third resolved overlay style, so leaving this cache Meeting-owned would immediately reproduce the existing Dictation defect in another mode.
4. **Keep the scope backend-only.** `RecordingOverlay.tsx` receives the correct state and keeps animating; the native window is what disappears. CSS changes would mask neither backend defect.

## Root causes

### Stale delayed hide

`hide_recording_overlay` emits `hide-overlay`, sleeps 300 ms on a detached thread, then calls `window.hide()` unconditionally. The app owns one reused `recording_overlay` window. The Obsidian capture path sends `--cancel` and then `--toggle-transcription` about 50–70 ms apart, so the cancel's delayed hide fires after the new capture has called `window.show()`.

The expected warm-path order is:

| Time | Event |
| --- | --- |
| 0 ms | Plugin sends `--cancel` |
| ~50 ms | App schedules native hide for ~350 ms |
| ~60 ms | Plugin sends `--toggle-transcription` |
| ~120 ms | New capture shows the overlay |
| ~350 ms | Old delayed hide hides the reused window |

The shortcut path has no preceding cancel, which explains why Dictation appeared healthy in the report. The race itself is mode- and style-independent.

### Meeting-owned event cache

`OVERLAY_ENABLED` gates `emit_levels` and `emit_recording_ready`. It is seeded in `lib.rs` from `AppSettings::overlay_style` and updated only by `change_overlay_style_setting`, which also changes the Meeting field. A Dictation capture resolves `DictationSettings::overlay_style`, but the cache never sees it.

This produces both wrong directions:

- Meetings `None`, Dictation `Minimal`: the Dictation window appears, but its waveform is flat and its arming dot never becomes ready.
- Meetings `Minimal`, Dictation `None`: the hidden overlay webview still receives mic-level work during Dictation.

## Task 1: cancel stale delayed hides

**File:** `src-tauri/src/overlay.rs`

- [ ] Add a process-wide `AtomicU64` beside the other overlay atomics:

  ```rust
  /// Bumped by every native visibility request. A delayed hide may act only
  /// while its epoch is still current, so an overlay shown during the fade
  /// window cannot be hidden by the lifecycle that preceded it.
  static OVERLAY_VISIBILITY_EPOCH: AtomicU64 = AtomicU64::new(0);
  ```

  `Relaxed` ordering is enough. The value is an identity token; it does not publish any other memory.

- [ ] In `show_overlay_state`, resolve the style first. If it is `None`, return without changing the visibility epoch so an already scheduled hide can complete. Otherwise bump `OVERLAY_VISIBILITY_EPOCH` **before** calling `run_on_main_thread`.

- [ ] In `hide_recording_overlay`, bump the epoch and capture the returned value as this hide's token. Keep emitting `hide-overlay` immediately so the 300 ms CSS fade remains unchanged.

- [ ] After sleeping, use `AppHandle::run_on_main_thread`. Inside that main-thread closure, compare the current epoch with the captured token and call `hide()` only when they match.

  The epoch check and the native hide belong in the same main-thread closure. Do not check on the worker and call `hide()` later; that leaves the original time-of-check/time-of-use race open. Do not call Tauri window mutation APIs directly from the sleeper thread.

- [ ] Extract only the comparison as a pure helper:

  ```rust
  fn visibility_request_is_current(scheduled: u64, current: u64) -> bool {
      scheduled == current
  }
  ```

  Add unit cases for equal tokens, an older hide, and a different token. The helper does not prove thread ordering; it pins the rule the main-thread closure applies.

**Verify:**

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml overlay
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

## Task 2: make event emission follow the active capture

**Files:**

- `src-tauri/src/overlay.rs`
- `src-tauri/src/actions.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/shortcut/mod.rs`

- [ ] Rename `update_overlay_enabled_cache` to `set_capture_overlay_events_enabled` and rewrite its comment: the atomic describes the active capture, not the top-level setting.

- [ ] In `show_overlay_state`, set the cache from the resolved style **before** the `OverlayStyle::None` return:

  ```rust
  let settings = crate::shorthand::dictation::resolve_settings(app_handle);
  set_capture_overlay_events_enabled(settings.overlay_style != OverlayStyle::None);
  if settings.overlay_style == OverlayStyle::None {
      return;
  }
  ```

  This covers transitions to recording, streaming, transcribing, and processing states.

- [ ] In `TranscribeAction::start`, keep the existing resolved `overlay_style` match local, but call `set_capture_overlay_events_enabled(false)` in the `OverlayStyle::None` arm. That arm deliberately skips `show_overlay_state`, so it must update the capture cache itself.

- [ ] Delete the startup cache seed in `lib.rs`. No capture is active at startup, so the correct default is the atomic's existing `false`.

- [ ] Delete the cache write from `change_overlay_style_setting` in `shortcut/mod.rs`. A Meeting preference edited while Dictation is active must not change Dictation's event flow. Leave the settings write and overlay-position update untouched.

- [ ] Update the cache comments above `emit_recording_ready` and `emit_levels` to name the active capture and the memory-allocation reason for avoiding store reads on the audio callback.

- [ ] Add a pure `overlay_events_enabled(style: OverlayStyle) -> bool` helper and unit-test `None`, `Minimal`, and `Live`. Use it at the cache write sites so the tests cover the actual predicate.

**Verify:**

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml overlay
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

## Task 3: regression pass

Run the full backend gate:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Then run `bun run tauri dev` and check:

1. Set Meetings to `Minimal`. From Obsidian, run the existing capture-on-note command. The overlay stays visible for the whole recording; it does not flash and disappear after about 300 ms.
2. Run Obsidian's bare **Toggle Shorthand recording** command. Its behaviour matches the capture command.
3. Set Meetings to `None` and Dictation to `Minimal`. Start Dictation by shortcut. The waveform moves and the arming dot reaches ready.
4. Set Meetings to `Minimal` and Dictation to `None`. Start Dictation. No overlay appears. The `overlay_events_enabled(None)` unit case is the regression guard that the audio callback exits before emitting.
5. Start a recording, stop it, and restart inside the 300 ms fade window. The new overlay remains visible.
6. Stop without restarting. The fade still completes and the native window hides.

Assisted Notes is not required for this plan, but its implementation plan depends on these checks passing before the third mode lands.

## Files read during diagnosis

| Path | Relevant behavior |
| --- | --- |
| `src-tauri/src/overlay.rs` | Reused window, delayed hide, main-thread show path, event cache |
| `src-tauri/src/utils.rs` | Idle cancel still requests an overlay hide |
| `src-tauri/src/lib.rs` | Single-instance cancel/toggle dispatch and startup cache seed |
| `src-tauri/src/actions.rs` | Per-mode style resolution and capture lifecycle |
| `src-tauri/src/shortcut/mod.rs` | Meeting-only cache update |
| `D:/tools/obsidian-shorthand/src/recorder.ts` | Sequential cancel-then-toggle start path |
| `D:/tools/obsidian-shorthand/main.ts` | Bare-toggle comparison path |

## Deliberately not in this plan

- Skipping `hide_recording_overlay` for an idle cancel. That would avoid the Obsidian reproduction but leave fast stop/start and tray-cancel races intact.
- Frontend animation or CSS changes. The React state is already correct when the native window disappears.
- Assisted Notes itself. This plan removes a prerequisite defect; the third mode remains a separate change.
