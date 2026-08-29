# Plan B — System audio capture on Linux

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture system (output) audio on Linux as a second speaker-labelled lane alongside the microphone, matching Windows behaviour, reporting unavailable when no PipeWire/PulseAudio server is running.

**Architecture:** Phase A already made the capture machinery platform-neutral, de-gated the Tauri commands, and added the availability plumbing and UI gating. What is left is Linux-specific: a **second cpal host**. `get_cpal_host()` pins ALSA for microphone and playback, and ALSA cannot see sink monitors — so loopback needs its own host, resolved as PipeWire then PulseAudio. That host enumerates a different device set than the ALSA one, so system audio also needs its own device list rather than sharing the playback selector's.

**Tech Stack:** Rust, `cpal` 0.18.x with `pipewire` + `pulseaudio` features, Tauri 2.x, React/TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Phase 2 of 3, all on one branch.** Phase A must be complete and green first. Phases A, B and C ship together as a single piece of work — no phase reaches users alone, so do not add guards or UI states whose only purpose is to make an intermediate phase safe. Each **commit** should still build.

## Global Constraints

- Enable the `pipewire`/`pulseaudio` cpal features **only** under `[target.'cfg(target_os = "linux")'.dependencies]`.
- **Do not change `get_cpal_host()`.** It pins ALSA for microphone and playback; repointing it would alter capture for every existing Linux user, which is out of scope. Loopback gets a separate host.
- No permission prompt, deep link, or consent UI exists or is needed on Linux.
- Systems with neither PipeWire nor PulseAudio report unavailable. No ALSA `snd-aloop` fallback.
- `Device::name()` is gone in cpal 0.18. Use `device.to_string()` for the display/persisted name — `description()` returns a `DeviceDescription` struct, not a `String`. Match exactly what Phase A Task 2 did in `device.rs`, so the two enumerations produce comparable strings.
- **No React unit tests.** Per `docs/FRONTEND_TESTING.md` this repo has no vitest/jest harness by deliberate decision. Frontend changes are verified manually.
- All `cargo` commands use `--manifest-path src-tauri/Cargo.toml`.

---

### Task 1: Enable the Linux cpal backends

**Files:**

- Modify: `src-tauri/Cargo.toml` (the `[target.'cfg(target_os = "linux")'.dependencies]` section, currently at line 164)

**Interfaces:**

- Produces: `cpal::HostId::PipeWire` and `cpal::HostId::PulseAudio` exist at compile time on Linux.

- [ ] **Step 1: Add the features**

Add a `cpal` line to the existing Linux target section, matching the exact version Phase A pinned on line 51:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
cpal = { version = "=0.18.2", features = ["pipewire", "pulseaudio"] }
gtk-layer-shell = { version = "0.8", features = ["v0_6"] }
gtk = "0.18"
```

Leave the rest of the section untouched. Cargo unions feature requests across the base and target-specific entries, so this adds the backends for Linux only.

- [ ] **Step 2: Install build dependencies and verify**

Only PipeWire needs a native library. cpal's PulseAudio backend is a **pure-Rust** reimplementation of the wire protocol (the `pulseaudio` crate) and links no `libpulse` — do not add one.

```bash
sudo apt-get install -y libpipewire-0.3-dev   # Debian/Ubuntu
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: compiles. A `pkg-config` failure means the dev package is missing.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(linux): enable cpal pipewire and pulseaudio backends"
```

---

### Task 2: Spike — establish how cpal actually exposes Linux loopback

**Files:** none permanent. This task produces **findings**, which you write into Task 4 below before implementing it.

**Why this task exists.** The Windows implementation opens an _output_ device as an input stream, and `get_preferred_loopback_config` asks that device for its **output** config. Neither is guaranteed on Linux. An independent review raised that cpal's PipeWire host may expose its synthetic default-output entry as output-only (with loopback living on a separate duplex entry), and that its PulseAudio host may reject input streams on sink devices entirely, requiring the monitor **source** to be opened as an input device instead. If that is right, Task 4's resolution logic is wrong in a way that only surfaces at runtime. Half an hour here saves debugging a silent no-audio bug later.

Do this on a real Linux machine with PipeWire running.

- [ ] **Step 1: Enumerate what each host offers**

Write a scratch binary `src-tauri/examples/loopback_probe.rs` (deleted at the end of this task) that, for each of `HostId::PipeWire` and `HostId::PulseAudio`, prints every device's `description()` and `id()`, whether it appears in `input_devices()`, `output_devices()` or both, and whether `default_input_config()` and `default_output_config()` succeed on it.

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example loopback_probe
```

