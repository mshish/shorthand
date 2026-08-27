# System Audio Capture on macOS — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the existing Windows-only system-audio-loopback feature to macOS 14.6+, using cpal's native Core Audio Process Tap support, with a permission-request UX that's a single reactive "grant access" flow — no manual driver install, no Multi-Output Device configuration.

**Architecture:** Same as Windows: widen the existing `#[cfg(windows)]` gates to also cover `target_os = "macos"`. macOS's wrinkle, unlike Linux, is permission: there is no precheck API for the `kTCCServiceAudioCapture` TCC bucket cpal's Process Tap backend triggers, so the flow is reactive — attempt capture, classify a TCC-denial-shaped failure, then surface a "grant access" affordance that opens System Settings, mirroring the existing Windows microphone-permission pattern in `commands/audio.rs` and the mac permission-onboarding flow already in `App.tsx`/`AccessibilityOnboarding.tsx`.

**Tech Stack:** Rust, `cpal` 0.18.x (macOS Core Audio Process Tap backend, built in — no extra Cargo feature needed, unlike Linux), Tauri 2.x, `tauri-plugin-macos-permissions`, React/TypeScript frontend, `tauri-specta` for command bindings.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**This plan is independent of the Linux plan** (`2026-08-26-system-audio-capture-linux.md`) and may be executed before, after, or interleaved with it — several tasks below touch the same files (`commands/audio.rs`, `managers/audio.rs`) and include explicit steps to check current file state before editing, so this works regardless of ordering.

## Global Constraints

