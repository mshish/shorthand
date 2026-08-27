# System audio capture for Linux and macOS

Status: draft
Date: 2026-08-26

## Problem

System (output) audio loopback capture — recording what's currently playing
through the machine's speakers, mixed alongside the microphone as a second
VAD lane — exists today only on Windows. `audio_toolkit/audio/recorder.rs`
defines `SystemAudioCapture` and gates it and its call sites end-to-end with
`#[cfg(windows)]`:

- `audio_toolkit/audio/recorder.rs` — the `SystemAudioCapture` struct and its
  `open()` support
- `managers/audio.rs` — the second VAD lane, `pending_system_audio`,
  `system_stream_router`, `get_effective_system_audio_device`,
  `update_system_audio_capture`
- `managers/transcription.rs` — `SystemAudioTranscription`, `StreamSource::System`
- `commands/audio.rs` — `change_system_audio_enabled_setting` and
  `set_system_audio_device` hard-error with "System audio capture is only
  available on Windows" under `#[cfg(not(windows))]`
- `src/components/settings/advanced/SystemAudioCapture.tsx` — hides the
  toggle entirely unless `useOsType() === "windows"`

`list_output_devices()` (`audio_toolkit/audio/device.rs`) is already
cross-platform and unaffected.

The Windows implementation captures by opening a cpal **output** `Device` in
input mode (WASAPI loopback) — a device-open-time trick, not a different
architecture. The goal of this work is to bring Linux and macOS up to the
same capability using that same dual-lane shape, not to invent a
platform-specific pipeline per OS.

## Research findings

Two independent web-research passes (Aug 2026) established the following.
Full findings and citations are in the conversation this spec came from; the
load-bearing conclusions:

