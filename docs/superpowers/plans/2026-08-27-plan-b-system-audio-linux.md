# Plan B — System audio capture on Linux

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture system (output) audio on Linux as a second speaker-labelled lane alongside the microphone, matching Windows behaviour, with graceful unavailability when no PipeWire/PulseAudio server is running.

**Architecture:** Plan A already made the capture machinery platform-neutral, so this plan adds only what is Linux-specific: a **second cpal host**. `get_cpal_host()` pins ALSA on Linux for microphone and playback, and ALSA cannot see sink monitors — so loopback needs its own host resolved as PipeWire, then PulseAudio. That host also enumerates a different device set than the ALSA one, so system audio gets its own device list rather than sharing the playback selector's.

**Tech Stack:** Rust, `cpal` 0.18.x with `pipewire` + `pulseaudio` features, Tauri 2.x, React/TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Prerequisite:** Plan A (`2026-08-27-plan-a-cpal-018-migration.md`) must be complete and green. This plan assumes cpal 0.18 is in place and the system-audio machinery compiles on Linux. Independent of Plan C (macOS); either may land first.

## Global Constraints

- Enable the `pipewire`/`pulseaudio` cpal features **only** under `[target.'cfg(target_os = "linux")'.dependencies]`.
- **Do not change `get_cpal_host()`.** It pins ALSA for microphone and playback; repointing it would alter capture behaviour for every existing Linux user, which is out of scope. Loopback gets a separate host.
- No permission prompt, deep link, or consent UI exists or is needed on Linux. The only user-visible state is available vs. unavailable.
- Systems with neither PipeWire nor PulseAudio report unavailable. No ALSA `snd-aloop` fallback.
- Availability must be enforced in the **backend commands**, not only in the UI — a persisted `system_audio_enabled = true` must not start capture on a machine with no sound server.
- **No React unit tests.** Per `docs/FRONTEND_TESTING.md` this repo has no vitest/jest harness by deliberate decision; frontend changes are verified manually. Do not add one.
- All `cargo` commands use `--manifest-path src-tauri/Cargo.toml` — there is no root `Cargo.toml`.

---

### Task 1: Enable the Linux cpal backends

**Files:**
- Modify: `src-tauri/Cargo.toml` (the `[target.'cfg(target_os = "linux")'.dependencies]` section, currently at line 164)

**Interfaces:**
- Produces: `cpal::HostId::PipeWire` and `cpal::HostId::PulseAudio` exist at compile time on Linux, for Task 2's host resolver.

- [ ] **Step 1: Add the features**

In `src-tauri/Cargo.toml`, add a `cpal` line to the existing Linux target section, matching the exact version Plan A pinned on line 51:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
cpal = { version = "=0.18.2", features = ["pipewire", "pulseaudio"] }
gtk-layer-shell = { version = "0.8", features = ["v0_6"] }
gtk = "0.18"
```

(Leave the rest of the section untouched. Cargo unions feature requests across the base and target-specific entries, so this adds the backends for Linux only.)

- [ ] **Step 2: Install the build dependencies and verify**

```bash
sudo apt-get install -y libpipewire-0.3-dev libpulse-dev   # Debian/Ubuntu
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: compiles. A `pkg-config` failure here means the dev packages are missing — install them before continuing; Task 6 documents them properly.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(linux): enable cpal pipewire and pulseaudio backends"
```

---

### Task 2: Add the system-audio host resolver and availability probe

**Files:**
- Modify: `src-tauri/src/audio_toolkit/utils.rs`
- Modify: `src-tauri/src/audio_toolkit/mod.rs` (export the new function)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module, added in Task 4)

**Interfaces:**
- Produces: `pub fn get_system_audio_host() -> Option<cpal::Host>` — the host to use for loopback capture. `Some` on Linux when PipeWire or PulseAudio is reachable, `None` when neither is; on other platforms always `Some(get_cpal_host())`.

- [ ] **Step 1: Add the resolver**

Append to `src-tauri/src/audio_toolkit/utils.rs`:

```rust
/// Returns the CPAL host to use for system-audio (loopback) capture, or `None`
/// when this machine cannot support it.
///
/// This is deliberately NOT `get_cpal_host()`. That function pins ALSA on
/// Linux for microphone capture and playback, and ALSA does not expose the
/// per-sink monitor sources loopback needs — reaching those requires the
/// PipeWire or PulseAudio client protocol. Constructing the host attempts a
/// real connection, so `Some` here means a sound server is actually running,
/// not merely that the feature was compiled in.
///
/// PipeWire is tried first, then PulseAudio, matching both cpal's own
/// Linux host priority and this codebase's existing mute-control order
/// (`wpctl` > `pactl` > `amixer` in `managers::audio`).
pub fn get_system_audio_host() -> Option<cpal::Host> {
    #[cfg(target_os = "linux")]
    {
        cpal::host_from_id(cpal::HostId::PipeWire)
            .or_else(|_| cpal::host_from_id(cpal::HostId::PulseAudio))
            .ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(get_cpal_host())
    }
}
```

- [ ] **Step 2: Export it**

In `src-tauri/src/audio_toolkit/mod.rs`, add `get_system_audio_host` to the same `pub use` that already exports `get_cpal_host`.

- [ ] **Step 3: Verify it compiles**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/audio_toolkit/utils.rs src-tauri/src/audio_toolkit/mod.rs
git commit -m "feat(linux): resolve a separate cpal host for loopback capture"
```

