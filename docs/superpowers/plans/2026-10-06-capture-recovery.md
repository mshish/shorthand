# Capture Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dictation recovers from a broken default audio device instead of failing until restart. A quick tap is no longer misread as a hold when the start path is slow. Capture failures and native crashes become visible, both in the overlay and in Sentry.

**Architecture:** Four independent fixes on `fix/capture-recovery` (worktree `.worktrees/capture-recovery`, based on `origin/main` = shipped 0.6.0), plus an upstream PR draft:
1. The coordinator measures hold-vs-tap from when the key event arrived, not from when its thread dequeued it.
2. On Windows, when opening the default mic or system-audio device fails, that one open is retried with the same endpoint opened by its id. Every open still tries the default handle first, so normal behaviour (following default-device changes mid-recording) is unchanged.
3. A crash marker written around the native model load turns a native abort into a `native_crash` Sentry event on the next launch. Mic open failures are reported as `mic_open`.
4. The overlay gets an error state that says what went wrong and what to do, instead of flashing "transcribing…" and vanishing.
5. Task 1, cut separately from `upstream/main`, is drafted as a PR to cjpais/Handy for the user to submit.

**Tech Stack:** Rust (Tauri 2, cpal 0.18.2, sentry 0.49), React/TypeScript overlay, `bun test` for pure TS logic.

**Spec:** No separate spec. The investigation behind this plan is summarised under "Evidence" below. That is the spec; read it first.

## Evidence (2026-10-06 investigation)

Log: `%LOCALAPPDATA%\com.mshish.shorthand\logs\handy.log`. Log times are UTC.

- **Mic (2026-10-05, 22:07 onwards):** a meeting session ran 21:14–22:03 on the default mic and default output.
  - cpal reported a device change at the start (`Microphone backend reported a stream error`) and again at the end (`System audio capture failed`).
  - From then on, every mic open failed with `Cannot change thread mode after it is set. (os error -2147417850)` (`RPC_E_CHANGED_MODE`) until the app was restarted.
  - Activation of cpal's *default* handle goes through `ActivateAudioInterfaceAsync` (`cpal-0.18.2/src/host/wasapi/device.rs:351-406`), which does its work on OS-owned threads.
  - cpal's default-device monitor runs `CoInitializeEx(COINIT_APARTMENTTHREADED)` on MMDevAPI's notification threads (`stream.rs:153,172` → `device.rs:1168` → `com.rs:16`).
  - Hypothesis, unconfirmed: that leaves a pooled OS thread STA, so later default activations fail.
  - A *specific* device (`DeviceHandle::Specific`) uses `IMMDevice::Activate` on our own worker thread and registers no monitor (`device.rs:553,589`). It avoids both.
  - The user chose not to fork cpal, and not to stay pinned after a failure. Keep default streams, and recover per open.
- **0-sample stop (2026-10-06 10:17:37):**
  - The press started recording, and the start effect blocked the coordinator thread for 416 ms (cold VAD load plus mic open).
  - The release was queued meanwhile. The coordinator stamped it with `Instant::now()` only when it dequeued it (`transcription_coordinator.rs:1003`), so a tap measured ≥ 300 ms (`default_hold_threshold_ms`).
  - It was therefore treated as a hold, recording stopped with 0 samples, and the overlay flashed "transcribing" and hid (`actions.rs:984-998`).
  - Toggle taps at 20:44 the night before worked because that start took 51 ms.
- **GPU (10:17:37–38):**
  - Vulkan `createDevice` threw `ErrorOutOfDeviceMemory` 25 min after the laptop left Modern Standby.
  - ggml caches the half-built device (`ggml_vk_get_device` publishes `devices[idx]` before init; still true in transcribe-cpp 0.3.0 and ggml master).
  - The next press reused it and `vulkan-1.dll` fail-fast aborted the process (0xC0000409; the same signature hit on 2026-09-18).
  - Sentry got only `model_load` (SHORTHAND-APP-4). It got nothing for the crash or the mic failures.
  - **Out of scope here:** the user decided to wait for Handy PR cjpais/Handy#2208 (transcribe.cpp in a worker process) and handle GPU recovery when merging it. This plan only makes such crashes visible (Task 3).

## Global Constraints

- **Cargo from Claude:** every cargo command sets `CARGO_TARGET_DIR=D:/tools/shorthand-repos/shorthand-app/src-tauri/target` and `LOCALAPPDATA=D:\tcsb`. Run it in the foreground with a 10-minute timeout. Never put `LOCALAPPDATA` in the repo.
- **Telemetry:** read `TELEMETRY.md` before Task 3. Nothing sent may carry audio, transcripts, notes, paths, device names, keys or the hostname. Detail strings must be fixed codes. Update `TELEMETRY.md` in the same commit that adds a kind.
- **Fork-only strings:** they go in `src/shorthand/locales/en.json` (flat dotted keys), never in upstream's `src/i18n/locales/*`. Run `bun run check:fork-translations` and `bun run check:locale-drift`.
- **Copy:** it speaks as a note taker; plain and short.
- **UI:** screenshot every new overlay state before merge.
- **Default branch is `main`.** Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Comments:** comments explaining a Windows-only gate state the *actual* reason, cpal's default-device activation path, not "COM is flaky".
- **Keep the upstream diff small (`AGENTS.md` § "Keep the diff mergeable"):**
  - Tasks 1, 2 and 4 edit upstream files; do not reformat or tidy around the edits.
  - Task 1 is a genuine upstream bug (`upstream/main` `transcription_coordinator.rs:570` has the same `Instant::now()`) and is a candidate to offer to Handy afterwards.
  - Task 3's edit at the `Model::load_with` call will conflict with cjpais/Handy#2208. Keep it to the few lines shown so that merge only has to move it into the worker.

