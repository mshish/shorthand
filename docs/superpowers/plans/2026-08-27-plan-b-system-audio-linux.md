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

### Task 2: Spike — establish how cpal actually exposes Linux loopback

**Files:** none permanent. This task produces **findings**, not shipped code. Write them into this plan (edit Task 3 below) before continuing.

**Why this task exists.** The Windows implementation opens an *output* device as an input stream, and `get_preferred_loopback_config` asks that device for its **output** config. Neither is guaranteed on Linux: an independent review raised that cpal's PipeWire host may expose its synthetic default-output entry as output-only (with loopback living on a separate duplex entry), and that its PulseAudio host may reject input streams on sink devices entirely, requiring the monitor **source** to be selected as an input device instead. If that is right, Task 3's resolution logic is wrong in a way that would only surface at runtime on a machine none of the earlier tasks touch. Half an hour here saves debugging a silent no-audio bug later.

Do this on a real Linux machine with PipeWire running.

- [ ] **Step 1: Enumerate what each host actually offers**

Write a scratch binary (`src-tauri/examples/loopback_probe.rs`, deleted at the end of this task) that, for each of `HostId::PipeWire` and `HostId::PulseAudio`:

```rust
// For each host: print every device's name, whether it appears in
// input_devices(), output_devices(), or both; and for each, whether
// default_input_config() and default_output_config() succeed.
```

Run it with `cargo run --manifest-path src-tauri/Cargo.toml --example loopback_probe`.

- [ ] **Step 2: Record the answers**

For each backend, answer in writing:

1. Which device does "capture what is currently playing" correspond to — the sink itself, a separate `*.monitor` entry, or a synthetic default?
2. Does that device appear in `output_devices()`, `input_devices()`, or both?
3. Does `default_output_config()` succeed on it, or only `default_input_config()`?
4. Does `host.default_output_device()` return something that can actually be opened for input, or is a named device required?

- [ ] **Step 3: Prove capture end-to-end**

Extend the scratch binary to build an input stream on the device your answers point at, play audio from another app, and print the peak sample value per callback. Confirm non-zero values while audio plays and near-zero when it stops — for **both** PipeWire and PulseAudio (stop PipeWire to test the latter).

- [ ] **Step 4: Fold the findings into Task 3**

Edit Task 3's Step 1 and Step 3 below so their code matches what you measured — particularly whether `list_system_audio_devices` should enumerate `output_devices()` or `input_devices()`, and whether the default-device path needs to resolve a named device instead. If the answers differ between PipeWire and PulseAudio, Task 3 must branch on the host, and say so.

If capture cannot be made to work on either backend, **stop and report** rather than proceeding — the rest of this plan rests on it.

- [ ] **Step 5: Delete the scratch binary and commit the findings**

```bash
rm src-tauri/examples/loopback_probe.rs
git add docs/superpowers/plans/2026-08-27-plan-b-system-audio-linux.md
git commit -m "docs(linux): record cpal loopback device findings from spike"
```

---

### Task 2b: Enable the real Linux host resolution

**Files:**
- Modify: `src-tauri/src/audio_toolkit/utils.rs` (`get_system_audio_host`, added by Plan A)

**Interfaces:**
- Produces: `get_system_audio_host()` returns a real host on Linux instead of Plan A's placeholder `None`.

Plan A added this function with its Linux arm stubbed to `None`, because naming the `HostId::PipeWire`/`PulseAudio` variants before Task 1 enabled their Cargo features would not compile. Now that they exist, fill it in.

- [ ] **Step 1: Replace the Linux arm**

In `src-tauri/src/audio_toolkit/utils.rs`, change the `#[cfg(target_os = "linux")]` arm of `get_system_audio_host` from `None` to:

```rust
        // Constructing the host attempts a real connection, so `Some` here
        // means a sound server is actually running — not merely that the
        // feature was compiled in. PipeWire first, then PulseAudio, matching
        // cpal's own Linux host priority and this codebase's existing
        // mute-control order (`wpctl` > `pactl` > `amixer`).
        cpal::host_from_id(cpal::HostId::PipeWire)
            .or_else(|_| cpal::host_from_id(cpal::HostId::PulseAudio))
            .ok()
```

Delete the now-obsolete sentence in the doc comment about the Linux arm returning `None` until this plan lands.

- [ ] **Step 2: Verify and commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/audio_toolkit/utils.rs
git commit -m "feat(linux): resolve PipeWire or PulseAudio for loopback capture"
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

> **Depends on Task 2's findings.** The code below assumes the loopback device appears in `output_devices()` and that `host.default_output_device()` is openable for input — the shape that holds on Windows. Task 2 exists to verify that on Linux. If it found otherwise (monitor sources appearing as *input* devices, a named device required instead of the synthetic default, or the two backends differing), amend the code below to match what you measured before writing it. Do not implement this task without Task 2's answers in hand.

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

Apply the same de-gating to `set_system_audio_device` (delete both `#[cfg]` arms, keep the former Windows body).

- [ ] **Step 5b: Enforce at startup too — the command guard is not enough**

The guard above only fires when the user toggles the setting. **Startup does not go through it**: `lib.rs` (around line 171) reads persisted settings directly and constructs the managers, so a settings file carrying `system_audio_enabled = true` from a machine that had a sound server would still attempt capture on one that does not.

At the point in `lib.rs` where persisted settings are read and the system-audio transcription manager is created, gate that construction on availability, and normalise the setting so the UI does not show a toggle that lies:

```rust
    // A settings file can arrive from a machine that had a sound server, or
    // this one may have lost it since. Availability is a property of the
    // running system, not of the saved preference.
    if settings.system_audio_enabled
        && commands::audio::get_system_audio_availability(app.handle().clone())
            != commands::audio::SystemAudioAvailability::Available
    {
        log::warn!("System audio was enabled in settings but is unavailable here; disabling");
        let mut settings = settings.clone();
        settings.system_audio_enabled = false;
        write_settings(app.handle(), settings);
    }
```

Read the surrounding startup code first and splice this where `settings` is already in scope and before the managers are built. If `get_system_audio_availability` takes an `AppHandle` (it does once Plan C lands; it does not before), match whichever signature is current.

- [ ] **Step 5c: Verify the guard**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Then confirm manually in Task 7 — with no sound server, a persisted `system_audio_enabled = true` must be corrected at startup rather than attempted.

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

  const refreshAvailability = useCallback(
    () => commands.getSystemAudioAvailability().then(setAvailability),
    [],
  );

  useEffect(() => {
    refreshAvailability();
  }, [refreshAvailability]);

  // Null while the probe is in flight; hide rather than flash a control that
  // may be about to disappear.
  if (availability === null || availability === "unavailable_no_sound_server") {
    return null;
  }
```

Then make the toggle re-read availability after every enable attempt. On macOS (Plan C) permission state is only *observed* from an attempt, so without this the UI can never learn it was denied:

```tsx
      onChange={async (enabled) => {
        await updateSetting("system_audio_enabled", enabled);
        await refreshAvailability();
      }}
```

Import `useCallback` alongside `useEffect`/`useState`.

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
- [ ] **Startup enforcement**: with no sound server running, set `system_audio_enabled = true` directly in the persisted settings file, restart the app, and confirm the Task 4 Step 5b startup guard corrects the setting (check the log for the "disabling" warning), no capture is attempted, and nothing crashes. Note that the Step 5 command guard alone would *not* catch this — startup never calls that command, which is why 5b exists.
- [ ] **Microphone unaffected**: confirm ordinary dictation still works on all three configurations — `get_cpal_host()` was deliberately left on ALSA and must not have regressed.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && bun run lint`.