---

### Task 3: Linux device enumeration and resolution

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/device.rs`
- Modify: `src-tauri/src/audio_toolkit/audio/mod.rs:8` (the `device` re-export)
- Modify: `src-tauri/src/managers/audio.rs` (`get_effective_system_audio_device`)

**Interfaces:**
- Consumes: `get_system_audio_host()` (Task 2).
- Produces: `pub fn list_system_audio_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>>` — output devices as seen by the loopback host. `get_effective_system_audio_device` resolves against that host on all platforms.

Why a separate list: on Linux the loopback host (PipeWire/PulseAudio) enumerates a **different device set** than `list_output_devices()`'s ALSA host. Sharing the playback selector's list would show names that cannot be matched at capture time.

- [ ] **Step 1: Add the enumeration**

Append to `src-tauri/src/audio_toolkit/audio/device.rs`:

```rust
/// Output devices as seen by the host used for loopback capture.
///
/// On Linux this enumerates a different device set than `list_output_devices`:
/// that function uses the ALSA host (see `get_cpal_host`), while loopback runs
/// on PipeWire/PulseAudio. The two name sets do not correspond, so the system
/// audio selector must use this list, not the playback one. Returns an empty
/// list when no loopback-capable host is available.
pub fn list_system_audio_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let Some(host) = crate::audio_toolkit::get_system_audio_host() else {
        return Ok(Vec::new());
    };
    let default_name = host.default_output_device().and_then(|d| d.name().ok());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device.name().unwrap_or_else(|_| "Unknown".into());
        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}
```

- [ ] **Step 2: Re-export it**

In `src-tauri/src/audio_toolkit/audio/mod.rs`, extend line 8:

```rust
pub use device::{
    list_input_devices, list_output_devices, list_system_audio_devices, CpalDeviceInfo,
};
```

- [ ] **Step 3: Point device resolution at the loopback host**

In `src-tauri/src/managers/audio.rs`, replace `get_effective_system_audio_device`'s body. Remove the `#[cfg(not(windows))] return None` stub Plan A added, and switch both branches from the ALSA-pinned host to the loopback host:

```rust
    fn get_effective_system_audio_device(
        &self,
        enabled: bool,
        device_name: Option<&str>,
    ) -> Option<SystemAudioCapture> {
        if !enabled {
            return None;
        }

        // Resolve against the loopback host, not `get_cpal_host()`: on Linux
        // those are different hosts enumerating different devices, and a name
        // from one cannot be opened on the other.
        let device = if let Some(device_name) = device_name {
            match list_system_audio_devices() {
                Ok(devices) => devices
                    .into_iter()
                    .find(|device| device.name == device_name)
                    .map(|device| device.device),
                Err(error) => {
                    warn!("Failed to list system audio devices: {error}");
                    None
                }
            }
        } else {
            crate::audio_toolkit::get_system_audio_host()
                .and_then(|host| host.default_output_device())
        };

        match device {
            Some(device) => Some(SystemAudioCapture { device }),
            None => {
                warn!("Configured system audio device is unavailable; continuing microphone-only");
                None
            }
        }
    }
```