---

### Task 1: Measure hold-vs-tap from the key event's arrival time

**Files:**
- Modify: `src-tauri/src/transcription_coordinator.rs`
  - `InputEvent` struct (~:173)
  - the coordinator loop (~:1002-1006)
  - `send` (~:1139-1160)
  - the tests module (helper `input` ~:3011, plus every other `InputEvent { … }` literal; there are 12 in the file)

**Interfaces:**
- Produces: `InputEvent.at: Instant`, and `fn dispatch_input(state: &mut CoordinatorState, input: InputEvent) -> Option<Effect>`, which the loop uses.

- [ ] **Step 1: Write the failing test** (append to the `tests` module)

```rust
    /// 2026-10-06: a cold start blocked the coordinator thread for 416ms, so
    /// a 100ms tap's release was only dequeued ~400ms after the press. Timed
    /// from the dequeue it read as a hold and stopped a 0-sample recording.
    #[test]
    fn a_tap_stays_a_tap_when_its_release_is_dequeued_late() {
        let mut state = CoordinatorState::new();
        let pressed = Instant::now();
        let mut press = input(ShortcutActivation::HoldOrToggle, true);
        press.at = pressed;
        assert!(matches!(
            dispatch_input(&mut state, press),
            Some(Effect::Start { .. })
        ));

        let mut release = input(ShortcutActivation::HoldOrToggle, false);
        release.at = pressed + ms(100);
        // The coordinator reaches the release only after a slow start effect.
        std::thread::sleep(ms(400));
        assert!(dispatch_input(&mut state, release).is_none());

        assert!(
            state.on_grace_expired().is_none(),
            "a 100ms tap must lock the session on, not stop it"
        );
        assert!(state.is_locked());
    }
```

- [ ] **Step 2: Add the field and a `dispatch_input` that keeps today's behaviour, so the test can fail on its assertion**

In `struct InputEvent`, after `external`:

```rust
    /// When the key event reached the app, stamped by `send` on the shortcut
    /// thread. Hold-vs-tap is measured from this rather than from when this
    /// thread dequeues the event: a start effect runs on this thread and can
    /// block it for hundreds of milliseconds (a cold microphone open), which
    /// turned taps into holds.
    at: Instant,
```

In `send`, add `at: Instant::now(),` to the `InputEvent` literal. In the tests module, add `at: Instant::now(),` to the helper `input` and to every other `InputEvent { … }` literal; `cargo check --tests` lists any you missed.

Add next to `run_effect`:

```rust
fn dispatch_input(state: &mut CoordinatorState, input: InputEvent) -> Option<Effect> {
    state.on_input(input, Instant::now())
}
```

and in the loop replace `if let Some(effect) = state.on_input(input, Instant::now()) {` with `if let Some(effect) = dispatch_input(&mut state, input) {`.

- [ ] **Step 3: Run the test and confirm it fails**

Run (from `src-tauri`): `cargo test --lib transcription_coordinator::tests::a_tap_stays_a_tap_when_its_release_is_dequeued_late`
Expected: FAIL at `a 100ms tap must lock the session on, not stop it`.

- [ ] **Step 4: Time the event from its arrival**

```rust
fn dispatch_input(state: &mut CoordinatorState, input: InputEvent) -> Option<Effect> {
    let at = input.at;
    state.on_input(input, at)
}
```

- [ ] **Step 5: Run the coordinator tests**

Run: `cargo test --lib transcription_coordinator`
Expected: all pass (65 existing + 1 new).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/transcription_coordinator.rs
git commit -m "fix: time hold-vs-tap from the key event, not from when the coordinator reaches it"
```

---

### Task 2: Reopen the default device by id when a default open fails (Windows)

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/device.rs` (add two resolvers)
- Modify: the `audio_toolkit::audio` re-exports (the module that already re-exports `list_output_devices`; add the two new names next to it)
- Modify: `src-tauri/src/managers/audio.rs`, `start_microphone_stream` (:798, open block :882-910)

**Interfaces:**
- Produces: `pub fn pinned_default_input() -> Option<cpal::Device>` and `pub fn pinned_default_output() -> Option<cpal::Device>` in `audio_toolkit::audio`.

**Behaviour:**
- Every open tries cpal's default handle first, exactly as today.
- Only when that open fails is it retried with the endpoint opened by id, and only for that recording. Nothing is remembered.
- The next press tries the default handle again, so mid-recording default-device following is unchanged whenever the default handle works.
- The cost in the broken state is one failed attempt per press (under a second in the 2026-10-05 log).

