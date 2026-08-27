# Plan C — System audio capture on macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture system (output) audio on macOS as a second speaker-labelled lane alongside the microphone, with a permission flow a user can complete without instructions — one system prompt, and if declined, one button that takes them to the right place.

**Architecture:** Plan A already made the capture machinery platform-neutral and put cpal 0.18 in place, whose CoreAudio backend implements loopback via Apple's Core Audio Process Tap. Because Plan A also set the app's minimum to macOS 14.6 — at or above every version requirement involved — **no runtime OS-version check is needed anywhere in this plan.** What remains is macOS-specific: the Info.plist consent string, observing permission state (there is no API to query it), and the UI for a declined prompt.

**Tech Stack:** Rust, `cpal` 0.18.x (CoreAudio Process Tap), Tauri 2.x, React/TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Prerequisite:** Plan A (`2026-08-27-plan-a-cpal-018-migration.md`) must be complete and green. Independent of Plan B (Linux) — either may land first, so Tasks 1 and 3 begin by checking what already exists.

## Global Constraints

- **No OS-version gating.** The app minimum is macOS 14.6 (set in Plan A), which is at the loopback requirement. Any version check would be dead code.
- **No entitlement change.** Process Tap capture is gated by the `NSAudioCaptureUsageDescription` Info.plist key alone. This app does not use App Sandbox — `Entitlements.plist` declares only microphone/audio-input — and hardened runtime plus that key is sufficient. Do not add sandbox entitlements.
- **Permission state can only be observed, never queried.** There is no precheck API for `kTCCServiceAudioCapture`. Any code that appears to ask the OS for current status is wrong.
- **No React unit tests.** Per `docs/FRONTEND_TESTING.md` this repo deliberately has no vitest/jest harness. Frontend verification is manual.
- Ship as a `.app` bundle (Tauri's default). A bare executable requesting this permission class cannot be managed from System Settings on current macOS.
- All `cargo` commands use `--manifest-path src-tauri/Cargo.toml`.

---

### Task 1: macOS device resolution

**Files:**
- Modify: `src-tauri/src/managers/audio.rs` (`get_effective_system_audio_device`)

**Interfaces:**
- Produces: `get_effective_system_audio_device` resolves a real output device on macOS, so `open()` builds a loopback stream instead of running microphone-only.

Plan A already added `get_system_audio_host()`, whose non-Linux arm returns the default host — which on macOS is CoreAudio, the one with Process Tap loopback. So macOS needs no new host logic: only Plan A's deliberate placeholder has to come out.

**Do not add a Linux arm here.** Plan A's Linux arm returns `None` on purpose, and Plan B replaces it. Naming `HostId::PipeWire`/`PulseAudio` from this plan would fail to compile on Linux, because only Plan B enables the Cargo features those variants live behind — which would break an A+C branch in CI.

- [ ] **Step 1: Check whether Plan B already did this**

```bash
grep -n "get_system_audio_host\|cfg(not(windows))" src-tauri/src/managers/audio.rs
```

If `get_effective_system_audio_device` already calls `get_system_audio_host()` and Plan A's `#[cfg(not(windows))]` stub is gone, Plan B landed first and did this work — **skip to Task 2.** Otherwise continue.

- [ ] **Step 2: Remove Plan A's stub and resolve through the loopback host**

In `get_effective_system_audio_device`, delete the `#[cfg(not(windows))] { let _ = device_name; None }` block Plan A added (and unwrap the `#[cfg(windows)]` block around the real body, so it applies to every platform). Change the default-device branch from `crate::audio_toolkit::get_cpal_host().default_output_device()` to:

```rust
            crate::audio_toolkit::get_system_audio_host()
                .and_then(|host| host.default_output_device())
```

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/audio_toolkit src-tauri/src/managers/audio.rs
git commit -m "feat(macos): resolve system audio devices via the loopback host"
```

---

### Task 2: The consent string

**Files:**
- Modify: `src-tauri/Info.plist`

**Interfaces:** none — but this string is shown verbatim in the OS consent dialog and is the single most user-visible artifact of this plan. It is the permission UX.

- [ ] **Step 1: Add the key**

Change `src-tauri/Info.plist` to:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSMicrophoneUsageDescription</key>
  <string>Request microphone access to transcribe audio locally</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>Shorthand records audio playing on this Mac so it can transcribe the other side of a call or meeting. Audio is transcribed locally.</string>
</dict>
</plist>
```

The wording matters: it names what is captured, why, and that it stays local — the three things a user weighs when the dialog appears with no other context.

- [ ] **Step 2: Verify it reaches the bundle**

`--no-bundle` skips bundling and produces no `.app`, so the bundle must actually be built:

```bash
bun run tauri build --bundles app
plutil -p src-tauri/target/release/bundle/macos/*.app/Contents/Info.plist | grep -A1 AudioCapture
```

Expected: the key and string appear verbatim. If the `.app` path differs, locate it with `find src-tauri/target/release/bundle -name "Info.plist"`. If the build fails at the signing step, that is the pre-existing `signCommand`/Trusted Signing issue documented in `BUILD.md` — it does not affect whether the plist was written, so check for the `.app` anyway.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Info.plist
git commit -m "feat(macos): add NSAudioCaptureUsageDescription consent string"
```

---

### Task 3: Observe permission state and expose availability

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (state registration and `collect_commands![...]`)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Produces:
  - `SystemAudioAvailability` — variants `Available`, `UnavailableNoSoundServer`, `PermissionDenied` (declare it here if Plan B has not already; check first).
  - `MacosSystemAudioCaptureState(pub Mutex<PermissionAccess>)` — Tauri-managed, holding the last *observed* outcome of a capture attempt.
  - `pub fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability`

**Read this before writing any code — the obvious approach does not work.**

You cannot detect denial from whether `update_system_audio_capture` returned `Ok`. `AudioRecorder::open()` deliberately swallows loopback failures: `recorder.rs:417-436` catches a failed `build_loopback_stream`, logs a warning, and continues **microphone-only and successful**. A denied TCC prompt therefore produces `Ok(())`, and a classifier reading that result would record `Allowed` on every denial — the deny CTA would never appear.

The correct signal is the flag Plan A Task 5 surfaced: `AudioRecordingManager::system_audio_active()`, which reports whether the loopback stream actually opened. Enable succeeded **and** the lane is live → `Allowed`. Enable succeeded but the lane is dead → something refused it, which on a Mac at our minimum OS is overwhelmingly a declined prompt → `Denied`.

**On classification precision.** The exact error macOS produces on TCC denial is **not verified** — Task 7 verifies it on real hardware. Rather than bet the UX on an unconfirmed OSStatus, this task treats "loopback did not come up" as "needs permission", with copy that reads correctly either way. Once Task 7 establishes the real signature, the classifier can be tightened to distinguish denial from a merely-missing device, with no UI change.

- [ ] **Step 1: Check what Plan B already added**

```bash
grep -n "enum SystemAudioAvailability\|fn get_system_audio_availability" src-tauri/src/commands/audio.rs
```

If present, Plan B landed first: you will **extend** the existing enum usage and change the function's signature to take `app: AppHandle`. If absent, you add both.

- [ ] **Step 2: Write the failing test**

Add to `src-tauri/src/commands/audio.rs`:

```rust
#[cfg(test)]
mod macos_system_audio_permission_tests {
    use super::*;

    #[test]
    fn a_live_loopback_lane_is_observed_as_allowed() {
        assert_eq!(
            observe_capture_outcome(true, true),
            PermissionAccess::Allowed
        );
    }

    #[test]
    fn a_dead_loopback_lane_after_a_successful_enable_is_observed_as_denied() {
        // The enable "succeeded" because open() degrades to microphone-only
        // rather than failing. The lane being dead is the real signal.
        // Deliberately coarse: macOS offers no way to ask why a tap failed,
        // and at our minimum OS a declined prompt is the likeliest cause.
        assert_eq!(
            observe_capture_outcome(true, false),
            PermissionAccess::Denied
        );
    }

    #[test]
    fn an_outright_failed_enable_says_nothing_about_permission() {
        // The stream never got far enough to ask. Don't accuse the user.
        assert_eq!(
            observe_capture_outcome(false, false),
            PermissionAccess::Unknown
        );
    }

    #[test]
    fn denial_is_reported_as_permission_denied_availability() {
        assert_eq!(
            macos_availability(PermissionAccess::Denied),
            SystemAudioAvailability::PermissionDenied
        );
    }

    #[test]
    fn an_unattempted_state_reports_available_not_denied() {
        // Before any attempt we must not accuse the user of having denied
        // something they were never asked about.
        assert_eq!(
            macos_availability(PermissionAccess::Unknown),
            SystemAudioAvailability::Available
        );
    }
}
```

- [ ] **Step 3: Run it and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_permission_tests
```

Expected: FAIL — `observe_capture_outcome` and `macos_availability` not found.

- [ ] **Step 4: Implement**

Add to `src-tauri/src/commands/audio.rs` (declaring `SystemAudioAvailability` too if Step 1 found it absent — copy the definition from Plan B Task 4 Step 3 verbatim so the two plans cannot drift):

```rust
/// The last observed outcome of a macOS system-audio capture attempt.
///
/// macOS exposes no way to query `kTCCServiceAudioCapture` state, so this is
/// the only source of truth available: it records what happened the last time
/// we tried, not what the OS currently thinks.
pub struct MacosSystemAudioCaptureState(pub std::sync::Mutex<PermissionAccess>);

/// Maps a capture attempt to an observed permission state.
///
/// `enable_succeeded` is whether `update_system_audio_capture` returned `Ok`;
/// `loopback_live` is `AudioRecordingManager::system_audio_active()` after it.
/// The second is the load-bearing one: `open()` degrades to microphone-only on
/// a refused loopback rather than failing, so a denied prompt looks like a
/// successful enable with a dead lane.
fn observe_capture_outcome(enable_succeeded: bool, loopback_live: bool) -> PermissionAccess {
    match (enable_succeeded, loopback_live) {
        (true, true) => PermissionAccess::Allowed,
        (true, false) => PermissionAccess::Denied,
        // The enable itself failed, so the tap was never reached — this tells
        // us nothing about permission.
        (false, _) => PermissionAccess::Unknown,
    }
}

/// Availability from the last observed permission state. `Unknown` means we
/// have not attempted capture yet, which is not a denial.
fn macos_availability(observed: PermissionAccess) -> SystemAudioAvailability {
    match observed {
        PermissionAccess::Denied => SystemAudioAvailability::PermissionDenied,
        PermissionAccess::Allowed | PermissionAccess::Unknown => {
            SystemAudioAvailability::Available
        }
    }
}
```

Then write `get_system_audio_availability` so each platform contributes its own answer:

```rust
#[tauri::command]
#[specta::specta]
pub fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability {
    #[cfg(target_os = "macos")]
    {
        let observed = *app
            .state::<MacosSystemAudioCaptureState>()
            .0
            .lock()
            .unwrap();
        macos_availability(observed)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &app;
        availability_from_host_probe(crate::audio_toolkit::get_system_audio_host().is_some())
    }
}
```

If Plan B has not landed, `availability_from_host_probe` does not exist yet — add it from Plan B Task 4 Step 3, unchanged, so whichever plan lands second finds it already correct.

- [ ] **Step 5: Record the observation at the capture attempt**

In `change_system_audio_enabled_setting`, the existing body calls `update_system_audio_capture` through `spawn_blocking` and maps its error. Capture that result before mapping it, and record the observation on macOS:

```rust
        let capture_result = tokio::task::spawn_blocking(move || {
            manager_for_update.update_system_audio_capture(enabled, device_name, stream_router)
        })
        .await
        .map_err(|error| format!("audio task join failed: {error}"))?;

        // Only an enable attempt tells us anything about permission; disabling
        // always succeeds and must not overwrite a known state. Note we ask
        // the manager whether the lane is actually live — `capture_result`
        // alone is Ok even when the tap was refused.
        #[cfg(target_os = "macos")]
        if enabled {
            let observed =
                observe_capture_outcome(capture_result.is_ok(), manager.system_audio_active());
            if observed != PermissionAccess::Unknown {
                *app.state::<MacosSystemAudioCaptureState>().0.lock().unwrap() = observed;
            }
        }

        capture_result
            .map_err(|error| format!("Failed to update system audio capture: {error}"))?;
```

Splice this to match the real surrounding code — read it first; the point is that the result is observed before `?` discards it.

- [ ] **Step 6: Run the tests and watch them pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_permission_tests
```

Expected: PASS (4 tests).

- [ ] **Step 7: Register state and command**

In `src-tauri/src/lib.rs`:
- Register the state alongside the app's other `.manage(...)` calls:
  ```rust
  #[cfg(target_os = "macos")]
  let app_handle_for_state = app.handle().clone();
  #[cfg(target_os = "macos")]
  app_handle_for_state.manage(commands::audio::MacosSystemAudioCaptureState(
      std::sync::Mutex::new(commands::audio::PermissionAccess::Unknown),
  ));
  ```
  Match the surrounding code's actual style for registering managed state rather than copying this shape blindly.
- Add `commands::audio::get_system_audio_availability,` to `collect_commands![...]` if Plan B did not already.

- [ ] **Step 8: Regenerate bindings and commit**

```bash
bun run tauri dev   # briefly, to regenerate src/bindings.ts
git add src-tauri/src/commands/audio.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(macos): observe and surface system audio permission state"
```

---

### Task 4: Open the privacy settings

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (`collect_commands![...]`)

**Interfaces:**
- Produces: `pub fn open_system_audio_privacy_settings() -> Result<(), String>`

- [ ] **Step 1: Add the command**

Next to the existing `open_microphone_privacy_settings` in `src-tauri/src/commands/audio.rs`:

```rust
#[tauri::command]
#[specta::specta]
pub fn open_system_audio_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // `kTCCServiceAudioCapture` is a newer bucket than Screen Recording and
        // its exact System Settings anchor is unverified (see Task 7). This URL
        // opens Privacy & Security itself, which is correct regardless; if
        // Task 7 finds a working per-pane anchor, tighten it here.
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security")
            .spawn()
            .map_err(|e| format!("Failed to open privacy settings: {e}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    Err("Opening system audio privacy settings is only supported on macOS".to_string())
}
```

- [ ] **Step 2: Register it**

Add `commands::audio::open_system_audio_privacy_settings,` to `collect_commands![...]` in `src-tauri/src/lib.rs`, next to `open_microphone_privacy_settings`.

- [ ] **Step 3: Verify and commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
bun run tauri dev   # briefly, to regenerate bindings
git add src-tauri/src/commands/audio.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(macos): add a command to open system audio privacy settings"
```

---

### Task 5: Permission-denied UI

**Files:**
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx`
- Modify: the fork's i18n catalogue (locate it in Step 2 — do not guess)

**Interfaces:**
- Consumes: `commands.getSystemAudioAvailability()`, `commands.openSystemAudioPrivacySettings()`.

No unit tests — verification is manual, in Task 7.

- [ ] **Step 1: Check the component's current state**

```bash
grep -n "availability\|useOsType" src/components/settings/advanced/SystemAudioCapture.tsx
```

If it still gates on `useOsType()`, apply Plan B Task 5 Step 2's transformation first (query `getSystemAudioAvailability` into an `availability` state, early-return `null` while `null` or `"unavailable_no_sound_server"`). This plan builds on that shape.

- [ ] **Step 2: Find the right i18n file**

```bash
grep -rn "systemAudio" src/i18n/locales/en/translation.json src/shorthand/locales/en.json src/shorthand/english-copy.json
```

Per `AGENTS.md`'s i18n rules, add the new key to whichever file already owns the `settings.advanced.systemAudio.*` keys. If they live in `src/i18n/locales/en/translation.json` (upstream's catalogue) but this key is fork-only, it belongs in `src/shorthand/locales/en.json` instead — the `check:locale-drift` gate enforces this. Add:

```json
"settings.advanced.systemAudio.permissionNeeded": "Shorthand needs permission to record audio playing on this Mac. If you declined the prompt, you can grant it in System Settings."
```

(Use the flat dotted-key form if the fork catalogue uses it; match the surrounding file's convention.)

- [ ] **Step 3: Add the denied branch**

In `SystemAudioCapture.tsx`, insert after the existing early-return and before the toggle's `return`:

```tsx
  if (availability === "permission_denied") {
    return (
      <div className="w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20">
        <p className="text-sm text-text/60 mb-3">
          {t("settings.advanced.systemAudio.permissionNeeded")}
        </p>
        <div className="flex gap-2">
          <button
            onClick={() => commands.openSystemAudioPrivacySettings()}
            className="px-4 py-2 rounded-lg bg-background-ui hover:bg-background-ui/90 text-white text-sm font-medium transition-colors"
          >
            {t("accessibility.openSettings")}
          </button>
          <button
            onClick={async () => {
              // Re-attempt capture: granting in System Settings cannot change
              // our observed state on its own, because the only way to learn
              // the new state is to try again.
              await updateSetting("system_audio_enabled", true);
              await refreshAvailability();
            }}
            className="px-4 py-2 rounded-lg border border-mid-gray/30 text-text text-sm font-medium transition-colors hover:bg-white/5"
          >
            {t("settings.advanced.systemAudio.tryAgain")}
          </button>
        </div>
      </div>
    );
  }
```

**The retry button is not optional.** This branch replaces the toggle, so without it a user who grants permission in System Settings has no way back: the observed state only changes when a capture attempt is made, and nothing here would make one. That would strand the feature permanently off after a single denial.

`accessibility.openSettings` is an existing key already used for the Windows microphone-permission button in `AccessibilityOnboarding.tsx:354` — reuse it rather than adding a second "Open Settings" string. `tryAgain` is new; add it in Step 2 alongside `permissionNeeded`:

```json
"settings.advanced.systemAudio.tryAgain": "Try again"
```

- [ ] **Step 4: Verify the gates pass**

```bash
bun run build && bun run lint && bun run check:translations && bun run check:locale-drift && bun run check:fork-translations
```

Expected: all pass. The locale-drift gate exists precisely to catch a fork-only key added to upstream's catalogue.

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "feat(macos): add grant-access CTA when system audio permission is denied"
```

---

### Task 6: Silent-tap watchdog — DEFERRED, do not implement

**Status: cut from this plan.** An earlier draft specified a background thread that rebuilt the tap after ten minutes without system-audio samples, to work around an upstream report that Core Audio Process Taps can silently degrade to all-zero buffers after long uptime. An independent review showed the design was **actively harmful**, and it is deferred rather than shipped broken.

Why it was cut, so nobody re-adds it unexamined:

- **It measured the wrong signal.** The watchdog would have timed `with_system_audio_callback`, but that callback is fed *post-VAD* and returns early while the app is not recording (`recorder.rs:1289`). An enabled-but-idle app therefore looks permanently "silent", so the watchdog would fire on healthy systems as a matter of course.
- **Firing is destructive.** Its recovery called `start_microphone_stream()`, which would defeat on-demand microphone closure entirely, and — if it fired mid-session — restart the whole mic+system stream and discard the recording in progress. `AudioRecorder` has no per-lane reopen, so there is no cheap version of this.
- **The bug is unconfirmed here.** We have not reproduced the upstream report on this codebase.

If Task 7's long-session check shows real degradation, implement it properly rather than reviving the sketch:

1. Measure at the **raw loopback callback or the pump** (`build_loopback_stream_typed`'s data callback, or `run_loopback_pump`), which sees every frame regardless of VAD and recording state — not the post-VAD consumer callback.
2. Distinguish "the tap is delivering silence" from "the tap has stopped delivering". The former is normal; only the latter is the bug.
3. Never restart while `is_recording()` is true. Defer to the next idle moment.
4. Prefer rebuilding only the system lane. If that means adding a per-lane reopen to `AudioRecorder`, that is the actual work — and its cost is a reason to be sure the bug is real first.

---

### Task 7: Manual verification matrix

No files change. None of this can run in CI.

Run on a real Mac at macOS 14.6 or later:

- [ ] **First-run consent**: with a fresh TCC state (`tccutil reset AudioCapture <bundle-id>`), enable system audio capture. Confirm the system dialog appears showing the Task 2 string **verbatim**, and that granting it starts capture.
- [ ] **The right indicator**: while capturing, confirm a **purple** menu-bar dot appears — not the orange screen-recording one. Orange means something is using ScreenCaptureKit and the design's central premise is wrong; stop and report.
- [ ] **Both speakers transcribe**: play audio from another app, record, confirm the transcript contains `me` and `them` lanes as on Windows.
- [ ] **Follow-stream**: run `handy --follow-stream` during a dual-speaker session; confirm both `"speaker":"me"` and `"speaker":"them"` events appear. No code change should have been needed.
- [ ] **Deny path**: reset TCC, decline the prompt, confirm `get_system_audio_availability` returns `permission_denied` and the Task 5 CTA renders.
- [ ] **Settings link**: click the CTA. Record **which pane actually opens** and whether the audio-capture row is reachable from there. If a more precise anchor exists, tighten `open_system_audio_privacy_settings` (Task 4) and re-verify; if not, confirm the copy is enough to guide the user.
- [ ] **Re-grant path**: grant from System Settings, return to the app, and click the CTA's **Try again** button (Task 5). Confirm capture works and availability flips back to `available`. This is the path that strands users if the retry button is missing — the toggle is not visible in the denied state, so there is no other way to trigger a fresh attempt.
- [ ] **Capture the real denial error**: with `--debug`, record the exact error text and OSStatus that `update_system_audio_capture` surfaces on a denied attempt. Add it as a comment above `observe_capture_outcome` (Task 3). If it is reliably distinguishable, tighten that function to classify only that signature as `Denied` and everything else as `Unknown`, and update its test.
- [ ] **Microphone unaffected**: confirm ordinary dictation still works with system audio both on and off.
- [ ] **Long-session check (decides Task 6)**: leave capture enabled and idle for >15 minutes with nothing playing, then play audio and record. If it still captures, the tap did not degrade and Task 6 stays deferred — write that result into Task 6 so the next person does not re-litigate it. If it captures nothing, the upstream bug is real here: report it, and implement Task 6 to the four constraints listed there rather than to the sketch that was cut.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && bun run lint`.