**macOS**: `cpal` itself added native macOS loopback support in v0.17.0
(released 2025-12-20; current 0.18.2), built on Apple's Core Audio Process
Tap API (`AudioHardwareCreateProcessTap`/`CATapDescription`, introduced
macOS 14.2, stable per cpal's own compatibility table from **macOS 14.6**).
The usage pattern is identical to the existing Windows code: call
`build_input_stream` on an output `Device`; cpal detects this and builds the
aggregate-device/process-tap machinery internally. No custom Objective-C/Swift
shim is needed.

Permission is a dedicated, one-time TCC prompt under `kTCCServiceAudioCapture`
(distinct from Screen Recording), tied to an `NSAudioCaptureUsageDescription`
Info.plist string. Unlike the Windows registry-based precheck, **there is no
public API to precheck or proactively request this permission** — it fires
implicitly on first capture attempt. Denial requires guiding the user to
System Settings; the exact pane/row for this specific TCC bucket was not
confirmed by research (Screen Recording's deep link does not apply here,
since this is a different permission).

The alternative, ScreenCaptureKit's `SCStreamConfiguration.capturesAudio`,
works down to macOS 13 but rides the Screen Recording TCC grant — a heavier,
more alarming permission to ask for (a persistent screen-recording indicator)
for what is, to the user, "just" audio. Given the explicit goal of a simple
permission ask, and that cpal already gives us the Process Tap path for
free, ScreenCaptureKit is rejected as the primary path.

A virtual-driver approach (BlackHole-style) is rejected outright: it requires
a separate installer and manual Multi-Output Device configuration in Audio
MIDI Setup, which fails the "simple to grant access" requirement outright.

Known risk: Apple's Process Tap API has an open, unresolved bug where taps
can silently degrade to all-zero PCM buffers after extended uptime, requiring
a teardown/rebuild to recover. cpal has landed several loopback fixes since
0.17.0 but this particular issue was still open in upstream Apple forums as
of research time.

**Linux**: cpal gained native `pipewire` and `pulseaudio` Cargo features
(both off-by-default) in the same 0.18.0 release, landing PRs merged
2026-02-19 and 2026-03-02. When both are compiled in, cpal prioritizes
PipeWire > PulseAudio > ALSA — the same backend-priority order the codebase
already uses for mute control (`wpctl` > `pactl` > `amixer` in
`managers/audio.rs`'s `set_mute`/`get_mute`). The PipeWire backend opens a
capture stream against the default sink's monitor via PipeWire's own
`STREAM_CAPTURE_SINK` mechanism — the direct native equivalent of WASAPI
loopback. The PulseAudio backend uses a from-scratch pure-Rust protocol
implementation (not `libpulse-binding`, which is comparatively stale) and
picks up monitor sources the same way `pactl list sources` does, working
identically whether the actual server is real PulseAudio or PipeWire's
`pipewire-pulse` compatibility shim.

Plain ALSA enumeration does **not** expose per-sink monitor sources — reaching
them requires going through the PipeWire or PulseAudio client protocol, which
is exactly what these new cpal features do. This confirms a new capture path
is required; it cannot be added by extending the existing ALSA-only ID
enumeration.

No permission prompt applies. PipeWire's portal-based access control
(`xdg-desktop-portal`) is a Flatpak/Snap sandboxing mechanism; a non-sandboxed
`.deb`/`.rpm`/AppImage process talking to the PipeWire or PulseAudio socket
directly gets unrestricted access by default — identical exposure to today's
microphone capture.

Systems running neither PipeWire nor PulseAudio (rare, minimal-WM setups)
have no supported path; the ALSA `snd-aloop` workaround requires the user to
have already manually rerouted their entire system audio output, which
cannot be set up or reliably detected by the app, so this case is treated as
"feature unavailable," not "attempt anyway."

Risk noted: the cpal PipeWire/PulseAudio backend is young — merged
Feb/Mar 2026, first released Jun 2026, and still receiving near-weekly fixes
as of the research date. Treat as promising, not battle-tested.

## Decisions

1. **Two independent specs/plans, not one.** Per `AGENTS.md`'s "give
   fork-only features a boundary" / "keep the diff mergeable" guidance, Linux
   and macOS support are unrelated capabilities that happen to touch the same
   files. Either could ship without the other. This design covers both, but
   downstream implementation plans are separate documents so each can be
   reviewed, merged, and (if needed) reverted independently.

2. **Extend cpal, don't hand-roll a backend.** Both platforms converge on
   "bump cpal, widen an existing `#[cfg(windows)]` gate" rather than adding a
   new audio library or writing native FFI glue. This keeps the capture
   pipeline (VAD lanes, `StreamRouter`, `RecordedAudio` merging) completely
   unchanged — only device resolution and availability-detection are new.

3. **Availability is a runtime question, not just a compile-time `cfg`.**
   Today's `#[cfg(not(windows))]` hard error becomes a real tri-state per
   platform: available / unavailable (OS version too old, or no sound server
   present) / permission-denied (macOS only). A new `get_system_audio_availability`
   command replaces the implicit "ask and get a hard-coded error string" flow
   so the frontend can render the right message instead of a generic failure.

4. **macOS permission handling is necessarily reactive.** Since there is no
   precheck API for `kTCCServiceAudioCapture`, the flow is: attempt capture →
   on TCC-denial-shaped failure, surface a "grant access" affordance that
   opens System Settings (exact pane TBD — resolved empirically during
   implementation, falling back to the Privacy & Security root pane with
   instructional text if no precise deep link exists) → same shape as the
   existing Windows microphone-permission UI in `commands/audio.rs`
   (`get_windows_microphone_permission_status` / `open_microphone_privacy_settings`),
   reused as a pattern, not literally shared code (the underlying permission
   models differ too much: registry read vs. reactive-failure detection).

5. **Linux needs no permission UX at all.** The Linux plan is pure capture
   plumbing: enable the cpal features, add build dependencies, wire
   availability detection (PipeWire/PulseAudio socket presence). No consent
   flow, no settings deep link, no new frontend permission state beyond
   "unavailable."

6. **Minimum versions are hard floors, not soft warnings.** macOS <14.6 and
   Linux without PipeWire/PulseAudio get "unavailable," full stop — no
   degraded fallback capture path (e.g. no ALSA `snd-aloop` attempt).

7. **No cross-repo impact.** This is entirely internal to `shorthand-app`'s
   capture pipeline. It does not touch the `--follow-stream` protocol or any
   entry point `shorthand-core` imports, so none of the "not done when
   tagged" obligations in the root `CLAUDE.md` / `shorthand-core/AGENTS.md`
   apply.

## Architecture

No new architectural shape — the existing dual-lane design in `managers/audio.rs`
(independent `SmoothedVad` per lane, independent `StreamRouter`, merged into
`RecordedAudio { microphone, system }` on stop) is reused as-is. Per platform:

### macOS

- Bump `cpal` from `0.16.0` to a pinned `≥0.18.2`.
- Widen every `#[cfg(windows)]` in the files listed under Problem to also
  cover `target_os = "macos"` where the logic is genuinely
  platform-independent (device open, VAD lane, stream routing). Anything
  Windows-registry-specific (permission status read) stays Windows-only and
  gets a macOS-specific sibling, not a shared code path.
- Gate the feature itself on a runtime macOS-version check (≥14.6); below
  that, `get_system_audio_availability` reports unavailable rather than
  attempting to open the tap.
- Add `NSAudioCaptureUsageDescription` to the Tauri macOS bundle Info.plist
  configuration with clear, specific copy explaining why Shorthand wants
  system audio (shown verbatim in the OS consent dialog — this string *is*
  the simple-UX lever on macOS, since there's no multi-step flow to design
  around).
- New macOS permission-status command: since there's no precheck API, this
  reports `Unknown` until a capture attempt has actually been made this
  session, then `Allowed`/`Denied` based on the outcome of that attempt.
  Persist the last-known state (not the OS's — ours, observed) so the UI
  doesn't need to force a capture attempt just to render a status.
- New "open System Audio privacy settings" command (macOS analog of
  `open_microphone_privacy_settings`), using the best available deep link;
  if none is confirmed by the time of implementation, open the Privacy &
  Security root pane and show in-app instructional text naming the row to
  look for.
- Health check for the known silent-tap bug: while system audio is enabled
  and expected to be producing samples, detect an implausibly long silent
  stretch and transparently close/reopen the tap. Exact thresholds are an
  implementation-plan detail, not a design decision.

### Linux

- Enable cpal's `pipewire` and `pulseaudio` Cargo features (Linux-only,
  via target-specific `[target.'cfg(target_os = "linux")'.dependencies]` in
  `Cargo.toml` — do not enable on other platforms).
- Widen the same `#[cfg(windows)]` gates to also cover `target_os = "linux"`.
- Add a Linux availability check: PipeWire/PulseAudio socket presence at
  runtime (mirroring how `get_mute`/`set_mute` already probe `wpctl` then
  `pactl` then fall back). No sound server present → unavailable.
- `BUILD.md` Linux prerequisites gain `libpipewire-0.3-dev` (or distro
  equivalent) and `libpulse-dev` to the apt/dnf/pacman lists.
- Packaging (`.deb`/`.rpm`/AppImage) needs the corresponding runtime shared
  libraries declared as dependencies or bundled, matching how ALSA is
  handled today.
- No permission-status command, no settings deep link — `get_system_audio_availability`
  only ever returns available/unavailable on Linux, never a permission state.

### Shared (both platforms)

- `commands/audio.rs`: replace the `#[cfg(not(windows))]` hard error strings
  in `change_system_audio_enabled_setting` and `set_system_audio_device`
  with real logic. Add `get_system_audio_availability(app) -> SystemAudioAvailability`
  (a new enum: `Available`, `UnavailableOsVersion`, `UnavailableNoSoundServer`,
  `PermissionDenied` — the last macOS-only in practice, but modeled generically
  so the frontend doesn't need per-OS branching to interpret it).
- Frontend: `SystemAudioCapture.tsx`'s `if (osType !== "windows") return null;`
  becomes a query against `get_system_audio_availability` instead of an OS
  name check. macOS's `PermissionDenied` state renders a "grant access" CTA
  analogous to whatever pattern the existing mic-permission-denied UI uses
  elsewhere in `CaptureSettings.tsx`/`ModesSettings.tsx` — reuse that pattern,
  don't invent a new one.
- i18n: new strings needed for the availability/permission states go wherever
  the existing `settings.advanced.systemAudio.*` keys already live (upstream
  vs. fork-only file — confirm at implementation time per `AGENTS.md`'s i18n
  rules; do not guess).

## Testing

Device enumeration, VAD lane wiring, and `RecordedAudio` merging are already
exercised by the existing Windows-only tests/paths and only need the `cfg`
gates widened — low new-code risk there.

Permission and availability flows cannot be meaningfully unit-tested (a TCC
prompt and a PipeWire-absent environment are both real-OS-state-dependent).
Manual test matrix required before shipping either plan:

- macOS ≥14.6: grant, deny, deny-then-re-grant-via-Settings
- macOS <14.6: confirm graceful "unavailable" with no crash/hang
- Linux with PipeWire (current default on most distros)
- Linux with PulseAudio only (PipeWire disabled/absent)
- Linux with neither sound server present

## Open questions carried into implementation

- Exact System Settings pane/row for `kTCCServiceAudioCapture` on macOS —
  resolve by testing on a real 14.6+ machine; don't guess a deep link URL
  into the spec.
- Silent-tap health-check thresholds (how long is "implausibly silent")
  are an implementation-plan detail.
- Whether `settings.advanced.systemAudio.*` i18n keys are upstream-shared or
  fork-only — confirm before adding new keys.