This path talks to WASAPI and has no unit-testable seam. It is verified by the forced-failure hook in Step 3 and the real reproduction in Task 5.

- [ ] **Step 1: Add the resolvers** to `device.rs`

```rust
/// The current default endpoint, opened by its id rather than as cpal's
/// default-device handle. On Windows the default handle activates through
/// `ActivateAudioInterfaceAsync` and registers a default-device monitor; a
/// device obtained by id activates with `IMMDevice::Activate` on the calling
/// thread and registers no monitor. Used to recover when the default handle
/// stops opening.
pub fn pinned_default_input() -> Option<cpal::Device> {
    let host = crate::audio_toolkit::get_cpal_host();
    let id = host.default_input_device()?.id().ok()?;
    host.device_by_id(&id)
}

/// Output counterpart of [`pinned_default_input`], for the system-audio lane.
pub fn pinned_default_output() -> Option<cpal::Device> {
    let host = crate::audio_toolkit::get_cpal_host();
    let id = host.default_output_device()?.id().ok()?;
    host.device_by_id(&id)
}
```

Re-export both alongside `list_output_devices`.

- [ ] **Step 2: Retry a failed default open by id in `start_microphone_stream`**

After `system_audio` is computed (:870-873), add:

```rust
        // Windows: opening cpal's *default* handle goes through
        // `ActivateAudioInterfaceAsync`, which stayed broken after default-device
        // changes on 2026-10-05 until restart. Opening the same endpoint by id
        // uses `IMMDevice::Activate` instead. The default handle is still tried
        // first on every open, so it keeps following default-device changes
        // mid-recording whenever it works; the by-id retry covers one recording.
        #[cfg(windows)]
        let mic_is_default = resolution.device.is_none();
        #[cfg(windows)]
        let system_audio_is_default =
            requested_system_audio.enabled && requested_system_audio.device_name.is_none();
        #[cfg(windows)]
        let by_id_output = |current: Option<SystemAudioCapture>| {
            crate::audio_toolkit::audio::pinned_default_output()
                .map(|device| SystemAudioCapture { device })
                .or(current)
        };
        #[cfg(windows)]
        let mut system_audio = system_audio;
        #[cfg(windows)]
        let mut opened_by_id = false;
```

Replace the `Err(first_err)` arm (:888-898) with:

```rust
                    Err(first_err) => {
                        // A cached device or config may have gone stale (unplugged,
                        // rate/format changed). Re-resolve from a fresh enumeration and
                        // retry once before surfacing the error.
                        warn!(
                            "Recorder open failed ({first_err}); re-resolving device and retrying once"
                        );
                        self.invalidate_device_cache();
                        resolution = self.resolve_microphone_device(&settings);
                        #[cfg(windows)]
                        if mic_is_default {
                            warn!("Retrying this recording with the default devices opened by id");
                            resolution.device = crate::audio_toolkit::audio::pinned_default_input();
                            if system_audio_is_default {
                                system_audio = by_id_output(system_audio);
                            }
                            opened_by_id = true;
                        }
                        rec.open(resolution.device.clone(), system_audio)
                            .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?
                    }
```

Then, after the `match` (so `rec` is open), handle a system-audio lane that failed while the mic opened:

```rust
            #[cfg(windows)]
            let system_audio_active = if system_audio_is_default
                && !system_audio_active
                && !opened_by_id
                && matches!(rec.loopback_open_outcome(), LoopbackOpenOutcome::Unavailable { .. })
            {
                warn!("System audio failed on the default output; reopening this recording with it opened by id");
                rec.close()
                    .map_err(|e| anyhow::anyhow!("Failed to close recorder: {}", e))?;
                rec.open(resolution.device.clone(), by_id_output(None))
                    .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?
            } else {
                system_audio_active
            };
```

The first `rec.open` call keeps `system_audio.clone()`. On non-Windows nothing changes; the `system_audio` binding stays immutable there. Nothing is stored on the manager.

- [ ] **Step 3: Add a debug-only forced failure to exercise the path**

Gate the first attempt of the `match`:

```rust
            // Debug builds only: SHORTHAND_DEBUG_FAIL_DEFAULT_MIC=1 fails every
            // first default-mic open, to exercise the by-id retry without
            // reproducing the WASAPI fault.
            #[cfg(all(windows, debug_assertions))]
            let forced_failure = mic_is_default
                && std::env::var_os("SHORTHAND_DEBUG_FAIL_DEFAULT_MIC").is_some();
            #[cfg(not(all(windows, debug_assertions)))]
            let forced_failure = false;
            let first = if forced_failure {
                Err("forced default-mic open failure (SHORTHAND_DEBUG_FAIL_DEFAULT_MIC)".into())
            } else {
                rec.open(resolution.device.clone(), system_audio.clone())
            };
            let system_audio_active = match first {
```

- [ ] **Step 4: Build, lint, and run the audio tests**

Run (from `src-tauri`): `cargo clippy --all-targets -- -D warnings` then `cargo test --lib audio`
Expected: clean; all pass.