- [ ] **Step 2: Record the answers**

For each backend, answer in writing:

1. Which device corresponds to "capture what is currently playing" — the sink itself, a separate `*.monitor` entry, or a synthetic default?
2. Does it appear in `output_devices()`, `input_devices()`, or both?
3. Does `default_output_config()` succeed on it, or only `default_input_config()`?
4. Does `host.default_output_device()` return something openable for input, or is a named device required?

- [ ] **Step 3: Prove capture end-to-end**

Extend the scratch binary to build an input stream on the device your answers point at, play audio from another app, and print the peak sample value per callback. Confirm non-zero values while audio plays and near-zero when it stops — for **both** PipeWire and PulseAudio (stop PipeWire to exercise the latter).

- [ ] **Step 4: Fold the findings into Task 4**

Edit Task 4's steps so their code matches what you measured — particularly whether `list_system_audio_devices` should enumerate `output_devices()` or `input_devices()`, and whether the default-device path must resolve a named device instead. If the two backends differ, Task 4 must branch on the host; say so explicitly.

If capture cannot be made to work on either backend, **stop and report** — the rest of this phase rests on it.

- [ ] **Step 5: Delete the scratch binary and commit the findings**

```bash
rm src-tauri/examples/loopback_probe.rs
git add docs/superpowers/plans/2026-08-27-plan-b-system-audio-linux.md
git commit -m "docs(linux): record cpal loopback device findings from spike"
```

---

### Task 3: Enable the real Linux host resolution

**Files:**

- Modify: `src-tauri/src/audio_toolkit/utils.rs` (`get_system_audio_host`, added by Phase A)

**Interfaces:**

- Produces: `get_system_audio_host()` returns a real host on Linux instead of Phase A's placeholder `None`.

Phase A added this function with its Linux arm stubbed to `None`, because naming the `HostId::PipeWire`/`PulseAudio` variants before Task 1 enabled their Cargo features would not compile. Now they exist.

- [ ] **Step 1: Replace the Linux arm**

Change the `#[cfg(target_os = "linux")]` arm of `get_system_audio_host` from `None` to:

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

Delete the doc-comment sentence about the Linux arm returning `None` until Phase B.

- [ ] **Step 2: Verify and commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/audio_toolkit/utils.rs
git commit -m "feat(linux): resolve PipeWire or PulseAudio for loopback capture"
```

---

### Task 4: Linux device enumeration and resolution

**Files:**

- Modify: `src-tauri/src/audio_toolkit/audio/device.rs`
- Modify: `src-tauri/src/audio_toolkit/audio/mod.rs:8` (the `device` re-export)
- Modify: `src-tauri/src/managers/audio.rs` (`get_effective_system_audio_device`)
- Modify: `src-tauri/src/commands/audio.rs` (new enumeration command)
- Modify: `src-tauri/src/lib.rs` (`collect_commands![...]`)

**Interfaces:**

- Consumes: `get_system_audio_host()` (Task 3).
- Produces:
  - `pub fn list_system_audio_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>>`
  - `pub async fn get_available_system_audio_devices() -> Result<Vec<AudioDevice>, String>`
  - `get_effective_system_audio_device` resolving against the loopback host.

Why a separate list: on Linux the loopback host enumerates a **different device set** than `list_output_devices()`'s ALSA host. Sharing the playback selector's list would show names that cannot be matched at capture time.

> **Depends on Task 2's findings.** The code below assumes the loopback device appears in `output_devices()` and that `host.default_output_device()` is openable for input — the shape that holds on Windows. Task 2 exists to verify that on Linux. If it found otherwise, amend the code below to match what you measured before writing it. Do not implement this task without those answers in hand.

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
    let default_name = host.default_output_device().map(|d| d.to_string());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device.to_string();
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

Note `device.to_string()`, not `description()`: cpal 0.18's `description()`
returns a `DeviceDescription` struct, not a `String`, and its `Display` impl is
what cpal's own docs point to for the plain name. `name()` is gone. Match
whatever `list_output_devices` ended up doing in Phase A Task 2 so both
enumerations produce comparable strings — a mismatch here silently breaks
device selection.

- [ ] **Step 2: Re-export it**

In `src-tauri/src/audio_toolkit/audio/mod.rs`, extend line 8:

```rust
pub use device::{
    list_input_devices, list_output_devices, list_system_audio_devices, CpalDeviceInfo,
};
```

- [ ] **Step 3: Point device resolution at the loopback host**

In `src-tauri/src/managers/audio.rs`, rewrite `get_effective_system_audio_device`, removing Phase A's `#[cfg(not(windows))]` stub and unwrapping the `#[cfg(windows)]` block around the real body:

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