Update the import at the top of the file from `list_output_devices` to `list_system_audio_devices` (check whether `list_output_devices` is still used elsewhere in this file before removing it).

- [ ] **Step 4: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/audio_toolkit src-tauri/src/managers/audio.rs
git commit -m "feat(linux): resolve system audio devices via the loopback host"
```

---

### Task 4: Availability command and backend enforcement

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (register the new commands in `collect_commands![...]`, near line 770)
- Test: `src-tauri/src/commands/audio.rs` (new inline `#[cfg(test)]` module)

**Interfaces:**
- Produces:
  - `SystemAudioAvailability` — `Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type`, `#[serde(rename_all = "snake_case")]`, variants `Available`, `UnavailableNoSoundServer`, `PermissionDenied`. (`PermissionDenied` is unused on Linux; Plan C uses it. Declared here so the TypeScript union is stable regardless of plan order.)
  - `pub fn get_system_audio_availability() -> SystemAudioAvailability`
  - `pub async fn get_available_system_audio_devices() -> Result<Vec<AudioDevice>, String>`

- [ ] **Step 1: Write the failing test**

Add at the bottom of `src-tauri/src/commands/audio.rs`:

```rust
#[cfg(test)]
mod system_audio_availability_tests {
    use super::*;

    #[test]
    fn available_when_a_loopback_host_is_reachable() {
        assert_eq!(
            availability_from_host_probe(true),
            SystemAudioAvailability::Available
        );
    }

    #[test]
    fn unavailable_when_no_loopback_host_is_reachable() {
        assert_eq!(
            availability_from_host_probe(false),
            SystemAudioAvailability::UnavailableNoSoundServer
        );
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_availability_tests
```

Expected: FAIL — `SystemAudioAvailability` and `availability_from_host_probe` not found.

- [ ] **Step 3: Implement**

Add near the existing `PermissionAccess` enum in `src-tauri/src/commands/audio.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioAvailability {
    Available,
    UnavailableNoSoundServer,
    PermissionDenied,
}

/// Pure decision logic, split from the real host probe so it is unit-testable
/// without a running sound server.
fn availability_from_host_probe(loopback_host_reachable: bool) -> SystemAudioAvailability {
    if loopback_host_reachable {
        SystemAudioAvailability::Available
    } else {
        SystemAudioAvailability::UnavailableNoSoundServer
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_system_audio_availability() -> SystemAudioAvailability {
    availability_from_host_probe(crate::audio_toolkit::get_system_audio_host().is_some())
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    // cpal enumeration can stall; keep it off the webview/main run loop, the
    // same way `get_available_output_devices` does.
    tokio::task::spawn_blocking(|| {
        let devices = crate::audio_toolkit::audio::list_system_audio_devices()
            .map_err(|e| format!("Failed to list system audio devices: {e}"))?;

        let mut result = vec![AudioDevice {
            index: "default".to_string(),
            name: "Default".to_string(),
            is_default: true,
        }];

        result.extend(devices.into_iter().map(|d| AudioDevice {
            index: d.index,
            name: d.name,
            is_default: false,
        }));

        Ok::<_, String>(result)
    })
    .await
    .map_err(|e| format!("audio task join failed: {e}"))?
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_availability_tests
```

Expected: PASS (2 tests).

- [ ] **Step 5: Replace the off-Windows hard errors and enforce availability**

In `change_system_audio_enabled_setting`, delete the `#[cfg(windows)]` / `#[cfg(not(windows))]` split entirely (Plan A already made every type it uses available on all platforms), so the former Windows body becomes the only body. Then add an availability guard alongside the existing mute and streaming-model guards near the top:

```rust
    if enabled && get_system_audio_availability() != SystemAudioAvailability::Available {
        return Err(
            "System audio capture is not available on this system (no PipeWire or PulseAudio server found)"
                .to_string(),
        );
    }
```

This is the backend enforcement the spec requires: a persisted setting or a direct command invocation cannot start capture on a machine with no sound server.

Apply the same de-gating to `set_system_audio_device` (delete both `#[cfg]` arms, keep the former Windows body).

- [ ] **Step 6: Register the commands**

In `src-tauri/src/lib.rs`'s `collect_commands![...]`, after `commands::audio::set_system_audio_device,` add:

```rust
            commands::audio::get_system_audio_availability,
            commands::audio::get_available_system_audio_devices,
```

- [ ] **Step 7: Regenerate bindings and verify**

```bash
bun run tauri dev    # briefly, then stop once bindings regenerate
```

Confirm `src/bindings.ts` gained `getSystemAudioAvailability`, `getAvailableSystemAudioDevices` and a `SystemAudioAvailability` type.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/audio.rs src-tauri/src/lib.rs src/bindings.ts
git commit -m "feat(linux): add system audio availability detection and enforcement"
```

---

### Task 5: Frontend availability gating

**Files:**
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx:22-24`
- Modify: `src/components/settings/advanced/SystemAudioDeviceSelector.tsx:26-30`
- Modify: `src/shorthand/settings/ModesSettings.tsx` (around line 415, the Windows-gated direct-dictation row)
- Modify: `src/stores/settingsStore.ts` (add a system-audio device list alongside `refreshOutputDevices`)

**Interfaces:**
- Consumes: `commands.getSystemAudioAvailability()`, `commands.getAvailableSystemAudioDevices()`.

No unit tests — this repo has no React test harness by deliberate decision (`docs/FRONTEND_TESTING.md`). Verification is manual, in Task 7.

- [ ] **Step 1: Add a system-audio device list to the store**

In `src/stores/settingsStore.ts`, add `systemAudioDevices` state and a `refreshSystemAudioDevices` action, copying the shape of the existing `outputDevices`/`refreshOutputDevices` pair exactly (including how it owns the `"Default"` sentinel — see the long comment in `SystemAudioDeviceSelector.tsx:35-45`, which documents that the store, not the component, prepends it). Back it with `commands.getAvailableSystemAudioDevices()`.

- [ ] **Step 2: Gate the toggle on availability instead of OS**

In `src/components/settings/advanced/SystemAudioCapture.tsx`, replace:

```tsx
  const osType = useOsType();
  const models = useModelStore((state) => state.models);

  if (osType !== "windows") {
    return null;
  }
```

with:

```tsx
  const models = useModelStore((state) => state.models);
  const [availability, setAvailability] =
    useState<SystemAudioAvailability | null>(null);

  useEffect(() => {
    commands.getSystemAudioAvailability().then(setAvailability);
  }, []);

  // Null while the probe is in flight; hide rather than flash a control that
  // may be about to disappear.
  if (availability === null || availability === "unavailable_no_sound_server") {
    return null;
  }
```

Add the imports:

```tsx
import { useEffect, useState } from "react";
import { commands, type SystemAudioAvailability } from "@/bindings";
```

and remove the `useOsType` import if nothing else in the file uses it.

Leave `"permission_denied"` falling through to the normal toggle for now — Plan C adds its CTA.

- [ ] **Step 3: Gate the device selector the same way**

In `src/components/settings/advanced/SystemAudioDeviceSelector.tsx`, replace the `osType !== "windows"` early return with the same availability check, and switch it from the store's `outputDevices`/`refreshOutputDevices` to the new `systemAudioDevices`/`refreshSystemAudioDevices` from Step 1.

- [ ] **Step 4: Un-gate the modes row**

In `src/shorthand/settings/ModesSettings.tsx` (around line 415), find the Windows-only condition guarding the system-audio-related row and replace it with the same availability check. Read the surrounding code first — if it gates on `osType` for more than just system audio, only change the system-audio part.

