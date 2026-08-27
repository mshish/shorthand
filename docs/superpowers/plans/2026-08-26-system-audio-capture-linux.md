# System Audio Capture on Linux — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the existing Windows-only system-audio-loopback feature (mic + system audio captured simultaneously, each with its own VAD lane, merged into `RecordedAudio { microphone, system }`) to Linux, with zero permission-UX work (none is needed) and graceful unavailability when no PipeWire/PulseAudio server is present.

**Architecture:** No new architecture — widen the existing `#[cfg(windows)]` gates in `audio_toolkit/audio/recorder.rs`, `managers/audio.rs`, `managers/transcription.rs`, and `commands/audio.rs` to also cover `target_os = "linux"`. Enable cpal's `pipewire` and `pulseaudio` Cargo features (Linux-only); cpal internally prioritizes PipeWire > PulseAudio > ALSA and exposes each sink's monitor as an ordinary capturable input device, so the capture call site is unchanged from Windows' `build_input_stream`-on-an-output-`Device` pattern.

**Tech Stack:** Rust, `cpal` 0.18.x (`pipewire` + `pulseaudio` features), Tauri 2.x, React/TypeScript frontend, `tauri-specta` for command bindings.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

## Global Constraints

- Pin `cpal` to a specific tested `0.18.x` version (not a loose `^0.18` range) — the PipeWire/PulseAudio backend is young and still receiving near-weekly fixes.
- Enable `pipewire` and `pulseaudio` cpal features **only** on `target_os = "linux"` — never as default features, never on other platforms.
- No permission prompt, no settings deep link, no consent UI on Linux. The only new user-facing state is "available" vs. "unavailable."
- Systems with neither PipeWire nor PulseAudio running: report unavailable. Do not attempt an ALSA `snd-aloop` fallback.
- Every `#[cfg(windows)]` touched in this plan becomes `#[cfg(any(windows, target_os = "linux"))]` — do not also add `target_os = "macos"` (that's a separate, independent plan).
- `AGENTS.md`'s "keep the diff mergeable" rule applies: this plan is additive to existing files at existing `cfg` boundaries, not a rewrite.

---

### Task 1: Bump cpal and enable Linux backend features

**Files:**
- Modify: `src-tauri/Cargo.toml:51` (the shared `cpal = "0.16.0"` line)
- Modify: `src-tauri/Cargo.toml:164-170` (the `[target.'cfg(target_os = "linux")'.dependencies]` section)

**Interfaces:**
- Produces: `cpal::HostId::PipeWire` and `cpal::HostId::PulseAudio` become available at compile time on Linux (gated behind the Cargo features enabled here), for use by `host_from_id` in Task 5.

Before writing code: confirm the latest stable `0.18.x` cpal release on crates.io (research at spec-writing time found 0.18.2, released 2026-08-16; a newer patch may exist by the time this task runs — pin whatever is latest `0.18.x`, do not jump to an unreleased `0.19`).

- [ ] **Step 1: Bump the shared cpal version**

Change line 51 in `src-tauri/Cargo.toml` from:

```toml
cpal = "0.16.0"
```

to:

```toml
cpal = "=0.18.2"
```

(Use `=` to pin exactly, per the Global Constraints — check crates.io first and substitute the actual latest `0.18.x` patch if newer than 0.18.2.)

- [ ] **Step 2: Add Linux-only cpal features**

In `src-tauri/Cargo.toml`, find the existing Linux target section:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
gtk-layer-shell = { version = "0.8", features = ["v0_6"] }
gtk = "0.18"
transcribe-cpp = { version = "0.1.3", default-features = false, features = [
  "dynamic-backends",
  "vulkan",
] }
```

Add a `cpal` line enabling both features, keyed to the same pinned version as Step 1:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
cpal = { version = "=0.18.2", features = ["pipewire", "pulseaudio"] }
gtk-layer-shell = { version = "0.8", features = ["v0_6"] }
gtk = "0.18"
transcribe-cpp = { version = "0.1.3", default-features = false, features = [
  "dynamic-backends",
  "vulkan",
] }
```

Cargo merges feature requests across the base and target-specific `cpal` entries, so this correctly adds the features only for Linux builds while other platforms keep the plain `cpal` dependency from Step 1.

- [ ] **Step 3: Verify the build picks up the new backend**

Run: `cargo check -p shorthand --target x86_64-unknown-linux-gnu` (or just `cargo check` if already on a Linux dev machine)

Expected: compiles cleanly. If it fails with a missing system library error (`libpipewire-0.3` or `libpulse` not found), that's expected until Task 8 installs the dev packages — note it and continue; Task 8 covers the prerequisite.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(linux): enable cpal pipewire and pulseaudio backends"
```

---

### Task 2: Widen `SystemAudioCapture` to Linux in `audio_toolkit/audio/recorder.rs`

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/recorder.rs:66-71` (the `SystemAudioCapture` struct)
- Modify: `src-tauri/src/audio_toolkit/audio/recorder.rs:249-252` (the `open()` signature's `system_audio` parameter)
- Modify: `src-tauri/src/audio_toolkit/audio/mod.rs:9-10` (the conditional re-export)

**Interfaces:**
- Consumes: nothing new — `cpal::Device` (already imported).
- Produces: `SystemAudioCapture { device: cpal::Device }` and `AudioRecorder::open(&mut self, device: Option<Device>, system_audio: Option<SystemAudioCapture>)` now compile and are usable on Linux, unchanged in shape from the Windows version — this is what Task 3's `managers/audio.rs` changes call.

- [ ] **Step 1: Widen the struct gate**

In `src-tauri/src/audio_toolkit/audio/recorder.rs`, change:

```rust
/// A Windows render endpoint to capture through WASAPI loopback.
#[cfg(windows)]
#[derive(Clone)]
pub struct SystemAudioCapture {
    pub device: Device,
}
```

to:

```rust
/// An output-side render endpoint to capture via platform loopback: WASAPI
/// on Windows, a Core Audio Process Tap on macOS, or PipeWire/PulseAudio's
/// sink-monitor capture on Linux — all reached the same way, by opening this
/// device (normally output-only) as an input stream.
#[cfg(any(windows, target_os = "linux"))]
#[derive(Clone)]
pub struct SystemAudioCapture {
    pub device: Device,
}
```

(Leave `target_os = "macos"` out of this `cfg` — that platform is a separate plan. When that plan lands, it will extend this same `cfg(any(...))` list.)

- [ ] **Step 2: Widen the `open()` parameter gate**

Change:

```rust
    pub fn open(
        &mut self,
        device: Option<Device>,
        #[cfg(windows)] system_audio: Option<SystemAudioCapture>,
    ) -> Result<(), Box<dyn std::error::Error>> {
```

to:

```rust
    pub fn open(
        &mut self,
        device: Option<Device>,
        #[cfg(any(windows, target_os = "linux"))] system_audio: Option<SystemAudioCapture>,
    ) -> Result<(), Box<dyn std::error::Error>> {
```

- [ ] **Step 3: Widen the re-export gate**

In `src-tauri/src/audio_toolkit/audio/mod.rs`, change:

```rust
#[cfg(windows)]
pub use recorder::SystemAudioCapture;
```

to:

```rust
#[cfg(any(windows, target_os = "linux"))]
pub use recorder::SystemAudioCapture;
```

- [ ] **Step 4: Verify it still compiles on the current platform**

Run: `cargo check -p shorthand`

Expected: compiles cleanly (Windows is unaffected since it's still in the `any(...)`; Linux now includes the type but nothing calls it yet, so no behavior change).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio_toolkit/audio/recorder.rs src-tauri/src/audio_toolkit/audio/mod.rs
git commit -m "feat(linux): widen SystemAudioCapture to Linux"
```

---

### Task 3: Widen the dual-lane wiring in `managers/audio.rs`

**Files:**
- Modify: `src-tauri/src/managers/audio.rs` (multiple `#[cfg(windows)]` sites: the top-level `use` at line 2, `PendingSystemAudioCapture` at line 27-32, the `create_audio_recorder` system-VAD block at lines 278/291-306/332-342, the `AudioRecordingManager` struct fields at lines 381-382/400-401, `get_effective_system_audio_device` at line 514, its call sites at lines 705-721, `update_system_audio_capture` at line 933, `set_system_stream_router` at line 968)

**Interfaces:**
- Consumes: `SystemAudioCapture`, `list_output_devices` (from Task 2's widened `audio_toolkit` re-export), `StreamRouter` (unchanged, from `managers::transcription`).
- Produces: `AudioRecordingManager::update_system_audio_capture(&self, enabled: bool, device_name: Option<String>, stream_router: Option<Arc<StreamRouter>>) -> Result<(), anyhow::Error>` now compiles and works on Linux — this is what Task 6's `commands/audio.rs` changes call.

This task is mechanical: every `#[cfg(windows)]` in this file that gates system-audio machinery (not the Windows-specific mute/permission code, which stays Windows-only) becomes `#[cfg(any(windows, target_os = "linux"))]`.

- [ ] **Step 1: Widen the top-level import**

Change:

```rust
#[cfg(windows)]
use crate::audio_toolkit::audio::{list_output_devices, SystemAudioCapture};
```

to:

```rust
#[cfg(any(windows, target_os = "linux"))]
use crate::audio_toolkit::audio::{list_output_devices, SystemAudioCapture};
```

- [ ] **Step 2: Widen `PendingSystemAudioCapture`**

Change:

```rust
#[cfg(windows)]
#[derive(Clone)]
struct PendingSystemAudioCapture {
    enabled: bool,
    device_name: Option<String>,
}
```

to:

```rust
#[cfg(any(windows, target_os = "linux"))]
#[derive(Clone)]
struct PendingSystemAudioCapture {
    enabled: bool,
    device_name: Option<String>,
}
```

- [ ] **Step 3: Widen `create_audio_recorder`'s system-VAD block**

In `create_audio_recorder`, the parameter:

```rust
    #[cfg(windows)] system_stream_router: Option<Arc<StreamRouter>>,
```

becomes:

```rust
    #[cfg(any(windows, target_os = "linux"))] system_stream_router: Option<Arc<StreamRouter>>,
```

The body's:

```rust
    #[cfg(windows)]
    let system_smoothed_vad = system_stream_router
```

becomes:

```rust
    #[cfg(any(windows, target_os = "linux"))]
    let system_smoothed_vad = system_stream_router
```

And the trailing:

```rust
    #[cfg(windows)]
    let recorder = match (recorder, system_smoothed_vad, system_stream_router) {
```

becomes:

```rust
    #[cfg(any(windows, target_os = "linux"))]
    let recorder = match (recorder, system_smoothed_vad, system_stream_router) {
```

- [ ] **Step 4: Widen `AudioRecordingManager`'s struct fields and constructor**

The struct field:

```rust
    #[cfg(windows)]
    system_stream_router: Arc<Mutex<Option<Arc<StreamRouter>>>>,
```

and

```rust
    #[cfg(windows)]
    pending_system_audio: Arc<Mutex<Option<PendingSystemAudioCapture>>>,
```

both become `#[cfg(any(windows, target_os = "linux"))]`. Likewise in `new()`:

```rust
    pub fn new(
        app: &tauri::AppHandle,
        stream_router: Arc<StreamRouter>,
        #[cfg(windows)] system_stream_router: Option<Arc<StreamRouter>>,
    ) -> Result<Self, anyhow::Error> {
```

and the two `#[cfg(windows)]` field initializers inside the constructor body — all become `#[cfg(any(windows, target_os = "linux"))]`.

- [ ] **Step 5: Widen `get_effective_system_audio_device`**

Change:

```rust
    #[cfg(windows)]
    fn get_effective_system_audio_device(
```

to:

```rust
    #[cfg(any(windows, target_os = "linux"))]
    fn get_effective_system_audio_device(
```

- [ ] **Step 6: Widen the call site in `start_microphone_stream`**

The four `#[cfg(windows)]` attributes around `pending_system_audio` / `system_audio` resolution in `start_microphone_stream` (roughly lines 705-721 and 733-747) all become `#[cfg(any(windows, target_os = "linux"))]`.

- [ ] **Step 7: Widen `update_system_audio_capture` and `set_system_stream_router`**

```rust
    #[cfg(windows)]
    pub fn update_system_audio_capture(
```

and

```rust
    #[cfg(windows)]
    pub fn set_system_stream_router(&self, router: Option<Arc<StreamRouter>>) {
```

both become `#[cfg(any(windows, target_os = "linux"))]`.

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p shorthand`

Expected: compiles cleanly on whatever platform you're developing on.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/managers/audio.rs
git commit -m "feat(linux): widen system-audio dual-lane wiring to Linux"
```

---

### Task 4: Widen `SystemAudioTranscription` in `managers/transcription.rs`

**Files:**
- Modify: `src-tauri/src/managers/transcription.rs` (search for `#[cfg(windows)]` — expect it around `StreamSource::System` and the `SystemAudioTranscription` wrapper type)
- Modify: `src-tauri/src/commands/audio.rs:5-8` (the `use` importing `StreamSource, SystemAudioTranscription, TranscriptionManager`)
- Modify: `src-tauri/src/lib.rs` (wherever `SystemAudioTranscription` is registered as Tauri-managed state — search for `SystemAudioTranscription::` or `.manage(`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `SystemAudioTranscription` (a `Mutex<Option<Arc<TranscriptionManager>>>` wrapper, per its usage in `commands/audio.rs`) and `StreamSource::System` are now available as Tauri-managed state on Linux — this is what Task 6 reads/writes.

- [ ] **Step 1: Find and widen the gates**

Search `src-tauri/src/managers/transcription.rs` for `#[cfg(windows)]`:

```bash
grep -n '#\[cfg(windows)\]' src-tauri/src/managers/transcription.rs
```

For each match gating `StreamSource::System` or `SystemAudioTranscription`, change `#[cfg(windows)]` to `#[cfg(any(windows, target_os = "linux"))]`. Do not touch any Windows-only code in this file unrelated to system audio (e.g. Windows-specific registry or WASAPI-only helpers, if any exist here).

- [ ] **Step 2: Widen the import in `commands/audio.rs`**

Change:

```rust
#[cfg(windows)]
use crate::managers::transcription::{
    StreamSource, SystemAudioTranscription, TranscriptionManager,
};
```

to:

```rust
#[cfg(any(windows, target_os = "linux"))]
use crate::managers::transcription::{
    StreamSource, SystemAudioTranscription, TranscriptionManager,
};
```

- [ ] **Step 3: Widen the state registration in `lib.rs`**

Find where `SystemAudioTranscription` is registered (likely `.manage(SystemAudioTranscription(...))` or similar inside a `#[cfg(windows)]` block in the Tauri builder setup). Widen that gate to `#[cfg(any(windows, target_os = "linux"))]` the same way.

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p shorthand`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/managers/transcription.rs src-tauri/src/commands/audio.rs src-tauri/src/lib.rs
git commit -m "feat(linux): widen SystemAudioTranscription state to Linux"
```

---

### Task 5: Add `SystemAudioAvailability` and the availability command

**Files:**
- Modify: `src-tauri/src/commands/audio.rs` (add new enum + command near the existing `PermissionAccess`/`WindowsMicrophonePermissionStatus` types)
- Modify: `src-tauri/src/lib.rs` (register the new command in the `collect_commands![...]` list, alongside `commands::audio::change_system_audio_enabled_setting`)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module — new)

**Interfaces:**
- Produces: `SystemAudioAvailability` enum with variants `Available`, `UnavailableOsVersion`, `UnavailableNoSoundServer`, `PermissionDenied` (the last two currently unused on Linux — `PermissionDenied` is reserved for the macOS plan, `UnavailableOsVersion` likewise, per the spec's "model generically" decision). `pub fn get_system_audio_availability() -> SystemAudioAvailability` — a new `#[tauri::command]`.
- Consumes: `cpal::host_from_id`, `cpal::HostId` (from Task 1).

The pure logic here — "given whether PipeWire or PulseAudio can be reached, what's the availability?" — is unit-testable. The actual reachability check (calling into cpal, which touches real sockets) is not, so it's split into a thin wrapper (untested, one line) and a pure function (tested).

- [ ] **Step 1: Write the failing test for the pure decision logic**

Add to the bottom of `src-tauri/src/commands/audio.rs`:

```rust
#[cfg(test)]
mod system_audio_availability_tests {
    use super::*;

    #[test]
    fn available_when_a_linux_sound_server_is_reachable() {
        assert_eq!(
            linux_availability_from_backend_check(true),
            SystemAudioAvailability::Available
        );
    }

    #[test]
    fn unavailable_when_no_linux_sound_server_is_reachable() {
        assert_eq!(
            linux_availability_from_backend_check(false),
            SystemAudioAvailability::UnavailableNoSoundServer
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shorthand system_audio_availability_tests`

Expected: FAIL — `SystemAudioAvailability`, `linux_availability_from_backend_check` not found.

- [ ] **Step 3: Write the enum and the pure + impure functions**

Add above the test module (near the existing `PermissionAccess` enum, for proximity):

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioAvailability {
    Available,
    UnavailableOsVersion,
    UnavailableNoSoundServer,
    PermissionDenied,
}

/// Pure decision logic, split out from the real backend probe so it's
/// unit-testable without touching a real PipeWire/PulseAudio socket.
fn linux_availability_from_backend_check(sound_server_reachable: bool) -> SystemAudioAvailability {
    if sound_server_reachable {
        SystemAudioAvailability::Available
    } else {
        SystemAudioAvailability::UnavailableNoSoundServer
    }
}

/// Attempts to construct a cpal host for PipeWire, then PulseAudio — the
/// same priority order cpal itself uses when both features are compiled in.
/// Each `host_from_id` call attempts a real connection, so success here means
/// a sound server is actually reachable right now, not just that the Cargo
/// features were compiled in.
#[cfg(target_os = "linux")]
fn linux_sound_server_reachable() -> bool {
    cpal::host_from_id(cpal::HostId::PipeWire).is_ok()
        || cpal::host_from_id(cpal::HostId::PulseAudio).is_ok()
}

#[tauri::command]
#[specta::specta]
pub fn get_system_audio_availability() -> SystemAudioAvailability {
    #[cfg(target_os = "linux")]
    {
        linux_availability_from_backend_check(linux_sound_server_reachable())
    }

    #[cfg(windows)]
    {
        // Windows system audio capture has been available unconditionally
        // since before this availability command existed.
        SystemAudioAvailability::Available
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        SystemAudioAvailability::UnavailableOsVersion
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shorthand system_audio_availability_tests`

Expected: PASS (2 tests).

- [ ] **Step 5: Register the command**

In `src-tauri/src/lib.rs`, in the `collect_commands![...]` list, add `commands::audio::get_system_audio_availability,` immediately after the existing `commands::audio::set_system_audio_device,` line (around line 771-772).

- [ ] **Step 6: Regenerate frontend bindings**

Run: `bun run tauri dev` briefly (or whatever the repo's existing bindings-export step is — check for a dedicated script first: `grep -n "specta\|bindings" package.json`), then stop it once `src/bindings.ts` has regenerated with `getSystemAudioAvailability` and the `SystemAudioAvailability` type.

Expected: `src/bindings.ts` now contains a `getSystemAudioAvailability` command and a `SystemAudioAvailability` union type with the four snake_case variant strings.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/audio.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(linux): add system audio availability detection"
```

---

### Task 6: Replace the Linux hard-error in `commands/audio.rs` with real logic

**Files:**
- Modify: `src-tauri/src/commands/audio.rs:392-472` (`change_system_audio_enabled_setting`)
- Modify: `src-tauri/src/commands/audio.rs:474-516` (`set_system_audio_device`)

**Interfaces:**
- Consumes: `AudioRecordingManager::update_system_audio_capture` (Task 3), `SystemAudioTranscription`, `StreamSource::System`, `TranscriptionManager::new` (Task 4).
- Produces: both commands now succeed on Linux instead of always returning `Err("... only available on Windows")`.

- [ ] **Step 1: Widen the cfg on `change_system_audio_enabled_setting`'s body**

Change:

```rust
    #[cfg(windows)]
    {
        let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
        // ... existing Windows body unchanged ...
    }

    #[cfg(not(windows))]
    Err("System audio capture is only available on Windows".to_string())
```

to:

```rust
    #[cfg(any(windows, target_os = "linux"))]
    {
        let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
        // ... existing body unchanged — it was already platform-generic,
        // relying only on the types widened in Tasks 2-4 ...
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    Err("System audio capture is not available on this platform".to_string())
```

The existing body between the braces needs no other changes — it only calls `AudioRecordingManager::update_system_audio_capture`, `SystemAudioTranscription`, and `TranscriptionManager::new`, all of which Tasks 3-4 already made available on Linux.

- [ ] **Step 2: Widen the cfg on `set_system_audio_device`'s body**

Same transformation:

```rust
    #[cfg(windows)]
    {
        // ... existing Windows body unchanged ...
    }

    #[cfg(not(windows))]
    {
        let _ = (app, device_name);
        Err("System audio capture is only available on Windows".to_string())
    }
```

becomes:

```rust
    #[cfg(any(windows, target_os = "linux"))]
    {
        // ... existing body unchanged ...
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (app, device_name);
        Err("System audio capture is not available on this platform".to_string())
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p shorthand`

- [ ] **Step 4: Manual verification (cannot be unit-tested — real sound server required)**

On a Linux machine with PipeWire running (the default on current Ubuntu/Fedora):
1. `bun run tauri dev`
2. Play audio from any app (e.g. a browser video).
3. In Shorthand's settings, enable "System audio capture" and select the default output device.
4. Trigger a recording. Confirm the transcript includes speech from the played audio, not just the microphone.

Expected: system audio is transcribed alongside the microphone, matching existing Windows behavior.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/audio.rs
git commit -m "feat(linux): enable system audio capture commands on Linux"
```

---

### Task 7: Frontend availability gating

**Files:**
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx:22-24`
- Test: `src/components/settings/advanced/SystemAudioCapture.test.tsx` (new — check `docs/FRONTEND_TESTING.md` first for the repo's test setup/conventions before writing this)

**Interfaces:**
- Consumes: `commands.getSystemAudioAvailability()` (from Task 5's regenerated `bindings.ts`).
- Produces: the toggle renders (or doesn't) based on availability rather than OS name.

- [ ] **Step 1: Read the frontend testing doc**

Before writing the test in this task, read `docs/FRONTEND_TESTING.md` for the existing test runner, mocking conventions for `@/bindings`, and file-naming pattern used elsewhere in `src/components/settings/`.

- [ ] **Step 2: Write the failing test**

Following whatever pattern `docs/FRONTEND_TESTING.md` and existing sibling tests (search `src/components/settings/**/*.test.tsx`) use for mocking `commands.*` calls, write a test asserting: when `getSystemAudioAvailability` resolves to `"unavailable_no_sound_server"`, the component renders nothing (`null`), and when it resolves to `"available"`, the toggle renders. Use the actual mocking utility this codebase already has (do not introduce a new one) — copy the shape from an existing test in the same directory.

- [ ] **Step 3: Run the test to verify it fails**

Run whatever command `docs/FRONTEND_TESTING.md` specifies (e.g. `bun test` or `bun run test`).

Expected: FAIL — component still gates on `osType`, not on availability.

- [ ] **Step 4: Replace the OS-name gate with an availability query**

Change:

```tsx
export const SystemAudioCapture: React.FC<SystemAudioCaptureProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const osType = useOsType();
  const models = useModelStore((state) => state.models);

  if (osType !== "windows") {
    return null;
  }
```

to:

```tsx
export const SystemAudioCapture: React.FC<SystemAudioCaptureProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const models = useModelStore((state) => state.models);
  const [availability, setAvailability] = useState<
    SystemAudioAvailability | null
  >(null);

  useEffect(() => {
    commands.getSystemAudioAvailability().then(setAvailability);
  }, []);

  if (availability === null || availability === "unavailable_os_version" ||
      availability === "unavailable_no_sound_server") {
    return null;
  }
```

Add the necessary imports at the top of the file:

```tsx
import { useEffect, useState } from "react";
import { commands, type SystemAudioAvailability } from "@/bindings";
```

(Remove the now-unused `useOsType` import if nothing else in the file references it.)

Leave the `PermissionDenied` case rendering the existing toggle for this task — the macOS plan is responsible for adding a permission-denied CTA; this task's job is only to stop gating on OS name and start gating on real availability.

- [ ] **Step 5: Run the test to verify it passes**

Run the same test command as Step 3.

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/settings/advanced/SystemAudioCapture.tsx src/components/settings/advanced/SystemAudioCapture.test.tsx
git commit -m "feat(linux): gate system audio toggle on real availability, not OS name"
```

---

### Task 8: Build prerequisites and packaging dependencies

**Files:**
- Modify: `BUILD.md` (Linux prerequisites section, apt/dnf/pacman lists)
- Modify: `src-tauri/tauri.conf.json:47-64` (`deb.depends` and `rpm.depends` lists)

**Interfaces:** none (documentation and packaging metadata only).

- [ ] **Step 1: Add dev headers to `BUILD.md`**

In `BUILD.md`'s Linux prerequisites, add to each distro's install command:

Ubuntu/Debian: add `libpipewire-0.3-dev libpulse-dev` to the `apt install` line.
Fedora/RHEL: add `pipewire-devel pulseaudio-libs-devel` to the `dnf install` line.
Arch Linux: add `libpipewire libpulse` to the `pacman -S` line.

- [ ] **Step 2: Add runtime library dependencies to packaging**

In `src-tauri/tauri.conf.json`, change:

```json
"deb": {
  "depends": ["libgtk-layer-shell0", "libopenblas0"],
```

to:

```json
"deb": {
  "depends": ["libgtk-layer-shell0", "libopenblas0", "libpipewire-0.3-0", "libpulse0"],
```

And in the `rpm.depends` array, add the RPM-style equivalents (matching the existing `.so`-versioned style already used for `libgtk-layer-shell.so.0()(64bit)`):

```json
"rpm": {
  "depends": [
    "libgtk-layer-shell.so.0()(64bit)",
    "libpipewire-0.3.so.0()(64bit)",
    "libpulse.so.0()(64bit)"
  ],
```

(Read the full existing `rpm.depends` array first — it has more entries than shown in this excerpt — and append to it rather than replacing it.)

- [ ] **Step 3: Verify a full packaged build**

Run: `bun run tauri build -- --bundles deb` (per the AppImage troubleshooting note already in `BUILD.md`, skip AppImage if it's failing for the unrelated `linuxdeploy`/`strip` reason documented there)

Expected: builds successfully; `dpkg -I` on the resulting `.deb` shows the new dependencies listed.

- [ ] **Step 4: Commit**

```bash
git add BUILD.md src-tauri/tauri.conf.json
git commit -m "docs(linux): document pipewire/pulseaudio build and packaging deps"
```

---

### Task 9: Manual cross-configuration verification

No files change in this task — it's the manual test matrix the spec calls for, since none of these states can be exercised in CI.

- [ ] **Verify on Linux with PipeWire running** (default on current Ubuntu/Fedora): system audio toggle appears, capture works (per Task 6 Step 4).
- [ ] **Verify on Linux with PipeWire disabled and PulseAudio running** (`systemctl --user stop pipewire pipewire-pulse wireplumber; pulseaudio --start` or an older distro): toggle still appears, capture still works via the PulseAudio fallback.
- [ ] **Verify on Linux with neither running** (stop both, `pulseaudio --kill`): `get_system_audio_availability` returns `unavailable_no_sound_server`, toggle does not render, no crash or hang anywhere in the app.
- [ ] **Verify `cargo clippy` and `cargo fmt --check` are clean** per `AGENTS.md`'s Code Style section: `cargo fmt --check && cargo clippy -p shorthand`.