Update this file's import from `list_output_devices` to `list_system_audio_devices` (check whether `list_output_devices` is still used elsewhere in the file before removing it).

- [ ] **Step 4: Expose the list to the frontend**

Add to `src-tauri/src/commands/audio.rs`, mirroring `get_available_output_devices`:

```rust
#[tauri::command]
#[specta::specta]
pub async fn get_available_system_audio_devices() -> Result<Vec<AudioDevice>, String> {
    // cpal enumeration can stall; keep it off the webview/main run loop.
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

Register `commands::audio::get_available_system_audio_devices,` in `collect_commands![...]`.

- [ ] **Step 5: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
bun run tauri dev   # briefly, to regenerate src/bindings.ts
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src src/bindings.ts
git commit -m "feat(linux): resolve system audio devices via the loopback host"
```

---

### Task 5: Point the device selector at the loopback device list

**Files:**

- Modify: `src/stores/settingsStore.ts`
- Modify: `src/hooks/useSettings.ts`
- Modify: `src/components/settings/advanced/SystemAudioDeviceSelector.tsx`

**Interfaces:**

- Consumes: `commands.getAvailableSystemAudioDevices()` (Task 4).

Phase A already gated this component on availability; this task fixes _which devices it lists_. Without it the selector shows ALSA playback devices whose names cannot be opened on the loopback host.

- [ ] **Step 1: Add the list to the store**

In `src/stores/settingsStore.ts`, add `systemAudioDevices` state and a `refreshSystemAudioDevices` action, copying the shape of the existing `outputDevices`/`refreshOutputDevices` pair exactly — including how the store, not the component, owns the `"Default"` sentinel (see the comment at `SystemAudioDeviceSelector.tsx:35-45`, which records that duplicating it in the component was a previous bug). Back it with `commands.getAvailableSystemAudioDevices()`.

- [ ] **Step 2: Expose it through the hook**

`SystemAudioDeviceSelector` consumes `useSettings()`, not the store directly, and that hook's interface currently exposes only `outputDevices`. Add `systemAudioDevices` and `refreshSystemAudioDevices` to `src/hooks/useSettings.ts` — both its TypeScript interface and its returned object — following how `outputDevices`/`refreshOutputDevices` are surfaced. Skipping this fails the frontend build.

- [ ] **Step 3: Switch the selector over**

In `src/components/settings/advanced/SystemAudioDeviceSelector.tsx`, destructure `systemAudioDevices`/`refreshSystemAudioDevices` instead of `outputDevices`/`refreshOutputDevices`, and use them wherever the old pair was used.

- [ ] **Step 4: Verify**

```bash
bun run build && bun run lint
```

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "feat(linux): list loopback devices in the system audio selector"
```

---

### Task 6: Enforce availability at startup

**Files:**

- Modify: `src-tauri/src/lib.rs` (around line 171, where persisted settings are read)

**Interfaces:**

- Consumes: `get_system_audio_availability` (Phase A Task 6).

Phase A's command-level path only runs when the user toggles the setting. **Startup does not go through it**: `lib.rs` reads persisted settings directly and constructs the managers, so a settings file carrying `system_audio_enabled = true` from a machine that had a sound server would still configure capture on one that does not.

- [ ] **Step 1: Normalise the persisted setting at startup**

At the point where `settings` is read and before the managers are built:

```rust
    // A settings file can arrive from a machine that had a sound server, or
    // this one may have lost it since. Availability is a property of the
    // running system, not of the saved preference.
    if settings.system_audio_enabled
        && commands::audio::get_system_audio_availability(app.handle().clone()).await
            != commands::audio::SystemAudioAvailability::Available
    {
        log::warn!("System audio was enabled in settings but is unavailable here; disabling");
        let mut settings = settings.clone();
        settings.system_audio_enabled = false;
        write_settings(app.handle(), settings);
    }
