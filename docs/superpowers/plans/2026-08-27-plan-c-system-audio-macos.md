# Plan C — system audio capture on macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make system-audio capture actually work on macOS, and — because macOS gives no error when it refuses — make the app able to tell the user that it was refused.

**Architecture:** cpal 0.18 already captures system audio on macOS via the Core Audio Process Tap; Phase A made that machinery compile everywhere and Phase B proved the shape on Linux. Almost nothing is left to do for _capture_. What is left is _permission_, and macOS provides no supported way to observe it. This phase adds a small fork-only module that reads the permission through TCC's private preflight SPI, behind a Cargo feature, and wires that answer into the availability enum Phase A already plumbed to the UI.

**Tech Stack:** Rust, `cpal` 0.18.x, Core Audio Process Tap, TCC private SPI (`dlopen`/`dlsym`), Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Phase 3 of 3, on the same branch as Plans A and B.**

---

## Revision history

**Rewritten 2026-08-27** after research into how real Core Audio tap consumers
handle permission. The previous draft was built on the assumption that a denied
tap fails to open, so that a failed open could be classified as a denial. **That
assumption is false.** Everything downstream of it — the old Task 1b spike, the
`probe_system_audio` helper, `observe_probe_outcome`, and the
`MacosSystemAudioCaptureState` observation — has been deleted rather than
adapted, because the signal they were built to read does not exist.

The old draft anticipated this. Its Task 1b said: _"Open succeeds, samples are
silent → an open-time flag cannot classify… the design must drop automatic
detection and always offer the 'grant access' affordance."_ The research
confirms that branch, and also found a better option than the fallback that task
proposed: a real permission read, via the same private SPI every shipping app in
this space uses.

---

## What the research established

This section is the evidence base. Do not re-derive it; do not "fix" the design
back toward the old assumption.

**A denied tap does not fail. It succeeds and delivers silence.**
`AudioHardwareCreateProcessTap`, the aggregate device, and the stream start all
return `noErr`; the data callback fires at its normal cadence; every sample is
zero. Sources, strongest first:

- **SuperKenVery**, author of cpal's macOS loopback backend, on
  [cpal PR #894](https://github.com/RustAudio/cpal/pull/894#issuecomment-3823323664):
  _"You silently get denied, and record complete silence."_ — describing an
  explicitly not-granted state, not a missing-Info.plist state.
- **roderickvd**, cpal maintainer, on [PR #1124](https://github.com/RustAudio/cpal/pull/1124),
  designing a non-blocking preflight because it _"catches the silent-silence
  bug"_ — i.e. because the OS raises no error.
- **Chromium** creates tap, aggregate device and IOProcID successfully without
  permission, then runs a _separate_ probe before reporting
  `kFailedSystemPermissions` (`media/audio/mac/catap_audio_input_stream.mm`). If
  creation failed on denial, that probe would not exist.

**cpal cannot report it either.** `ErrorKind::PermissionDenied` is produced from
exactly two inputs (`cpal-0.18.2/src/host/coreaudio/mod.rs:107-110`):
`AudioUnit(AudioUnitError::Unauthorized)` and `Audio(AudioError::FilePermission)`.
Both are AudioUnit/file paths. `AudioHardwareCreateProcessTap` is a **HAL** call,
and coreaudio-rs has no `kAudioHardware*` table at all — so no HAL status can
ever reach `PermissionDenied`. Nor can the message be matched: Apple reuses
four-char codes between `AudioHardwareBase.h` and `AudioCodec.h`, so
`kAudioHardwareIllegalOperationError` surfaces as the string `"Illegal
operation"` and `kAudioHardwareUnspecifiedError` as `"Unspecified"` — codec
labels on HAL failures. The error callback cannot report it either: the
coreaudio backend only ever emits `Xrun`, `DeviceNotAvailable`,
`StreamInvalidated`, `DeviceChanged` and `ResourceExhausted`.

**There is no public API to read the permission.** From
[insidegui/AudioCap](https://github.com/insidegui/AudioCap)'s README, the
reference implementation for this API: _"There's no public API to request audio
recording permission or to check if the app has that permission."_ There is no
`AVCaptureDevice.authorizationStatus(for:)` analogue and no
`CGPreflightScreenCaptureAccess` analogue for audio capture.

**So every shipping consumer uses TCC's private preflight SPI**, and gates
_before_ opening rather than classifying afterwards: `thewh1teagle/vibe` (Tauri +
cpal, the closest analogue to this app), `afonsojramos/muesly`,
`insidegui/AudioCap`, `jameshball/osci-render`, plus the 115 files GitHub code
search returns for `check_system_audio_permission language:rust`. cpal's own
[PR #1257](https://github.com/RustAudio/cpal/pull/1257) does the same thing and
is still unmerged; vibe runs a fork of cpal to get it.

**Why we implement it ourselves rather than forking cpal.** We already carry a
rodio fork; a second forked dependency in the same chain is real maintenance
cost for ~60 lines. It also lets us bind only the safe half of the SPI: cpal's
PR additionally binds `TCCAccessRequest`, which needs an Objective-C block, and
we have no use for it (see below).

**The App Store objection does not apply to this app.** Private SPI disqualifies
a Mac App Store submission, but Shorthand cannot ship there regardless: it
depends on `rdev` for global keyboard hooks and `enigo` for synthetic keystrokes
into other applications, neither of which works under the App Sandbox that MAS
requires. `SIGNING_AND_UPDATES.md` plans distribution around Tauri's own updater
with Developer ID signing. The Cargo feature in Task 2 exists so the decision
stays reversible, not because a store build is planned.

**Two findings that shape the implementation, from reading the reference
projects' actual FFI code.**

_Bind `TCCAccessPreflight` only; do not bind `TCCAccessRequest`._ The consent
dialog is raised by starting the tap, not by any permission API — confirmed
independently: _"`AudioDeviceStart` is the call that triggers the TCC prompt.
Not creating the tap, aggregate device, or IOProc."_ `afonsojramos/muesly`
therefore never binds `TCCAccessRequest` at all: it attempts capture and polls
the preflight for the answer. Skipping it removes the only genuinely dangerous
part of this work. `TCCAccessPreflight` is a plain
`extern "C" fn(*const c_void, *const c_void) -> i32`; `TCCAccessRequest` takes
an **Objective-C block**, which is not a C function pointer but a struct with a
layout and copy semantics, and getting it wrong is undefined behaviour rather
than a compile error. cpal PR #1257's binding has to launder a channel sender
through a raw `usize` _"so TCC's internal block memcpy doesn't double-drop the
sender"_ — a trap we simply do not need to walk into. Not binding it also means
we need no `block2` dependency.

_The preflight tells us granted-or-not, not why._ Sources disagree on the
finer mapping. AudioCap and muesly read `0 => granted, 1 => denied, _ =>
undetermined`; cpal checks only `== 0`; and `jameshball/osci-render` warns in a
comment that _"on some OS builds, preflight may return the same code for both
'not determined' and 'denied'."_ Nobody cites a stable contract for `1` versus
`2`. So treat the result as **granted / not-granted** and do not branch on the
difference. This is a UI constraint as much as a code one: we cannot tell a
user who declined from a user who was never asked, so the denied-state copy
must read correctly for both.

**Correction to the spec.** The spec's Task 7 premise about the menu-bar
indicator is inverted. Per Apple Support: **orange** is the microphone,
**green** the camera, **purple** system-audio recording. ScreenCaptureKit also
produces purple, so the dot does _not_ prove we are off the ScreenCaptureKit
path. The real tell is that the app appears under **Privacy & Security → Screen
& System Audio Recording → System Audio Recording Only**.

---

## Global Constraints

- **Windows and Linux behaviour must not change.** Everything in this phase is
  `#[cfg(target_os = "macos")]` or additive.
- **Never accuse the user of declining something they were not asked.**
  Not-granted is not a denial: the preflight cannot tell a refusal from a
  prompt never shown, so it maps to `Available` and a capture attempt is what
  resolves it. Only an attempt that failed to change the answer earns the CTA.
- **Never persist an enabled state the OS refused.** The permission read is
  process-local; a persisted `true` over a denial produces a checked toggle that
  captures nothing on every subsequent launch.
- **Do not block the UI thread.** The consent dialog is modal and the user may
  ignore it indefinitely. Nothing that waits on it may run on the main thread;
  everything here goes through `spawn_blocking`. (An earlier draft of this plan
  cited a Chromium-documented 60-second timeout here. That claim could not be
  substantiated — a search of Chromium's macOS audio sources found nothing of
  the kind — so do not encode a 60-second constant on the strength of it. Bound
  any wait with a timeout of our own choosing instead, and say it is ours.)
- **Private SPI must fail open, never panic.** If `dlopen` or `dlsym` fails —
  Apple moves the symbol, a future OS drops it — the result is `NotGranted`,
  which degrades to exactly the behaviour we would have had without the SPI at
  all. A missing private symbol must never break dictation.
- All `cargo` commands run from `src-tauri/` or use `--manifest-path
src-tauri/Cargo.toml` — there is no `Cargo.toml` at the repo root.
- **On this Windows dev machine**, MSBuild's FileTracker is broken; prefix cargo
  commands that build native deps with `TrackFileAccess=false`. Irrelevant on
  macOS.

---

### Task 1: The consent string — ALREADY DONE

`src-tauri/Info.plist` already carries:

```xml
<key>NSAudioCaptureUsageDescription</key>
<string>Shorthand records audio playing on this Mac so it can transcribe the other side of a call or meeting. Audio is transcribed locally.</string>
```

- [x] Landed in commit `48623a5`.

Verify it survives bundling (Task 6 covers this): the key must be in the **built
app bundle's** Info.plist, not merely the source file. TCC reads the bundle.

---

### Task 2: The TCC preflight module

**Files:**

- Create: `src-tauri/src/system_audio_permission.rs`
- Modify: `src-tauri/src/lib.rs` (`mod` declaration)
- Modify: `src-tauri/Cargo.toml` (the feature and the macOS-only dependency)

**Interfaces:**

- Produces:
  - `pub enum SystemAudioPermission { Granted, NotGranted }` —
    `Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type`,
    `#[serde(rename_all = "snake_case")]`
  - `pub fn preflight() -> SystemAudioPermission` — non-blocking, shows no UI.

Two states, not three, and no `request()`. Both follow from the research
section: the preflight cannot reliably separate "declined" from "never asked",
and the prompt is raised by starting the tap rather than by an API, so
`TCCAccessRequest` — the half that would need an Objective-C block — is never
bound. If a future macOS documents a stable denied-versus-undetermined code,
adding the third state is a small change; guessing at it now is not.

A new fork-only module, per `AGENTS.md`'s "give fork-only features a boundary" —
this costs nothing at merge time and keeps the private-SPI surface in one file
that is easy to find and easy to delete.

- [ ] **Step 1: Add the Cargo feature**

In `src-tauri/Cargo.toml`:

```toml
[features]
default = ["macos-tcc-spi"]
# Reads the system-audio permission through TCC's private preflight SPI. There
# is no public API for this (see the plan's research section), and without it a
# denied tap is indistinguishable from a silent room. Disqualifying for the Mac
# App Store, which this app cannot target anyway — the flag exists so that
# decision stays reversible. With the feature off, permission always reads
# NotGranted and the UI falls back to offering the settings link
# unconditionally.
macos-tcc-spi = []
```

Add to `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
libloading = "0.8"
objc2-core-foundation = "0.3"
```

Both already resolve in `Cargo.lock` (0.8.9 and 0.3.2) as transitive
dependencies, so declaring them directly should not move any other version —
confirm that with a build rather than assuming. `objc2` and `objc2-foundation`
are already direct macOS dependencies.

**Do not add `block2`.** It is only needed for `TCCAccessRequest`'s completion
block, which this plan deliberately does not bind.

Note `objc2-core-foundation` 0.3.2 exports no `CFStringRef` alias — use
`CFString::from_str(...)` and cast `&*service as *const _ as *const c_void` at
the boundary, as cpal's PR does. The binding holds the +1 retain for the
duration of the call and releases on drop, so there is no manual `CFRelease`.

- [ ] **Step 2: Write the failing tests**

The SPI itself cannot be unit-tested — it needs a real TCC daemon and a real
bundle identity. Test the pure mapping instead, which is where the logic that
can be wrong actually lives:

```rust
#[cfg(test)]
mod system_audio_permission_tests {
    use super::*;

    #[test]
    fn zero_is_granted() {
        assert_eq!(
            SystemAudioPermission::from_preflight_status(0),
            SystemAudioPermission::Granted
        );
    }

    #[test]
    fn every_other_status_is_not_granted() {
        // Only `0 == granted` is agreed across the reference implementations.
        // osci-render warns that some OS builds return the same code for
        // "denied" and "not determined", so no other value may be read as
        // meaning anything in particular — least of all as a denial we would
        // then show the user.
        for status in [1, 2, 3, -1, i32::MAX, i32::MIN] {
            assert_eq!(
                SystemAudioPermission::from_preflight_status(status),
                SystemAudioPermission::NotGranted,
                "status {status} should be not-granted"
            );
        }
    }
}
```

- [ ] **Step 3: Run them and watch them fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_permission_tests
```

- [ ] **Step 4: Implement**

Model on `insidegui/AudioCap`'s `AudioRecordingPermission.swift` and
`afonsojramos/muesly`'s `permissions.rs`. The shape:

```rust
//! Reads the macOS system-audio recording permission (`kTCCServiceAudioCapture`).
//!
//! macOS ships the Core Audio Process Tap API with no public way to query or
//! request its permission, and a tap without that permission does not fail —
//! it delivers all-zero samples indefinitely. Without this module the app
//! cannot tell "the user declined" from "the room is quiet", so a declined
//! prompt would strand the user with a toggle that appears on and captures
//! nothing, with no way back (macOS never prompts twice).
//!
//! So we read TCC's preflight SPI, as every shipping consumer of this API
//! does. It is private, and it fails open: any failure to load the symbol
//! yields `NotGranted`, which behaves exactly as if this module were absent —
//! the capture attempt still raises the prompt, and the settings link is still
//! offered.

use libloading::{Library, Symbol};
use objc2_core_foundation::{CFRetained, CFString};
use std::{ffi::c_void, sync::OnceLock};

const TCC_FRAMEWORK_PATH: &str =
    "/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
const SERVICE_AUDIO_CAPTURE: &str = "kTCCServiceAudioCapture";

/// `int TCCAccessPreflight(CFStringRef service, CFDictionaryRef options)`.
///
/// AudioCap's Swift declares the return as `Int` (64-bit), but every non-Swift
/// reimplementation — cpal, muesly, osci-render — types it as a 32-bit `int`,
/// and we only ever compare it against 0. Follow the C spelling.
type PreflightFn = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;
```

Requirements, each of which is load-bearing:

- **`from_preflight_status` is a separate pure function**, so the tests above
  can reach it. `0 => Granted`, everything else `=> NotGranted`.
- **Every failure path returns `NotGranted`**: `dlopen` returning null, `dlsym`
  returning null, a `CFString` that fails to construct. Log at `warn!` once, not
  per call — cache the loaded `Library` in a `OnceLock<Option<Library>>` so a
  failed load is remembered rather than retried on every probe.
- **When the `macos-tcc-spi` feature is off**, `preflight()` is a
  `#[cfg(not(feature = "macos-tcc-spi"))]` stub returning `NotGranted`, with no
  `dlopen` compiled in at all. That degrades to "always offer the settings
  link", which is a coherent product rather than a broken one.
- **The whole module is `#[cfg(target_os = "macos")]`** at the `mod` site in
  `lib.rs`.

cpal PR #1257's `permissions.rs` is the closest working model — copy its
`OnceLock` loading and its `CFString::from_str` plus
`&*service as *const _ as *const c_void` cast near-verbatim, and simply omit its
`request_system_audio_permission` half.

- [ ] **Step 5: Run the tests and watch them pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_permission_tests
```

Expected: PASS (2 tests). These pass on every platform, because the mapping is
pure; only the SPI is macOS-gated.

- [ ] **Step 6: Verify and commit**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings \
  && cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

```bash
git add src-tauri/Cargo.toml src-tauri/src/system_audio_permission.rs src-tauri/src/lib.rs
git commit -m "feat(macos): read the system audio permission via TCC preflight"
```

---

### Task 3: Fold permission into availability, and request it on enable

**Files:**

- Modify: `src-tauri/src/commands/audio.rs`

**Interfaces:**

- Consumes: `system_audio_permission::{preflight, request}` (Task 2).
- Modifies: `get_system_audio_availability` gains its macOS branch;
  `change_system_audio_enabled_setting` and
  `change_dictation_system_audio_enabled_setting` gain the request-on-enable.

Note there is **no managed state to register**. The old draft cached an observed
outcome in a `Mutex` because observing required an expensive open attempt. A
preflight is cheap and always current, so caching it would only create a way for
the answer to go stale — read it each time.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod macos_system_audio_availability_tests {
    use super::*;
    use crate::system_audio_permission::SystemAudioPermission;

    #[test]
    fn granted_is_available() {
        assert_eq!(
            availability_from_permission(SystemAudioPermission::Granted),
            SystemAudioAvailability::Available
        );
    }

    #[test]
    fn not_granted_is_available_until_an_attempt_has_proved_otherwise() {
        // `NotGranted` covers both "declined" and "never asked", and the
        // preflight cannot separate them. Reporting a denial here would show
        // the "grant access" CTA to a user who was never prompted, and hide
        // the toggle that is the only thing that would prompt them. The CTA is
        // earned by a capture attempt that failed to change this answer, which
        // is Task 3 Step 4's job — never by the probe alone.
        assert_eq!(
            availability_from_permission(SystemAudioPermission::NotGranted),
            SystemAudioAvailability::Available
        );
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_availability_tests
```

- [ ] **Step 3: Implement the availability branch**

Phase A left `let _ = &app;` in `get_system_audio_availability` as the seam.
Replace it with a macOS arm. The Linux/Windows arm stays exactly as it is.

```rust
/// Availability from the current permission state. Split out pure so it is
/// testable without a TCC daemon.
#[cfg(target_os = "macos")]
fn availability_from_permission(
    permission: crate::system_audio_permission::SystemAudioPermission,
) -> SystemAudioAvailability {
    use crate::system_audio_permission::SystemAudioPermission;
    match permission {
        // Both states render the ordinary toggle. `PermissionDenied` is
        // reported by the enable path in Step 4, once an attempt has proved
        // the OS will not grant it — never by this probe, which cannot tell a
        // refusal from a prompt that has not been shown yet.
        SystemAudioPermission::Granted | SystemAudioPermission::NotGranted => {
            SystemAudioAvailability::Available
        }
    }
}
```

The macOS arm of the command reads `preflight()` inside `spawn_blocking` — it is
cheap, but it is still a `dlopen`ed call into a system daemon and this command is
already async for the same reason on Linux.

- [ ] **Step 4: Raise the prompt by attempting capture, then re-read**

In **both** `change_system_audio_enabled_setting` and
`change_dictation_system_audio_enabled_setting` — they share
`set_system_audio_enabled_for_scope`, so put this in the shared path rather than
writing it twice.

There is no API that shows the prompt, so we cause it the way macOS actually
raises it: by starting the tap. The flow, on macOS only, when `enabled == true`:

```
preflight()
  Granted     -> persist enabled = true, done
  NotGranted  -> open the loopback stream AND play() it   // this is what prompts
                 poll preflight() until it changes, or until our timeout
                   Granted    -> persist enabled = true
                   NotGranted -> persist enabled = false,
                                 availability reports permission_denied
```

Requirements:

- **The attempt must reach `play()`.** Building the stream is not enough — the
  prompt is raised by starting IO on the tap-backed aggregate device. Tear the
  stream down afterwards; a probe must never leave capture running.
- **Poll; do not wait on a callback.** The dialog is asynchronous and the user
  may take seconds to answer. Poll `preflight()` on an interval (muesly uses
  500ms) up to a bounded timeout that is **ours to choose** — no source
  justifies a particular number, so pick one, name the constant, and say in a
  comment that it is our choice rather than an OS contract.
- **A timeout is not a denial.** If the answer has not changed when polling
  stops, the dialog is most likely still open. Persist `false` so nothing
  claims to be capturing, but prefer copy that invites another attempt over
  copy asserting the user refused.
- **Do not persist a refused enable.** Write `false`, and let the frontend's
  re-read (Phase A Task 6 Step 7, confirmed working in both scopes) pick up the
  corrected value.
- **Return `Ok`, not `Err`, on a refusal.** This is not a command failure — the
  user made a choice. The UI reflects it through availability, not an error
  toast.
- **All of it goes through `spawn_blocking`**, and none may run on the main
  thread: it opens cpal streams and waits on a modal dialog.
- **Disabling never prompts.** Only `enabled == true` touches permission.
- Beware the settings lost-update pattern being fixed separately in
  `commands/audio.rs`: re-read settings immediately before writing, and mutate
  only the field this command owns. Do not reintroduce a stale full-snapshot
  write across these new `await` points.

- [ ] **Step 5: Run the tests and watch them pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_availability_tests
```

Expected: PASS (2 tests).

- [ ] **Step 6: Verify and commit**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings \
  && cargo test --manifest-path src-tauri/Cargo.toml
```

```bash
git add src-tauri/src/commands/audio.rs
git commit -m "feat(macos): prompt for system audio permission when it is enabled"
```

---

### Task 4: Guard against the silent loopback downgrade

**Files:**

- Modify: `src-tauri/src/managers/audio.rs` (`get_effective_system_audio_device`,
  macOS arm)

**Interfaces:** none — a guard and a warning.

**Why this exists.** cpal decides between loopback and ordinary input capture on
one condition (`cpal-0.18.2/src/host/coreaudio/macos/device.rs:723-735`):

```rust
let mut audio_unit = if self.supports_input() {
    audio_unit_from_device(self, AudioUnitMode::Input)?
} else {
    loopback_aggregate.replace(LoopbackDevice::from_device(self)?);
    ...
```

and `supports_input()` is simply "does this device report any input config"
(`:685-690`). So an output device that _also_ has inputs — a USB interface, an
existing aggregate device, BlackHole, a Bluetooth headset in HFP mode — takes the
**ordinary input** branch and records that device's microphone inputs instead of
the system mix. Silently: no error, and nothing in the cpal API distinguishes the
two outcomes afterwards.

Phase B deliberately lets the user pick a system-audio device, so this is
reachable. Note `thewh1teagle/vibe` sidesteps it entirely by hardcoding
`default_output_device()` on macOS and ignoring the user's selection — an option
if the guard proves insufficient, but it silently discards a setting the UI
still shows, so prefer the guard.

_This trap is inferred from reading cpal's source; no project bug report of it
was found. Verify the behaviour in Task 6 before relying on the guard's
diagnosis._

- [ ] **Step 1: Refuse a device that would not loop back**

In the macOS arm of `get_effective_system_audio_device`, after resolving a
candidate device and before wrapping it in `SystemAudioCapture`, check whether it
reports any supported input config. If it does, cpal will not take the loopback
path: log a `warn!` naming the device and explaining that it would capture that
device's inputs rather than system output, and return `None` so capture runs
microphone-only rather than silently recording the wrong source.

Falling back to the default output device instead of `None` is tempting and
wrong: the user picked a device, and quietly substituting a different one is the
same class of dishonesty this whole phase exists to remove.

- [ ] **Step 2: Verify and commit**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

```bash
git add src-tauri/src/managers/audio.rs
git commit -m "fix(macos): refuse a system-audio device that would capture its inputs"
```

---

### Task 5: The denied-state UI and the settings link

**Files:**

- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (`collect_commands![...]`)
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx`
- Modify: `src/components/settings/advanced/DictationSystemAudioCapture.tsx`
- Modify: `src/shorthand/locales/en.json`

**Interfaces:**

- Produces: `pub fn open_system_audio_privacy_settings() -> Result<(), String>`

- [ ] **Step 1: Add the settings-link command**

Next to the existing `open_microphone_privacy_settings`:

```rust
#[tauri::command]
#[specta::specta]
pub fn open_system_audio_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // The audio-capture pane anchor, as used by cpal PR #1257. Task 6
        // records which pane this actually opens; if it lands somewhere too
        // general, tighten it there rather than guessing here.
        Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AudioCapture")
            .spawn()
            .map_err(|e| format!("Failed to open privacy settings: {e}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    Err("Opening system audio privacy settings is only supported on macOS".to_string())
}
```

Register it in `collect_commands![...]`, then regenerate `src/bindings.ts` with a
brief `bun run tauri dev`.

- [ ] **Step 2: Add the strings**

Fork-only keys go in `src/shorthand/locales/en.json`, never `src/i18n/locales/` —
`bun run check:locale-drift` enforces this. Read
`src/shorthand/locales/README.md` first and match the file's key convention:

```json
"settings.advanced.systemAudio.permissionNeeded": "Shorthand needs permission to record audio playing on this Mac. Grant it in System Settings, under Privacy & Security -> Screen & System Audio Recording. macOS only asks once, so the prompt may not appear again.",
"settings.advanced.systemAudio.tryAgain": "Try again"
```

Note what that copy does _not_ say. An earlier draft of this step read "so if
you declined, you can grant it" — which contradicts this plan's own constraint
never to accuse the user of declining something they were not asked. The same
state is reached by refusing the dialog, by ignoring it until our timeout, and
by never being shown one at all, and we cannot tell those apart. It also names
the pane: the row lives under Screen & System Audio Recording, which is not
where anyone looks for an audio-only permission.

Reuse the existing `accessibility.openSettings` key for the settings button —
it already labels the Windows microphone-permission button — rather than adding
a second "Open Settings" string.

- [ ] **Step 3: Add the denied branch to both toggles**

Both `SystemAudioCapture.tsx` and `DictationSystemAudioCapture.tsx` currently
gate on `null || "unavailable_no_sound_server"` and let `permission_denied` fall
through to the ordinary, fully-enabled UI — a toggle that spins, round-trips and
silently reverts. Add a `permission_denied` branch to each, rendering the
explanation, an **Open Settings** button and a **Try again** button.

**The retry button is not optional.** This branch replaces the toggle, so
without it a user who grants permission in System Settings has no way back into
the app's own flow.

Two things the review of Phase A's UI established, which this branch must not
regress:

- The two components deliberately duplicate their gate rather than sharing a
  hook; a shared denied-state component is fine, but do not refactor the gates.
- Availability must stay visible while re-probing. `null` means "never
  answered", and `isProbingSystemAudio` carries in-flight state — do not blank
  availability to show a spinner.

Wire **Try again** to re-read availability rather than to write the setting
directly: with a real preflight, granting in System Settings changes the answer
without any capture attempt, so a refresh alone resolves it. That is a genuine
simplification over the old draft, which had to re-attempt an enable because an
attempt was the only way to learn anything.

- [ ] **Step 4: Verify the gates**

```bash
bun run build && bun run lint && bun run check:translations \
  && bun run check:locale-drift && bun run check:fork-translations \
  && bun run check:settings
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src src/bindings.ts src/components src/shorthand
git commit -m "feat(macos): offer a way back when system audio is denied"
```

---

### Task 6: Manual verification matrix

No files change. None of this can run in CI. Run on a real Mac at macOS 14.6+.

**Read this before you start.** TCC keys its permission record to a stable
signing identity, and the _responsible_ process must carry
`NSAudioCaptureUsageDescription`. A `cargo run` or ad-hoc-signed dev build can
silently inherit the grant of whatever launched it — one project's reference
probe was run from Terminal and inherited _Terminal's_ grant, hiding the bug from
the first commit. **Test a signed `bun run tauri build` bundle launched from
Finder**, not a dev build launched from a terminal.

- [ ] **The consent string reaches the bundle.** Run
      `plutil -p Shorthand.app/Contents/Info.plist | grep NSAudioCapture` against
      the built bundle and confirm it prints the Task 1 string. If the key is
      absent, TCC hard-denies with no prompt at all, and every test below it is
      measuring nothing.
- [ ] **First-run consent fires at the toggle.** Reset with
      `tccutil reset AudioCapture <bundle-id>`, leave the app in its default
      on-demand microphone mode, and enable system audio **without recording
      anything**. The dialog must appear immediately, showing the Task 1 string
      verbatim.
- [ ] **A never-asked state does not render the denied CTA**: after the reset and
      _before_ touching the toggle, confirm the ordinary toggle renders, not the
      "you declined" branch.
- [ ] **Deny path**: decline the prompt. Confirm the CTA renders, and that the
      toggle did **not** persist as enabled.
- [ ] **Denial survives restart honestly**: restart and confirm the CTA still
      renders and the toggle still reads off — not on-but-broken. This is the
      failure the whole phase exists to prevent.
- [ ] **Denied does not re-prompt**: with permission denied, click the toggle
      path again and confirm no dialog appears (macOS will not show one) and the
      app does not hang waiting for one.
- [ ] **Re-grant path**: grant in System Settings, return to the app, click **Try
      again**, and confirm availability flips to available and the toggle
      returns — with **no** capture attempt required. This is the path that
      strands users if the retry button is missing.
- [ ] **Settings link**: click it and record **which pane actually opens** and
      whether the audio-capture row is reachable. Tighten the URL in Task 5 if a
      better anchor exists; if not, confirm the copy is enough to guide the user.
- [ ] **Both speakers transcribe**: with permission granted, play audio from
      another app, record, and confirm the transcript carries `me` and `them`
      lanes as on Windows.
- [ ] **The right permission, not screen recording**: confirm the app appears
      under **Privacy & Security → Screen & System Audio Recording → System
      Audio Recording Only**. Confirm a **purple** menu-bar dot while capturing —
      but note purple is also what ScreenCaptureKit produces, so the Settings
      row is the authoritative check, not the dot.
- [ ] **The `supports_input` guard (Task 4)**: select a system-audio device that
      also has inputs — a USB interface, BlackHole, or an aggregate device — and
      confirm the warning fires and capture falls back to microphone-only,
      rather than silently recording that device's inputs. If cpal in fact loops
      back correctly from such a device, record that and relax the guard.
- [ ] **Follow-stream**: run `handy --follow-stream` during a dual-speaker
      session; confirm both `"speaker":"me"` and `"speaker":"them"` appear. No
      code change should have been needed.
- [ ] **Microphone unaffected**: ordinary dictation works with system audio both
      on and off, in both on-demand and always-on microphone modes.
- [ ] **Feature-off build still works**: build with `--no-default-features`
      (or with `macos-tcc-spi` disabled) and confirm permission reads
      undetermined, the toggle still prompts on first use via the capture
      attempt, and nothing crashes. This is the escape hatch; it must not rot.
- [ ] **Long-session check (decides Task 7)**: leave capture enabled and idle for >15 minutes with nothing playing, then play audio and record. If it still
      captures, write that result into Task 7 so nobody re-litigates it.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check &&
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings &&
bun run lint`.

---

### Task 7: Silent-tap watchdog — DEFERRED, do not implement

**Status: cut**, for the reasons the previous draft recorded, which still stand.
An earlier design rebuilt the tap after ten minutes without system-audio samples,
working around a report that Process Taps can degrade to all-zero buffers after
long uptime. Review showed that design was actively harmful:

- **It measured the wrong signal** — `with_system_audio_callback` is fed
  _post-VAD_ and returns early while not recording (`recorder.rs:1289`), so an
  enabled-but-idle app looks permanently silent and the watchdog fires on healthy
  systems as a matter of course.
- **Firing is destructive** — recovery called `start_microphone_stream()`,
  defeating on-demand microphone closure, and mid-session would restart both
  streams and discard the recording in progress.
- **The bug is unconfirmed on this codebase.**

The research adds two further reasons to leave it cut, and a correction:

- **Zeros are not a fault signal.** screenpipe encodes this as policy —
  _"Zero-filled callbacks are healthy delivery of legitimate silence"_ — and does
  not rebuild on zeros, only on a complete callback stall. An Apple Developer
  Forums thread reaches the same conclusion: all-zero buffers from a broken tap
  are indistinguishable from a muted participant or a paused video.
- **With Task 2 in place, the main motivation is gone.** The watchdog was the
  only way to notice a denial; a preflight notices it up front and cheaply.
- **But the degradation report is real and separate.** Apple Developer Forums
  thread 825780 reports a _granted_ tap going all-zero for minutes mid-session on
  macOS 26.5, recovered only by full teardown. Do not attribute that to
  permissions — it is a distinct bug.

If Task 6's long-session check shows real degradation, implement it properly:
measure at the raw loopback callback or the pump (which sees every frame
regardless of VAD and recording state); distinguish "delivering silence" from
"stopped delivering" and act only on the latter; never restart while
`is_recording()`; and rebuild only the system lane, which needs a per-lane reopen
on `AudioRecorder` that does not exist yet.