- Pin `cpal` to the same specific tested `0.18.x` version the Linux plan uses (not a loose range). If the Linux plan already bumped it, do not re-bump to a different patch — check first.
- Effective minimum OS version for this feature is **macOS 14.6**. Below that (or on any non-macOS, non-Linux, non-Windows target), report unavailable — do not attempt to open the tap.
- No entitlement change is needed — Process Tap capture is gated by the `NSAudioCaptureUsageDescription` Info.plist key alone, not by an App Sandbox device entitlement (confirmed: this app does not use App Sandbox; hardened runtime + this Info.plist key is sufficient).
- There is no public API to precheck or proactively request `kTCCServiceAudioCapture` — permission status is necessarily *observed* (from a real capture attempt's outcome), never predicted.
- Every `#[cfg(windows)]` (or, if the Linux plan already ran, `#[cfg(any(windows, target_os = "linux"))]`) touched in this plan gains `target_os = "macos"` in its `any(...)` list — check the current gate text before editing, don't assume it's still bare `#[cfg(windows)]`.
- Ship as a proper `.app` bundle (Tauri already does this by default) — a bare executable requesting this permission class reportedly can't be managed via System Settings on current macOS.

---

### Task 1: Bump cpal (idempotent with the Linux plan)

**Files:**
- Modify: `src-tauri/Cargo.toml` (the `cpal` line(s))

**Interfaces:**
- Produces: cpal ≥0.18.2 available on macOS with its built-in Core Audio Process Tap loopback support (no feature flag required, unlike Linux's `pipewire`/`pulseaudio`).

- [ ] **Step 1: Check current cpal version**

Run: `grep -n 'cpal' src-tauri/Cargo.toml`

If it already shows `cpal = "=0.18.2"` (or a newer pinned `0.18.x`) on the shared dependency line, skip to Step 3 — the Linux plan already did this. If it still shows `cpal = "0.16.0"`, proceed to Step 2.

- [ ] **Step 2: Bump the shared cpal version**

Change line 51 in `src-tauri/Cargo.toml` from `cpal = "0.16.0"` to `cpal = "=0.18.2"` (substitute the actual latest `0.18.x` patch from crates.io if newer).

- [ ] **Step 3: Verify the build**

Run: `cargo check -p shorthand` (on a macOS machine, or `cargo check -p shorthand --target aarch64-apple-darwin` cross-checking if not on macOS — full verification requires an actual Mac for later tasks anyway).

- [ ] **Step 4: Commit** (skip if Step 1 found nothing to change)

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(macos): bump cpal for Core Audio Process Tap loopback support"
```

---

### Task 2: Widen `SystemAudioCapture` to macOS in `audio_toolkit/audio/recorder.rs`

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/recorder.rs` (the `SystemAudioCapture` struct and `open()` signature)
- Modify: `src-tauri/src/audio_toolkit/audio/mod.rs` (the conditional re-export)

**Interfaces:**
- Produces: `SystemAudioCapture` and `AudioRecorder::open(...)` compile and are usable on macOS.

- [ ] **Step 1: Check current gate state**

Run: `grep -n 'cfg(any(windows\|cfg(windows)' src-tauri/src/audio_toolkit/audio/recorder.rs src-tauri/src/audio_toolkit/audio/mod.rs`

- [ ] **Step 2: Add macOS to each gate found**

For each match:
- If it reads `#[cfg(windows)]`, change to `#[cfg(any(windows, target_os = "macos"))]`.
- If it reads `#[cfg(any(windows, target_os = "linux"))]` (the Linux plan already ran), change to `#[cfg(any(windows, target_os = "linux", target_os = "macos"))]`.

This applies to: the `SystemAudioCapture` struct definition, the `open()` method's `system_audio` parameter, and the re-export in `mod.rs`. Update the doc comment above the struct too if it still says "A Windows render endpoint" — it should already read the platform-neutral wording from the Linux plan's Task 2 if that ran first; if not, update it:

```rust
/// An output-side render endpoint to capture via platform loopback: WASAPI
/// on Windows, a Core Audio Process Tap on macOS, or PipeWire/PulseAudio's
/// sink-monitor capture on Linux — all reached the same way, by opening this
/// device (normally output-only) as an input stream.
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p shorthand`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/audio_toolkit/audio/recorder.rs src-tauri/src/audio_toolkit/audio/mod.rs
git commit -m "feat(macos): widen SystemAudioCapture to macOS"
```

---

### Task 3: Widen the dual-lane wiring in `managers/audio.rs` and `managers/transcription.rs`

**Files:**
- Modify: `src-tauri/src/managers/audio.rs` (same sites the Linux plan's Task 3 touched: the top-level `use`, `PendingSystemAudioCapture`, `create_audio_recorder`'s system-VAD block, `AudioRecordingManager`'s struct fields and constructor, `get_effective_system_audio_device`, the call site in `start_microphone_stream`, `update_system_audio_capture`, `set_system_stream_router`)
- Modify: `src-tauri/src/managers/transcription.rs` (the `StreamSource::System`/`SystemAudioTranscription` gates)
- Modify: `src-tauri/src/commands/audio.rs` (the `use` importing `StreamSource, SystemAudioTranscription, TranscriptionManager`)
- Modify: `src-tauri/src/lib.rs` (the `SystemAudioTranscription` state registration)

**Interfaces:**
- Produces: `AudioRecordingManager::update_system_audio_capture(...)` and `SystemAudioTranscription` now work on macOS.

- [ ] **Step 1: Find every remaining Windows/Linux-only gate on system-audio code**

Run:

```bash
grep -n 'cfg(windows)\|cfg(any(windows' src-tauri/src/managers/audio.rs src-tauri/src/managers/transcription.rs src-tauri/src/commands/audio.rs src-tauri/src/lib.rs
```

- [ ] **Step 2: Add `target_os = "macos"` to each gate that touches system audio**

Same rule as Task 2 Step 2: `#[cfg(windows)]` → `#[cfg(any(windows, target_os = "macos"))]`; `#[cfg(any(windows, target_os = "linux"))]` → `#[cfg(any(windows, target_os = "linux", target_os = "macos"))]`. Apply this to every site listed under Files above. Do not touch gates unrelated to system audio (e.g. any purely-Windows registry code, or purely-Linux `wpctl`/`pactl` mute helpers) — only the ones gating `SystemAudioCapture`, `PendingSystemAudioCapture`, `system_stream_router`, `pending_system_audio`, `get_effective_system_audio_device`, `update_system_audio_capture`, `set_system_stream_router`, `StreamSource::System`, and `SystemAudioTranscription`.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p shorthand`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/managers/audio.rs src-tauri/src/managers/transcription.rs src-tauri/src/commands/audio.rs src-tauri/src/lib.rs
git commit -m "feat(macos): widen system-audio dual-lane wiring to macOS"
```

---

### Task 4: macOS availability (OS version gate)

**Files:**
- Modify: `src-tauri/src/commands/audio.rs` (extend `get_system_audio_availability`, or define `SystemAudioAvailability` + the command from scratch if the Linux plan hasn't run yet)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Consumes: nothing new for the version check itself.
- Produces: `get_system_audio_availability()` returns `SystemAudioAvailability::UnavailableOsVersion` on macOS < 14.6, `Available` on macOS ≥ 14.6 (permission state is handled separately in Task 6 — this task is purely the OS-version floor).

- [ ] **Step 1: Check whether `SystemAudioAvailability` already exists**

Run: `grep -n 'enum SystemAudioAvailability' src-tauri/src/commands/audio.rs`

If found (the Linux plan ran first), skip to Step 3 and only add the macOS branch inside the existing `get_system_audio_availability` function body. If not found, do Step 2 first to define the full enum (identical to what's specified in the Linux plan's Task 5, Step 3) before proceeding.

- [ ] **Step 2 (only if the enum doesn't exist yet): define it**

Add, near the existing `PermissionAccess` enum:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioAvailability {
    Available,
    UnavailableOsVersion,
    UnavailableNoSoundServer,
    PermissionDenied,
}
```

- [ ] **Step 3: Write the failing test for the macOS version-floor decision**

Add to (or create) the `#[cfg(test)] mod system_audio_availability_tests` block in `src-tauri/src/commands/audio.rs`:

```rust
#[test]
fn macos_below_14_6_is_unavailable() {
    assert_eq!(
        macos_availability_from_version((14, 5)),
        SystemAudioAvailability::UnavailableOsVersion
    );
}

#[test]
fn macos_at_14_6_is_available_pending_permission_check() {
    assert_eq!(
        macos_availability_from_version((14, 6)),
        SystemAudioAvailability::Available
    );
}

#[test]
fn macos_above_14_6_is_available_pending_permission_check() {
    assert_eq!(
        macos_availability_from_version((15, 0)),
        SystemAudioAvailability::Available
    );
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cargo test -p shorthand system_audio_availability_tests`

Expected: FAIL — `macos_availability_from_version` not found.

- [ ] **Step 5: Write the pure version-decision function and wire it in**

```rust
/// Pure decision logic for the macOS version floor, split out from the real
/// OS-version query so it's unit-testable. (14, 6) means macOS 14.6.
fn macos_availability_from_version(version: (u32, u32)) -> SystemAudioAvailability {
    if version >= (14, 6) {
        SystemAudioAvailability::Available
    } else {
        SystemAudioAvailability::UnavailableOsVersion
    }
}

#[cfg(target_os = "macos")]
fn macos_os_version() -> (u32, u32) {
    // NSProcessInfo's operatingSystemVersion is the standard way to read this;
    // objc2-foundation is already a dependency (see Cargo.toml's macOS target
    // section) via the paste_tx module's use of AppKit/Foundation bindings.
    use objc2_foundation::NSProcessInfo;
    let info = NSProcessInfo::processInfo();
    let version = info.operatingSystemVersion();
    (version.majorVersion as u32, version.minorVersion as u32)
}
```

Then, in `get_system_audio_availability`'s body, add the macOS arm (using whichever cfg style already exists there — extend the `#[cfg(windows)] { ... Available }` / `#[cfg(target_os = "linux")]` pattern from the Linux plan, or write both from scratch if this is the first plan to touch the function):

```rust
    #[cfg(target_os = "macos")]
    {
        macos_availability_from_version(macos_os_version())
    }
```

Place it as a sibling arm alongside the existing `#[cfg(target_os = "linux")]` and `#[cfg(windows)]` blocks, and narrow the catch-all `#[cfg(not(any(...)))]` arm's condition to also exclude `target_os = "macos"`.

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p shorthand system_audio_availability_tests`

Expected: PASS.

- [ ] **Step 7: Verify the real OS-version reader compiles on macOS**

Run: `cargo check -p shorthand` on a Mac (the `macos_os_version` function itself isn't unit-tested — it calls a real system API — so this compile check plus Task 11's manual verification is its coverage).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/audio.rs
git commit -m "feat(macos): gate system audio availability on macOS 14.6+"
```

---

### Task 5: Info.plist usage-description string

**Files:**
- Modify: `src-tauri/Info.plist`

**Interfaces:** none — this is the exact copy macOS shows the user in the system consent dialog, so its wording *is* the permission UX.

- [ ] **Step 1: Add the key**

Change `src-tauri/Info.plist` from:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSMicrophoneUsageDescription</key>
  <string>Request microphone access to transcribe audio locally</string>
</dict>
</plist>
```

to:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSMicrophoneUsageDescription</key>
  <string>Request microphone access to transcribe audio locally</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>Request system audio access to transcribe played audio locally, alongside your microphone</string>
</dict>
</plist>
```

- [ ] **Step 2: Verify it's picked up in a built bundle**

Run: `bun run tauri build --no-bundle` then check the built `.app`'s `Contents/Info.plist` (e.g. `plutil -p src-tauri/target/release/bundle/macos/Shorthand.app/Contents/Info.plist | grep -A1 AudioCapture`) contains the new key.

Expected: key and string present verbatim.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Info.plist
git commit -m "feat(macos): add NSAudioCaptureUsageDescription for system audio capture"
```

---

### Task 6: Permission status command and privacy-settings deep link

**Files:**
- Modify: `src-tauri/src/commands/audio.rs` (new `MacosSystemAudioPermissionStatus` state, `get_macos_system_audio_permission_status`, `open_system_audio_privacy_settings`, and error classification in the existing `change_system_audio_enabled_setting`)
- Modify: `src-tauri/src/lib.rs` (register the two new commands)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Produces: `pub fn get_macos_system_audio_permission_status(app: AppHandle) -> PermissionAccess` (reusing the existing `PermissionAccess { Allowed, Denied, Unknown }` enum already defined for Windows mic permission — same three-state shape fits here) and `pub fn open_system_audio_privacy_settings() -> Result<(), String>`.
- Consumes: `AudioRecordingManager` (to observe the outcome of the most recent capture attempt).

Since there's no precheck API, permission state must be *observed*, not queried. This task stores the last-observed outcome as app-managed state, updated whenever `change_system_audio_enabled_setting` (or the tap's own health-check reopen from Task 9) attempts to open the device.

- [ ] **Step 1: Write the failing test for TCC-denial error classification**

The real `cpal::BuildStreamError`/`cpal::StreamError` produced when a tap open fails due to TCC denial is a real macOS-only value that can't be constructed in a portable unit test. So, as with the availability checks, split this into a pure classifier over an already-extracted string, and test the classifier:

```rust
#[cfg(test)]
mod macos_system_audio_permission_tests {
    use super::*;

    #[test]
    fn classifies_known_tcc_denial_message_as_denied() {
        // cpal surfaces Core Audio's kAudioHardwareUnauthorizedError as this
        // substring in its Display/Debug output as of cpal 0.18.x — verify
        // this exact substring against a real denial on a real Mac in Task
        // 11's manual verification, and update this test if cpal's wording
        // has changed since.
        assert_eq!(
            classify_macos_open_error("AudioUnitErr(-66748)"),
            PermissionAccess::Denied
        );
    }

    #[test]
    fn classifies_unrelated_error_as_unknown_not_denied() {
        assert_eq!(
            classify_macos_open_error("device disconnected"),
            PermissionAccess::Unknown
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shorthand macos_system_audio_permission_tests`

Expected: FAIL — `classify_macos_open_error` not found.

- [ ] **Step 3: Write the classifier and the state**

```rust
/// Core Audio's kAudioHardwareUnauthorizedError is -66748. cpal's macOS host
/// surfaces build/stream errors from Core Audio via their OSStatus code
/// embedded in the error's string representation — match on that code rather
/// than a full string, since surrounding wording can change across cpal
/// versions. Verify this against a real denial on a real Mac (Task 11)
/// before shipping; update the code if cpal's error surface has changed.
fn classify_macos_open_error(error_display: &str) -> PermissionAccess {
    if error_display.contains("-66748") {
        PermissionAccess::Denied
    } else {
        PermissionAccess::Unknown
    }
}

/// The last-observed outcome of a macOS system-audio capture attempt.
/// There is no precheck API for this permission bucket, so this is the only
/// source of truth the app has — it reflects what happened last time we
/// tried, not a live OS query.
pub struct MacosSystemAudioPermissionState(pub Mutex<PermissionAccess>);

#[tauri::command]
#[specta::specta]
pub fn get_macos_system_audio_permission_status(
    app: AppHandle,
) -> PermissionAccess {
    #[cfg(target_os = "macos")]
    {
        *app.state::<MacosSystemAudioPermissionState>().0.lock().unwrap()
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        PermissionAccess::Unknown
    }
}

#[tauri::command]
#[specta::specta]
pub fn open_system_audio_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // The exact pane for kTCCServiceAudioCapture was not confirmed by
        // research at plan-writing time (it's a newer, separate bucket from
        // Screen Recording). Verify on a real macOS 14.6+ machine during
        // Task 11 which pane opens the right row; this URL opens the
        // Privacy & Security root, which is always correct as a fallback
        // even if a more specific deep link exists.
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security")
            .spawn()
            .map_err(|e| format!("Failed to open privacy settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    Err("Opening system audio privacy settings is only supported on macOS".to_string())
}
```

- [ ] **Step 4: Wire classification into the existing capture-attempt path**

In `change_system_audio_enabled_setting`'s macOS/shared body (from Task 3), after the call to `update_system_audio_capture` returns, add an update to `MacosSystemAudioPermissionState`:

```rust
    #[cfg(target_os = "macos")]
    let capture_result = manager_for_update.update_system_audio_capture(enabled, device_name, stream_router);
    #[cfg(not(target_os = "macos"))]
    let capture_result = manager_for_update.update_system_audio_capture(enabled, device_name, stream_router);

    #[cfg(target_os = "macos")]
    {
        let observed = match &capture_result {
            Ok(()) => PermissionAccess::Allowed,
            Err(e) => classify_macos_open_error(&e.to_string()),
        };
        // Only overwrite on a definitive signal; an "Unknown" result from an
        // unrelated error (e.g. device unplugged) shouldn't erase a
        // previously-observed Allowed/Denied state.
        if observed != PermissionAccess::Unknown {
            *app.state::<MacosSystemAudioPermissionState>().0.lock().unwrap() = observed;
        }
    }

    capture_result
        .map_err(|error| format!("Failed to update system audio capture: {error}"))?;
```

(Adjust the exact splice point to match the real surrounding code from Task 3 — the two `#[cfg]` branches shown for `capture_result` collapse to one once you're editing the real file; they're written separately here only to show that macOS needs the extra observation step immediately after the same call every other platform already makes.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p shorthand macos_system_audio_permission_tests`

Expected: PASS.

- [ ] **Step 6: Register the new commands and state**

In `src-tauri/src/lib.rs`:
- Add `.manage(MacosSystemAudioPermissionState(Mutex::new(PermissionAccess::Unknown)))` alongside the app's other `.manage(...)` calls (gate with `#[cfg(target_os = "macos")]` if the builder chain supports conditional `.manage()`; otherwise register unconditionally since the struct itself is cheap and the command already no-ops off-macOS).
- Add `commands::audio::get_macos_system_audio_permission_status,` and `commands::audio::open_system_audio_privacy_settings,` to the `collect_commands![...]` list.

- [ ] **Step 7: Regenerate frontend bindings**

Run: `bun run tauri dev` briefly, confirm `src/bindings.ts` now has `getMacosSystemAudioPermissionStatus` and `openSystemAudioPrivacySettings`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/audio.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(macos): observe and surface system audio permission state"
```

---

### Task 7: Extend `get_system_audio_availability` with the permission state

**Files:**
- Modify: `src-tauri/src/commands/audio.rs` (`get_system_audio_availability`'s macOS arm from Task 4)

**Interfaces:**
- Produces: `get_system_audio_availability` on macOS now returns `PermissionDenied` if a prior attempt was denied, in addition to the OS-version check from Task 4.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn macos_reports_permission_denied_over_version_check_when_both_could_apply() {
    assert_eq!(
        macos_availability(macos_availability_from_version((15, 0)), PermissionAccess::Denied),
        SystemAudioAvailability::PermissionDenied
    );
}

#[test]
fn macos_reports_os_version_unavailable_even_if_permission_unknown() {
    assert_eq!(
        macos_availability(macos_availability_from_version((14, 0)), PermissionAccess::Unknown),
        SystemAudioAvailability::UnavailableOsVersion
    );
}

#[test]
fn macos_reports_available_when_version_ok_and_not_denied() {
    assert_eq!(
        macos_availability(macos_availability_from_version((15, 0)), PermissionAccess::Unknown),
        SystemAudioAvailability::Available
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p shorthand macos_availability`

Expected: FAIL — `macos_availability` not found.

- [ ] **Step 3: Implement and wire in**

```rust
/// Combines the OS-version floor with the last-observed permission state.
/// Version check wins when the OS is simply too old (permission is moot);
/// otherwise a known denial takes priority over reporting bare availability.
fn macos_availability(
    version_availability: SystemAudioAvailability,
    permission: PermissionAccess,
) -> SystemAudioAvailability {
    if version_availability != SystemAudioAvailability::Available {
        return version_availability;
    }
    if permission == PermissionAccess::Denied {
        SystemAudioAvailability::PermissionDenied
    } else {
        SystemAudioAvailability::Available
    }
}
```

Update `get_system_audio_availability`'s macOS arm (from Task 4 Step 5) to:

```rust
    #[cfg(target_os = "macos")]
    {
        macos_availability(
            macos_availability_from_version(macos_os_version()),
            *app.state::<MacosSystemAudioPermissionState>().0.lock().unwrap(),
        )
    }
```

This requires `get_system_audio_availability` to take an `app: AppHandle` parameter if it doesn't already (the Linux plan's version doesn't need one) — add it, and update the Linux/Windows arms to simply ignore it (`let _ = &app;` where unused) rather than special-casing the signature per platform.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p shorthand macos_availability`

- [ ] **Step 5: Regenerate bindings and commit**

Note: adding an `app: AppHandle` parameter does not change the generated TypeScript call signature — Tauri auto-injects `AppHandle` server-side, so `commands.getSystemAudioAvailability()` in the frontend stays argument-free. Regenerate bindings anyway as a matter of course after any command signature edit:

```bash
bun run tauri dev   # briefly, to confirm src/bindings.ts is unchanged/still correct
git add src-tauri/src/commands/audio.rs
git commit -m "feat(macos): fold permission state into system audio availability"
```

---

### Task 8: Silent-tap health check

**Files:**
- Modify: `src-tauri/src/managers/audio.rs` (add a watchdog around the system-audio lane)
- Test: `src-tauri/src/managers/audio.rs` (inline `#[cfg(test)]` module — new)

**Interfaces:**
- Consumes: the system-audio `StreamRouter`'s frame feed (already wired in `create_audio_recorder`'s `with_system_audio_callback`).
- Produces: a pure decision function `should_rebuild_system_tap(enabled: bool, elapsed_since_last_sample: Duration, threshold: Duration) -> bool`, plus real wiring that closes and reopens just the system-audio device when it fires.

This addresses the known upstream Core Audio Process Tap bug where a tap can silently degrade to all-zero buffers after extended uptime. The decision logic is pure and testable; the actual "was audio really silent or did nothing play" distinction is inherently fuzzy (we can't tell "system was quiet" from "tap died" from sample data alone) — so this task implements a conservative, opt-in-feeling safeguard rather than an aggressive one: it only fires while system audio capture is enabled AND the feature has been continuously open for a long time, and it's a cheap no-op (device reopen) if triggered spuriously.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod system_audio_health_check_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn does_not_rebuild_when_disabled() {
        assert!(!should_rebuild_system_tap(
            false,
            Duration::from_secs(999_999),
            Duration::from_secs(600)
        ));
    }

    #[test]
    fn does_not_rebuild_before_threshold() {
        assert!(!should_rebuild_system_tap(
            true,
            Duration::from_secs(599),
            Duration::from_secs(600)
        ));
    }

    #[test]
    fn rebuilds_after_threshold_while_enabled() {
        assert!(should_rebuild_system_tap(
            true,
            Duration::from_secs(601),
            Duration::from_secs(600)
        ));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p shorthand system_audio_health_check_tests`

Expected: FAIL — `should_rebuild_system_tap` not found.

- [ ] **Step 3: Implement the pure function**

Add near the top of `managers/audio.rs`, alongside the other small pure helpers:

```rust
/// Whether the system-audio tap should be torn down and reopened, given how
/// long it's been since the last sample arrived on that lane. Guards against
/// a known macOS Core Audio Process Tap bug where the tap can silently
/// degrade to all-zero buffers after extended uptime — a real "nothing is
/// playing" silence is indistinguishable from this from sample data alone,
/// so the threshold is intentionally long (minutes, not seconds) to avoid
/// rebuilding a perfectly healthy tap just because the user's audio is quiet.
#[cfg(target_os = "macos")]
fn should_rebuild_system_tap(
    system_audio_enabled: bool,
    elapsed_since_last_sample: Duration,
    threshold: Duration,
) -> bool {
    system_audio_enabled && elapsed_since_last_sample > threshold
}

#[cfg(target_os = "macos")]
const SYSTEM_TAP_SILENCE_REBUILD_THRESHOLD: Duration = Duration::from_secs(600);
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p shorthand system_audio_health_check_tests`

Expected: PASS.

- [ ] **Step 5: Wire in real tracking and the watchdog**

Add a `last_system_sample_at: Arc<Mutex<Instant>>` field to `AudioRecordingManager`, initialized to `Instant::now()` in `new()`.

`create_audio_recorder` is a free function, not a method, so give it a new parameter and thread it through. Change its signature (from the version Task 3 widened) from:

```rust
fn create_audio_recorder(
    vad_path: &Path,
    app_handle: &tauri::AppHandle,
    selected_channel: Option<u16>,
    stream_router: Arc<StreamRouter>,
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))] system_stream_router: Option<Arc<StreamRouter>>,
) -> Result<AudioRecorder, anyhow::Error> {
```

to add one more parameter:

```rust
fn create_audio_recorder(
    vad_path: &Path,
    app_handle: &tauri::AppHandle,
    selected_channel: Option<u16>,
    stream_router: Arc<StreamRouter>,
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))] system_stream_router: Option<Arc<StreamRouter>>,
    #[cfg(target_os = "macos")] last_system_sample_at: Arc<Mutex<Instant>>,
) -> Result<AudioRecorder, anyhow::Error> {
```

Then change the existing match arm's callback (the `router` binding there is the destructured `Some(router)` system-audio `StreamRouter`, unchanged from what Task 3 widened — only the closure body gains a line):

```rust
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    let recorder = match (recorder, system_smoothed_vad, system_stream_router) {
        (recorder, Some(system_vad), Some(router)) => {
            #[cfg(target_os = "macos")]
            let last_sample = Arc::clone(&last_system_sample_at);
            recorder
                .with_system_vad(
                    Box::new(system_vad),
                    VAD_OFFLINE_HANGOVER_FRAMES,
                    VAD_STREAMING_HANGOVER_FRAMES,
                )
                .with_system_audio_callback(move |frame| {
                    #[cfg(target_os = "macos")]
                    {
                        *last_sample.lock().unwrap() = Instant::now();
                    }
                    router.feed(frame);
                })
        }
        (recorder, _, _) => recorder,
    };
```

Finally, update both call sites of `create_audio_recorder` (in `preload_vad` and anywhere else it's invoked in `managers/audio.rs`) to pass `#[cfg(target_os = "macos")] self.last_system_sample_at.clone()` as the new trailing argument, matching how they already pass `self.system_stream_router.lock().unwrap().clone()`.

Then, only on macOS, spawn a watchdog thread from `start_microphone_stream` (or `AudioRecordingManager::new`, run once) that polls every 30 seconds:

```rust
#[cfg(target_os = "macos")]
fn spawn_system_tap_watchdog(manager: Arc<AudioRecordingManager>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let settings = get_settings(&manager.app_handle);
        let elapsed = manager.last_system_sample_at.lock().unwrap().elapsed();
        if should_rebuild_system_tap(
            settings.system_audio_enabled,
            elapsed,
            SYSTEM_TAP_SILENCE_REBUILD_THRESHOLD,
        ) {
            warn!(
                "System audio tap silent for {:?}; rebuilding (known Core Audio Process Tap issue)",
                elapsed
            );
            manager.stop_microphone_stream();
            if let Err(e) = manager.start_microphone_stream() {
                error!("Failed to rebuild system audio tap: {e}");
            }
        }
    });
}
```

Call `spawn_system_tap_watchdog(Arc::clone(&manager))` once, from wherever `AudioRecordingManager` is constructed and wrapped in an `Arc` in `lib.rs`'s setup (gate the call site with `#[cfg(target_os = "macos")]`).

Note this rebuilds the *whole* mic+system stream (there's no per-lane reopen in the current `AudioRecorder` API) — acceptable since it only fires after 10 minutes of suspected silence, but call this out in the manual verification task below since it will audibly interrupt an in-progress recording if it fires mid-session.

- [ ] **Step 6: Verify it compiles**

Run: `cargo check -p shorthand` on macOS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/managers/audio.rs src-tauri/src/lib.rs
git commit -m "feat(macos): rebuild system audio tap after prolonged silence"
```

---

### Task 9: Frontend permission-denied CTA

**Files:**
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx` (extend the availability handling from the Linux plan's Task 7 with a macOS-specific denied state)
- Test: `src/components/settings/advanced/SystemAudioCapture.test.tsx` (extend, or create if the Linux plan hasn't run yet — see its Task 7 for the file-discovery step)

**Interfaces:**
- Consumes: `commands.getSystemAudioAvailability()`, `commands.openSystemAudioPrivacySettings()` (Task 7, Task 6).

- [ ] **Step 1: Check current file state**

Run: `grep -n 'unavailable_no_sound_server\|PermissionDenied\|permission_denied' src/components/settings/advanced/SystemAudioCapture.tsx`

If the Linux plan's availability-gating change (its Task 7) hasn't landed yet, do that transformation first (see that plan's Task 7, Steps 4) before proceeding — this task assumes the component already queries `getSystemAudioAvailability` instead of `useOsType`.

- [ ] **Step 2: Write the failing test**

Following the same mocking pattern established in the Linux plan's Task 7 test, add a case: when `getSystemAudioAvailability` resolves to `"permission_denied"`, the component renders a "Grant access" button (not the toggle), and clicking it calls `commands.openSystemAudioPrivacySettings`.

- [ ] **Step 3: Run to verify it fails**

Run the test command from `docs/FRONTEND_TESTING.md`.

Expected: FAIL.

- [ ] **Step 4: Add the denied-state branch**

Extend the component (building on the Linux plan's Task 7 result):

```tsx
  if (availability === "permission_denied") {
    return (
      <div className={grouped ? undefined : "w-full p-4 rounded-lg bg-white/5 border border-mid-gray/20"}>
        <p className="text-sm text-text/60 mb-3">
          {t("settings.advanced.systemAudio.permissionDenied")}
        </p>
        <button
          onClick={() => commands.openSystemAudioPrivacySettings()}
          className="px-4 py-2 rounded-lg bg-background-ui hover:bg-background-ui/90 text-white text-sm font-medium transition-colors"
        >
          {t("accessibility.openSettings")}
        </button>
      </div>
    );
  }
```

Insert this branch after the existing `if (availability === null || ...) return null;` early-return and before the toggle's `return (...)`.

Add the new translation key alongside the existing `settings.advanced.systemAudio.*` keys — first check which file those live in (`grep -rn 'systemAudio.label' src/i18n src/shorthand`) and add `permissionDenied` to that same file, per `AGENTS.md`'s i18n rules (don't guess which file — fork-only vs. upstream-shared).

- [ ] **Step 5: Run to verify it passes**

Run the same test command.

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/settings/advanced/SystemAudioCapture.tsx src/components/settings/advanced/SystemAudioCapture.test.tsx
git commit -m "feat(macos): add grant-access CTA for denied system audio permission"
```

---

### Task 10: Manual verification matrix

No files change in this task.

- [ ] **Verify the consent prompt on a clean macOS 14.6+ install**: enable system audio capture for the first time; confirm the system dialog shows the exact `NSAudioCaptureUsageDescription` string from Task 5, and that a small purple menu-bar indicator appears while capturing (not the orange Screen Recording one).
- [ ] **Verify the deny path**: deny the prompt, confirm `get_system_audio_availability` returns `permission_denied` and the "Grant access" CTA (Task 9) appears; click it and confirm it opens System Settings; note in this checklist which exact pane/row actually appears (per Task 6 Step 3's open question) and file a follow-up if `x-apple.systempreferences:com.apple.preference.security` doesn't land on the right row — update `open_system_audio_privacy_settings` with a more specific deep link if one is found.
- [ ] **Verify the re-grant path**: from System Settings, grant the permission, return to Shorthand, re-enable the toggle, confirm capture now works (this exercises the "attempt again, observe Allowed" path from Task 6).
- [ ] **Verify macOS < 14.6 reports unavailable**: on (or via a version-string override for testing) an older macOS, confirm the toggle doesn't render and no crash occurs.
- [ ] **Verify `error_display` string classification (Task 6 Step 1's test)** against a real denial: capture the actual error string cpal 0.18.x's `open()` surfaces on a real TCC denial on a real Mac, and confirm it contains `-66748`; if cpal's wording differs from what was assumed, update `classify_macos_open_error` and its test accordingly.
- [ ] **Verify `cargo clippy` and `cargo fmt --check` are clean**: `cargo fmt --check && cargo clippy -p shorthand`.
- [ ] **Optional, if feasible to induce**: leave system audio capture enabled and idle for >10 minutes with no audio playing, confirm the watchdog (Task 8) doesn't spuriously fire on legitimate silence, and separately confirm (from logs) it does fire and successfully rebuilds if a tap can be made to go silent-while-active in a test environment.