```

Read the surrounding startup code first and splice where `settings` is in scope. The command is `async` (Phase A Task 6) — if this startup path is not async, call the inner `availability_from_host_probe(get_system_audio_host().is_some())` directly instead of the command, rather than blocking on a runtime.

- [ ] **Step 2: Verify and commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/lib.rs
git commit -m "feat(linux): disable persisted system audio when unavailable"
```

---

### Task 7: Build, CI, Nix, and packaging dependencies

**Files:**

- Modify: `BUILD.md` (Linux prerequisites)
- Modify: `src-tauri/tauri.conf.json` (`linux.deb.depends`, `linux.rpm.depends`)
- Modify: `.github/workflows/build.yml` (lines 124, 136, 142)
- Modify: `flake.nix` (`commonNativeDeps`, around line 40)

**Interfaces:** none — build and packaging metadata.

Only PipeWire needs native libraries. cpal's PulseAudio backend is pure Rust, so **do not add `libpulse-dev` or a `libpulse.so` runtime dependency** — that would restrict where the package installs for no benefit.

- [ ] **Step 1: Document the dev dependency**

In `BUILD.md`'s Linux prerequisites, add to each distro's install line:

- Ubuntu/Debian: `libpipewire-0.3-dev`
- Fedora/RHEL: `pipewire-devel`
- Arch: `libpipewire`

Note alongside it that PulseAudio support needs no package, since the backend is pure Rust — otherwise someone will "helpfully" add it later.

- [ ] **Step 2: Add the runtime dependency to the packages**

In `src-tauri/tauri.conf.json`, append `"libpipewire-0.3-0"` to `linux.deb.depends`, and `"libpipewire-0.3.so.0()(64bit)"` to `linux.rpm.depends`, matching that array's existing `.so`-versioned style. Read both arrays in full first and append rather than replace.

- [ ] **Step 3: Add the dev package to CI**

Add `libpipewire-0.3-dev` to the `apt-get install` lines at `.github/workflows/build.yml:124`, `:136` and `:142`. Check for other workflows installing Linux build deps (`grep -rn "libasound2-dev" .github/workflows/`) and update those too.

- [ ] **Step 4: Add it to the Nix dev shell**

Add `pipewire` to `commonNativeDeps` in `flake.nix`, alongside the existing `alsa-lib`.

- [ ] **Step 5: Verify a packaged build**

```bash
bun run tauri build -- --bundles deb
dpkg -I src-tauri/target/release/bundle/deb/*.deb | grep -i depends
```

Expected: builds, and `libpipewire-0.3-0` appears. (Skip AppImage if it fails for the pre-existing `linuxdeploy`/`strip` reason documented in `BUILD.md`.)

- [ ] **Step 6: Commit**

```bash
git add BUILD.md src-tauri/tauri.conf.json .github/workflows flake.nix
git commit -m "build(linux): add pipewire build and runtime dependencies"
```

---

### Task 8: Manual verification matrix

No files change. These states cannot be exercised in CI.

- [ ] **PipeWire (default on current Ubuntu/Fedora)**: play audio from another app, enable system audio capture, record, confirm the transcript contains both speakers labelled `me` and `them`.
- [ ] **Device selection**: pick a non-default output in the system-audio selector and confirm capture follows it. Confirm the listed names are the loopback host's, and that selecting one actually opens — this is the cross-host name-matching risk Tasks 4 and 5 exist to avoid.
- [ ] **Follow-stream**: run `handy --follow-stream` during a dual-speaker session and confirm both `"speaker":"me"` and `"speaker":"them"` events appear. No code change should have been needed for this.
- [ ] **PulseAudio only**: `systemctl --user stop pipewire pipewire-pulse wireplumber`, start PulseAudio, confirm the toggle still appears and capture still works via the fallback host.
- [ ] **Neither server**: stop both, confirm the toggle and selector do not render and nothing crashes or hangs.
- [ ] **Startup enforcement**: with no sound server, set `system_audio_enabled = true` directly in the persisted settings file, restart, and confirm Task 6 corrects it (check the log for the "disabling" warning) and no capture is attempted.
- [ ] **Microphone unaffected**: confirm ordinary dictation still works in all three configurations — `get_cpal_host()` was deliberately left on ALSA and must not have regressed.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && bun run lint`.