- [ ] **Step 5: Verify the frontend builds and lints**

```bash
bun run build && bun run lint
```

Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat(linux): gate system audio UI on availability rather than OS"
```

---

### Task 6: Build, CI, Nix, and packaging dependencies

**Files:**
- Modify: `BUILD.md` (Linux prerequisites)
- Modify: `src-tauri/tauri.conf.json` (`linux.deb.depends`, `linux.rpm.depends`)
- Modify: `.github/workflows/build.yml` (lines 124, 136, 142 — the three `apt-get install` invocations)
- Modify: `flake.nix` (the `commonNativeDeps` list, around line 40)

**Interfaces:** none — build and packaging metadata.

- [ ] **Step 1: Document the dev dependencies**

In `BUILD.md`'s Linux prerequisites, add to each distro's install line:

- Ubuntu/Debian: `libpipewire-0.3-dev libpulse-dev`
- Fedora/RHEL: `pipewire-devel pulseaudio-libs-devel`
- Arch: `libpipewire libpulse`

- [ ] **Step 2: Add runtime dependencies to the packages**

In `src-tauri/tauri.conf.json`, append to `linux.deb.depends`: `"libpipewire-0.3-0"`, `"libpulse0"`. Append to `linux.rpm.depends` the `.so`-versioned equivalents matching that array's existing style: `"libpipewire-0.3.so.0()(64bit)"`, `"libpulse.so.0()(64bit)"`. Read both arrays in full first and append rather than replace.

- [ ] **Step 3: Add the dev packages to CI**

In `.github/workflows/build.yml`, add `libpipewire-0.3-dev libpulse-dev` to the `apt-get install` lines at 124, 136 and 142. Check whether any other workflow installs Linux build deps (`grep -rn "libasound2-dev" .github/workflows/`) and update those too.

- [ ] **Step 4: Add them to the Nix dev shell**

In `flake.nix`, add `pipewire` and `libpulseaudio` to `commonNativeDeps` alongside the existing `alsa-lib` (around line 40).

- [ ] **Step 5: Verify a packaged build**

```bash
bun run tauri build -- --bundles deb
dpkg -I src-tauri/target/release/bundle/deb/*.deb | grep -i depends
```

Expected: builds, and the new dependencies appear. (Skip AppImage if it fails for the pre-existing `linuxdeploy`/`strip` reason already documented in `BUILD.md`'s troubleshooting.)

- [ ] **Step 6: Commit**

```bash
git add BUILD.md src-tauri/tauri.conf.json .github/workflows flake.nix
git commit -m "build(linux): add pipewire and pulseaudio build and runtime deps"
```

---

### Task 7: Manual verification matrix

No files change. These states cannot be exercised in CI.

- [ ] **PipeWire (default on current Ubuntu/Fedora)**: play audio from another app, enable system audio capture, record, confirm the transcript contains both speakers labelled `me` and `them`.
- [ ] **Device selection**: pick a non-default output in the system-audio selector and confirm capture follows it. Confirm the listed names are PipeWire's, and that selecting one actually opens (this is the cross-host name-matching risk Task 3 exists to avoid).
- [ ] **Follow-stream**: run `handy --follow-stream` during a dual-speaker session and confirm both `"speaker":"me"` and `"speaker":"them"` events appear. No code change should have been needed for this.
- [ ] **PulseAudio only**: `systemctl --user stop pipewire pipewire-pulse wireplumber`, start PulseAudio, confirm the toggle still appears and capture still works via the fallback host.
- [ ] **Neither server**: stop both, confirm `get_system_audio_availability` returns `unavailable_no_sound_server`, the toggle and selector do not render, and nothing crashes or hangs.
- [ ] **Backend enforcement**: with no sound server running, set `system_audio_enabled = true` directly in the persisted settings file, restart the app, and confirm it does not attempt capture and surfaces no crash — the Task 4 Step 5 guard should reject it.
- [ ] **Microphone unaffected**: confirm ordinary dictation still works on all three configurations — `get_cpal_host()` was deliberately left on ALSA and must not have regressed.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && bun run lint`.