- [ ] **Step 5: Exercise the retry in a debug build**

1. Run `bun run tauri dev` with `SHORTHAND_DEBUG_FAIL_DEFAULT_MIC=1` and the mic set to Default.
2. Dictate twice.
3. Expected log on **each** press: `Recorder open failed (forced …)`, then `Retrying this recording with the default devices opened by id`, then `Microphone stream initialized`. A transcript is pasted both times.
4. Restart without the variable and dictate. Expected: no retry warning; the default handle opens as before.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/audio_toolkit src-tauri/src/managers/audio.rs
git commit -m "fix: retry a failed default mic or output open with the device opened by id on Windows"
```

---

### Task 3: Report mic-open failures and native crashes to Sentry

**Files:**
- Create: `src-tauri/src/shorthand/native_marker.rs`
- Modify: `src-tauri/src/shorthand/mod.rs` (`pub mod native_marker;`)
- Modify: `src-tauri/src/managers/transcription.rs:1045-1050` (wrap `Model::load_with`)
- Modify: `src-tauri/src/lib.rs` (after `shorthand::telemetry::apply_stored(app_handle);`, ~:273)
- Modify: `src-tauri/src/actions.rs:868-885` (start-failure branch)
- Modify: `TELEMETRY.md` (the "named failures" bullet, lines 19-23)

**Interfaces:**
- Produces: `native_marker::begin(dir: &Path, stage: &'static str, accel: &'static str)`, `native_marker::end(dir: &Path)`, and `native_marker::take_previous(dir: &Path) -> Option<String>` (returns `"<stage>:<accel>"`).
- Consumes: `telemetry::report_error(kind: &'static str, detail: Option<&str>)`.

- [ ] **Step 1: Write the failing tests** in `native_marker.rs`

```rust
//! A crash marker for native calls that can abort the process without a Rust
//! panic (e.g. vulkan-1.dll's fail-fast on 2026-10-06). `begin` writes it
//! before the call and `end` removes it after; a marker still present at the
//! next launch means the process died inside that call. Holds only fixed
//! codes, so reporting it keeps TELEMETRY.md's promise.

use std::path::{Path, PathBuf};

const FILE: &str = "native-call.marker";

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

pub fn begin(dir: &Path, stage: &'static str, accel: &'static str) {
    let _ = std::fs::create_dir_all(dir);
    if let Err(e) = std::fs::write(path(dir), format!("{stage}:{accel}")) {
        log::warn!("Could not write native-call marker: {e}");
    }
}

pub fn end(dir: &Path) {
    let _ = std::fs::remove_file(path(dir));
}

/// Returns and clears a marker left by a previous run. Only codes this module
/// wrote are returned, so a hand-edited file cannot inject text into a report.
pub fn take_previous(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path(dir)).ok()?;
    end(dir);
    let valid = text
        .split_once(':')
        .is_some_and(|(s, a)| STAGES.contains(&s) && ACCELS.contains(&a));
    valid.then_some(text)
}

pub const STAGES: &[&str] = &["model_load"];
pub const ACCELS: &[&str] = &["auto", "cpu", "pinned_device"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_left_behind_is_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        begin(dir.path(), "model_load", "pinned_device");
        assert_eq!(
            take_previous(dir.path()).as_deref(),
            Some("model_load:pinned_device")
        );
        assert_eq!(take_previous(dir.path()), None);
    }

    #[test]
    fn a_completed_call_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        begin(dir.path(), "model_load", "cpu");
        end(dir.path());
        assert_eq!(take_previous(dir.path()), None);
    }

    #[test]
    fn unknown_text_is_cleared_but_never_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "C:\\Users\\someone:x").unwrap();
        assert_eq!(take_previous(dir.path()), None);
        assert!(!dir.path().join(FILE).exists());
    }
}
```

(`tempfile` is already a dependency.) Add `pub mod native_marker;` to `shorthand/mod.rs`.

- [ ] **Step 2: Run the tests**

Run: `cargo test --lib shorthand::native_marker`
Expected: 3 pass. These are behaviour tests of a new module, so they pass on first run. Check they would fail by temporarily removing `end(dir)` from `take_previous`: the first test should fail. Then restore it.

- [ ] **Step 3: Wrap the native load**

In `managers/transcription.rs`, replace the block at :1045-1050:

```rust
                let marker_dir = self.app_handle.path().app_data_dir().ok();
                let accel = if model_options.device.is_some() {
                    "pinned_device"
                } else if matches!(model_options.backend, Backend::Cpu) {
                    "cpu"
                } else {
                    "auto"
                };
                let model = {
                    let _load_guard = native_model_load_lock()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if let Some(dir) = &marker_dir {
                        crate::shorthand::native_marker::begin(dir, "model_load", accel);
                    }
                    let loaded = Model::load_with(&model_path, &model_options);
                    if let Some(dir) = &marker_dir {
                        crate::shorthand::native_marker::end(dir);
                    }
                    loaded
                }
```

Match `Backend` to the import already used by `load_model_with_device_inner`; check the `ModelOptions` field names there before writing.

- [ ] **Step 4: Report a leftover marker at startup**

In `lib.rs`, right after `shorthand::telemetry::apply_stored(app_handle);`:

```rust
    if let Ok(dir) = app_handle.path().app_data_dir() {
        if let Some(code) = shorthand::native_marker::take_previous(&dir) {
            log::error!("The previous run ended inside a native call ({code})");
            shorthand::telemetry::report_error("native_crash", Some(&code));
        }
    }
```

- [ ] **Step 5: Report mic-open failures**

In `actions.rs`, inside the start-failure branch, after `let error_type = match …;` (:874-878):

```rust
                // The fixed error_type only; `err` can name the device.
                crate::shorthand::telemetry::report_error("mic_open", Some(error_type));
```

- [ ] **Step 6: Update `TELEMETRY.md`**

Replace "One of four named failures" with "One of six named failures". After the `request_socket_listen` item add:

> `mic_open` (a fixed reason: microphone permission denied, no input device, or unknown), and `native_crash` (sent at the next launch when Shorthand stopped inside model loading: the stage and whether a GPU was pinned, CPU, or automatic; no memory or crash dump)

- [ ] **Step 7: Build, test, commit**

Run: `cargo clippy --all-targets -- -D warnings` and `cargo test --lib`
Expected: clean; all pass.

```bash
git add src-tauri/src/shorthand src-tauri/src/managers/transcription.rs src-tauri/src/lib.rs src-tauri/src/actions.rs TELEMETRY.md
git commit -m "feat: report mic-open failures and native crashes during model load"
```

---

### Task 4: Show capture failures in the overlay

**Files:**
- Create: `src-tauri/src/shorthand/overlay_error.rs` (fork-only: maps failures to fixed kinds), and add `pub mod overlay_error;` to `src-tauri/src/shorthand/mod.rs`
- Modify: `src-tauri/src/overlay.rs`
  - `overlay_dimensions` (:57)
  - size constants (:49-54)
  - add `show_error_overlay` next to `show_processing_overlay` (~:669)
- Modify: `src-tauri/src/actions.rs` (start-failure branch :868-885; transcription failure :1310-1312)
- Create: `src/shorthand/overlayError.ts` and `src/shorthand/overlayError.test.ts`
- Modify: `src/overlay/RecordingOverlay.tsx` (state type :16; listeners; render)
- Modify: `src/overlay/RecordingOverlay.css`
- Modify: `src/shorthand/locales/en.json`

**Interfaces:**
- Produces (Rust): `overlay_error::for_start_failure(error_type: &str) -> &'static str` and `overlay_error::for_transcription_reason(reason: &str) -> &'static str`.
- Produces (Rust): `overlay::show_error_overlay(app_handle: &AppHandle, kind: &'static str, saved: bool)`. It emits `overlay-error` with `{ kind, saved }`, shows state `"error"`, and hides after 3 s unless the overlay was shown or hidden again meanwhile.
- Produces (TS): `overlayErrorKeys(kind: string, saved: boolean): string[]`, the copy keys in display order.
- Consumes: `error_type` from `start_failure_code` (`actions.rs:874-878`), `telemetry::transcription_reason`, and `wav_saved` (`actions.rs:1095`).

**Copy.** This was agreed with the user on 2026-10-06. Do not reword it without asking. Name the app only when telling the user to restart it.

| kind | first line | second line |
|---|---|---|
| `mic_permission_windows` | Can't use your microphone. | Turn it on in Windows Settings → Privacy → Microphone. |
| `mic_permission_macos` | Can't use your microphone. | Turn it on in System Settings → Privacy & Security → Microphone. |
| `mic_permission` | Can't use your microphone. | Turn on microphone access in your system settings. |
| `mic_missing` | No microphone found. | — |
| `mic_failed` | Your microphone wouldn't start. | Restart Shorthand to fix it. |
| `not_ready` | Couldn't get ready to transcribe. | Restart Shorthand and try again. |
| `busy` | Still finishing your last note. | — |
| `transcribe_failed` | Couldn't transcribe that recording. | — |
| unknown kind | Something went wrong with that recording. | — |

When `saved` is true, "Your recording is in History." is added as the last line. That is only true when the WAV was saved, so it follows `wav_saved`.

- [ ] **Step 1: Write the failing Rust tests** in `src-tauri/src/shorthand/overlay_error.rs`

```rust
//! Maps capture failures to the fixed kinds the overlay's error state shows.
//! Kinds are codes, never messages: the overlay looks up the copy.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_failures_map_to_their_kinds() {
        assert_eq!(for_start_failure("no_input_device"), "mic_missing");
        assert_eq!(for_start_failure("unknown"), "mic_failed");
        assert_eq!(for_start_failure("anything_else"), "mic_failed");
        #[cfg(windows)]
        assert_eq!(
            for_start_failure("microphone_permission_denied"),
            "mic_permission_windows"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            for_start_failure("microphone_permission_denied"),
            "mic_permission_macos"
        );
    }

    #[test]
    fn transcription_reasons_map_to_their_kinds() {
        assert_eq!(for_transcription_reason("model_not_loaded"), "not_ready");
        assert_eq!(for_transcription_reason("engine_busy"), "busy");
        assert_eq!(for_transcription_reason("finalize_timeout"), "busy");
        assert_eq!(for_transcription_reason("engine_error"), "transcribe_failed");
        assert_eq!(for_transcription_reason("engine_panic"), "transcribe_failed");
        assert_eq!(for_transcription_reason("other"), "transcribe_failed");
    }
}
```

Add `pub mod overlay_error;` to `shorthand/mod.rs`.

Run: `cargo test --lib shorthand::overlay_error`
Expected: FAIL to compile, `for_start_failure` not found.

- [ ] **Step 2: Implement** above the tests

```rust
/// `error_type` is the code `actions.rs` already sends with `recording-error`.
pub fn for_start_failure(error_type: &str) -> &'static str {
    match error_type {
        "microphone_permission_denied" => {
            if cfg!(windows) {
                "mic_permission_windows"
            } else if cfg!(target_os = "macos") {
                "mic_permission_macos"
            } else {
                "mic_permission"
            }
        }
        "no_input_device" => "mic_missing",
        _ => "mic_failed",
    }
}

/// `reason` is `telemetry::transcription_reason`'s code.
pub fn for_transcription_reason(reason: &str) -> &'static str {
    match reason {
        "model_not_loaded" => "not_ready",
        "engine_busy" | "finalize_timeout" => "busy",
        _ => "transcribe_failed",
    }
}
```

Run: `cargo test --lib shorthand::overlay_error`. Expected: PASS.

- [ ] **Step 3: Write the failing TS test** `src/shorthand/overlayError.test.ts`

```ts
import { describe, expect, test } from "bun:test";
import { overlayErrorKeys } from "./overlayError";

describe("overlayErrorKeys", () => {
  test("a restart message keeps its second line", () => {
    expect(overlayErrorKeys("mic_failed", false)).toEqual([
      "overlay.error.micFailed",
      "overlay.error.micFailed.action",
    ]);
  });
  test("a one-line message has no second line", () => {
    expect(overlayErrorKeys("mic_missing", false)).toEqual([
      "overlay.error.micMissing",
    ]);
  });
  test("a saved recording adds the History line last", () => {
    expect(overlayErrorKeys("transcribe_failed", true)).toEqual([
      "overlay.error.transcribeFailed",
      "overlay.error.savedInHistory",
    ]);
  });
  test("an unknown kind gets the generic line", () => {
    expect(overlayErrorKeys("something_new", false)).toEqual([
      "overlay.error.generic",
    ]);
  });
});
```

Run: `bun test src/shorthand/overlayError.test.ts`
Expected: FAIL, cannot find module `./overlayError`.

- [ ] **Step 4: Implement** `src/shorthand/overlayError.ts`, and add the copy

```ts
// Copy keys for the overlay's error state. Kinds are fixed codes from
// `shorthand::overlay_error` in the backend.
const KEYS: Record<string, string[]> = {
  mic_permission_windows: [
    "overlay.error.micPermission",
    "overlay.error.micPermission.windows",
  ],
  mic_permission_macos: [
    "overlay.error.micPermission",
    "overlay.error.micPermission.macos",
  ],
  mic_permission: [
    "overlay.error.micPermission",
    "overlay.error.micPermission.other",
  ],
  mic_missing: ["overlay.error.micMissing"],
  mic_failed: ["overlay.error.micFailed", "overlay.error.micFailed.action"],
  not_ready: ["overlay.error.notReady", "overlay.error.notReady.action"],
  busy: ["overlay.error.busy"],
  transcribe_failed: ["overlay.error.transcribeFailed"],
};

export function overlayErrorKeys(kind: string, saved: boolean): string[] {
  const keys = KEYS[kind] ?? ["overlay.error.generic"];
  return saved ? [...keys, "overlay.error.savedInHistory"] : keys;
}
```

Add to `src/shorthand/locales/en.json`, keeping its alphabetical order:

```json
  "overlay.error.busy": "Still finishing your last note.",
  "overlay.error.generic": "Something went wrong with that recording.",
  "overlay.error.micFailed": "Your microphone wouldn't start.",
  "overlay.error.micFailed.action": "Restart Shorthand to fix it.",
  "overlay.error.micMissing": "No microphone found.",
  "overlay.error.micPermission": "Can't use your microphone.",
  "overlay.error.micPermission.macos": "Turn it on in System Settings → Privacy & Security → Microphone.",
  "overlay.error.micPermission.other": "Turn on microphone access in your system settings.",
  "overlay.error.micPermission.windows": "Turn it on in Windows Settings → Privacy → Microphone.",
  "overlay.error.notReady": "Couldn't get ready to transcribe.",
  "overlay.error.notReady.action": "Restart Shorthand and try again.",
  "overlay.error.savedInHistory": "Your recording is in History.",
  "overlay.error.transcribeFailed": "Couldn't transcribe that recording.",
```

Run: `bun test src/shorthand/overlayError.test.ts`. Expected: PASS.

`bun run check:branding` may flag "Shorthand" in copy. It is intended here; follow `BRANDING.md` if the gate needs an allowance.

- [ ] **Step 5: Backend: size, show, and hide the error state**

In `overlay.rs`, next to the size constants:

```rust
// Up to three short lines (message, action, "in History").
const OVERLAY_ERROR_WIDTH: f64 = 360.0;
const OVERLAY_ERROR_HEIGHT: f64 = 88.0;
const ERROR_OVERLAY_DURATION: std::time::Duration = std::time::Duration::from_millis(4000);
```

In `overlay_dimensions`, add a branch so `"error"` returns `(OVERLAY_ERROR_WIDTH, OVERLAY_ERROR_HEIGHT)`.

The native window must be hidden by the backend; hiding only the React content leaves the window up. A recording that starts during the delay must not be hidden by the timer. `show_overlay_state` already calls `bump_visibility_epoch()` synchronously on every show, and so does every hide, so the timer captures the epoch after showing and hides only if it is unchanged:

```rust
/// Shows a short error in the overlay. `kind` is a fixed code from
/// `shorthand::overlay_error`; `saved` adds the "in History" line. Hides after
/// `ERROR_OVERLAY_DURATION` unless the overlay was shown or hidden again first.
pub fn show_error_overlay(app_handle: &AppHandle, kind: &'static str, saved: bool) {
    #[derive(Clone, serde::Serialize)]
    struct OverlayError {
        kind: &'static str,
        saved: bool,
    }
    let _ = app_handle.emit_to("recording_overlay", "overlay-error", OverlayError { kind, saved });
    show_overlay_state(app_handle, "error");
    let shown = OVERLAY_VISIBILITY_EPOCH.load(Ordering::Relaxed);
    let app = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(ERROR_OVERLAY_DURATION);
        if OVERLAY_VISIBILITY_EPOCH.load(Ordering::Relaxed) == shown {
            hide_recording_overlay(&app);
        }
    });
}
```

The duration is 4 s rather than 3 s because the copy runs to two or three lines.

In `actions.rs`:
- **Start-failure branch:** compute `error_type` before the overlay call, then replace `utils::hide_recording_overlay(app);` with `crate::overlay::show_error_overlay(app, crate::shorthand::overlay_error::for_start_failure(error_type), false);`. Keep the rest of the branch.
- **Transcription failure:** next to `let _ = ah.emit("transcription-error", error_message);` add:

```rust
                            crate::overlay::show_error_overlay(
                                &ah,
                                crate::shorthand::overlay_error::for_transcription_reason(
                                    crate::shorthand::telemetry::transcription_reason(&err.to_string()),
                                ),
                                wav_saved,
                            );
```

Check that nothing after it in that branch hides the overlay immediately. If something does, skip that hide when an error was shown.

- [ ] **Step 6: Frontend: render the state**

In `RecordingOverlay.tsx`:
- Change the state type to `"recording" | "streaming" | "transcribing" | "processing" | "error"`.
- Add `const [errorLines, setErrorLines] = useState<string[]>([]);`.
- Inside `setupEventListeners`, add:

```tsx
      const unlistenError = await listen<{ kind: string; saved: boolean }>(
        "overlay-error",
        (event) => {
          setErrorLines(overlayErrorKeys(event.payload.kind, event.payload.saved));
        },
      );
```

- Call `unlistenError()` in the cleanup alongside the others. The backend hides the overlay; add no timer here.

In the render, `state === "error"` shows a rounded panel (the pill's radius and padding) with no spinner or waveform:
- The first line uses the pill's label style.
- The remaining lines use a smaller, muted style.
- Each line is `t(key)`. Add an `overlay-error` class and style it in `RecordingOverlay.css` with the existing brand tokens from `src/shorthand/brand/theme.css` for text and a warning accent. Add no new colours.
- Lines wrap within `OVERLAY_ERROR_WIDTH`, so do not truncate them.

- [ ] **Step 7: Checks**

Run: `bun run lint`, `bunx tsc --noEmit`, `bun run check:fork-translations`, `bun run check:locale-drift`, `bun run check:branding`, `bun run test:unit`, and from `src-tauri` `cargo clippy --all-targets -- -D warnings` plus `cargo test --lib shorthand::overlay_error`
Expected: all clean.

- [ ] **Step 8: Look at it**

1. In `bun run tauri dev`, trigger:
   - `mic_failed`: `SHORTHAND_DEBUG_FAIL_DEFAULT_MIC=1` together with the mic unplugged, so the retry also fails.
   - `mic_missing`: no input devices enabled.
   - `not_ready`: dictate with the selected model's file renamed away.
   - `transcribe_failed` with "Save recordings" on, to see the History line.
2. Screenshot each with the overlay at the top and at the bottom, in light and dark.
3. Check that no line is clipped, that the overlay disappears after about 4 s, and that a press during those 4 s shows the normal recording state and is not hidden by the timer.
4. Show the screenshots to the user before merging.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/shorthand/overlay_error.rs src-tauri/src/shorthand/mod.rs src-tauri/src/overlay.rs src-tauri/src/actions.rs src/shorthand/overlayError.ts src/shorthand/overlayError.test.ts src/shorthand/locales/en.json src/overlay
git commit -m "feat: say what failed and what to do in the overlay instead of hiding it"
```

---

### Task 5: Reproduce the original faults on a release build

No code. This step confirms the fixes against the real failures, not only the debug hook.

- [ ] **Step 1:** Build an installer (`BUILD.md`) and install it over 0.6.0. Leave the mic and system audio on Default.
- [ ] **Step 2 (tap):**
  1. Wait for the idle mic stream to close (30 s) and the model to unload (5 min).
  2. Tap the dictate hotkey (under 300 ms) and speak.
  3. Expected: the session locks on (`Tap (…ms) … recording locked on` in the log) and the second tap transcribes. No 0-sample stop.
- [ ] **Step 3 (mic):**
  1. Start a Meeting session.
  2. During it, unplug the USB mic (PowerConf S3), plug it back in, and switch the Windows default output once.
  3. End the meeting and dictate.
  4. Expected: if the default handle now fails, the log shows the by-id retry and dictation works with no restart. If it does not fail, record that the trigger was not reproduced; the forced-failure check in Task 2 still covers the recovery path.
- [ ] **Step 4 (Sentry):**
  1. Telemetry on. In a debug build, end the process with `taskkill /F` while a model is loading (set a large model and dictate).
  2. Relaunch.
  3. Expected: a `native_crash: model_load:<accel>` event in Sentry project `shorthand-app`.
- [ ] **Step 5:** Record the results in the PR description.

---

### Task 6: Draft the tap-timing fix as an upstream PR

The same bug is on `upstream/main` (`transcription_coordinator.rs:570`: `state.on_input(input, Instant::now())`). Upstream's coordinator differs from the fork's by about 1,800 lines, so the fix is redone against upstream rather than cherry-picked.

**Rules (`AGENTS.md` § "GitHub workflow for AI coding assistants"):**
- Read `.github/PULL_REQUEST_TEMPLATE.md` and fill in every section.
- Leave "Human Written Description" as a clearly marked TODO for the user.
- Do not open the PR. Handy's template says AI-opened PRs are closed; the user submits it through the GitHub website.
- Pushing the branch to `origin` is outward-facing, so ask the user before pushing.

- [ ] **Step 1: Check for duplicates**

Run:

```bash
gh search issues --repo cjpais/Handy "tap hold toggle" --limit 20
gh search prs --repo cjpais/Handy "hold threshold" --state open --limit 20
```

If an existing issue describes taps stopping a recording, or toggle/hold misfiring on a slow start, link it in the PR. If an open PR already fixes it, stop and tell the user.

- [ ] **Step 2: Branch from upstream**

```bash
git fetch upstream
git worktree add ../.worktrees/upstream-tap-timing -b fix/tap-timed-from-key-event upstream/main
```

- [ ] **Step 3: Port Task 1** to upstream's `transcription_coordinator.rs`

Apply the same three changes:
- the `at: Instant` field on upstream's `InputEvent`;
- the stamp in its `send`;
- `dispatch_input` used by its loop.

Adapt the Task 1 test to upstream's test helpers (they differ from the fork's). Prove it fails against `Instant::now()` first, then passes. Keep the diff to those lines; upstream reviews for one fix per PR.

- [ ] **Step 4: Run upstream's checks**

From `src-tauri`, with the Global Constraints env vars: `cargo test --lib transcription_coordinator`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`.

- [ ] **Step 5: Commit, and write the PR body to a file**

Commit with a conventional message (`fix: time hold-vs-tap from the key event, not from when the coordinator reaches it`).

Write the body to `docs/upstream-pr-tap-timing.md` in the *fork* worktree, not on the upstream branch. Fill in the template:
- what happened: a slow start, such as a cold mic open, blocks the coordinator thread, so a tap's release is timed late and reads as a hold;
- the fix;
- the test;
- `TODO(user): Human Written Description`.

- [ ] **Step 6: Hand over**

Ask the user whether to push `fix/tap-timed-from-key-event` to `origin`. Then give them the compare URL (`https://github.com/cjpais/Handy/compare/main...mshish:shorthand:fix/tap-timed-from-key-event`) and the body file, so they can open the PR themselves.

---

## Not in this plan

- **GPU recovery:** in-process re-init is impossible with transcribe-cpp ≤ 0.3.0 (ggml caches a failed device). It waits for cjpais/Handy#2208 (worker process) per the user's decision.
  - At that merge, implement the agreed policy: respawn and retry the GPU with backoff, then CPU for that load, then try the GPU again on the next load.
  - Until then, a second load after a Vulkan init failure can still abort the process. Task 3 makes that visible.
- **Heap-corruption crashes** (`c0000374`, 5 occurrences 2026-09-18 to 09-26 on 0.5.0, Windows Error Reporting archive): not investigated.
- **The audio-feedback chime** (rodio, default output) can still fail silently while the default handle is broken. The user accepted this.
- **Staying pinned to a device after a failure:** rejected by the user, because it would stop following default-device changes mid-recording.
