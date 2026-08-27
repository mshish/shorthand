# System audio capture for Linux and macOS

Status: draft (revised 2026-08-27 after code review — see Revision history)
Date: 2026-08-26

## Problem

System (output) audio loopback capture — recording what's playing through the
machine's speakers as a second, independently-VAD'd lane alongside the
microphone, merged into `RecordedAudio { microphone, system }` and published as
the `"them"` speaker — exists today only on Windows.

The feature is **fork-only**: commit `e22a920` ("feat(audio): transcribe Windows
system audio as a second speaker-labelled lane") introduced it and no part of it
exists in `upstream/main`. That matters for `AGENTS.md`'s "keep the diff
mergeable" rule — restructuring this code costs nothing at merge time, because
upstream has no competing version of these lines.

Its `#[cfg(windows)]` gating spans:

- `audio_toolkit/audio/recorder.rs` — ~70 gate sites: `SystemAudioCapture`,
  `build_loopback_stream`, `build_loopback_stream_typed`, `downmix_loopback`,
  `run_loopback_pump`, `LoopbackChunk`, `LoopbackPumpCmd`, `SystemAudioSession`,
  `with_system_vad`, `with_system_audio_callback`, and the system-lane branches
  inside `open()`, `stop()` and `close()`
- `managers/audio.rs` — the second VAD lane, `pending_system_audio`,
  `system_stream_router`, `get_effective_system_audio_device`,
  `update_system_audio_capture`, `set_system_stream_router`
- `managers/transcription.rs:550,628` — `SystemAudioTranscription` and its
  enablement check
- `commands/audio.rs:392-516` — `change_system_audio_enabled_setting` and
  `set_system_audio_device`, which hard-error off-Windows
- `src/components/settings/advanced/SystemAudioCapture.tsx:22-24` and
  `SystemAudioDeviceSelector.tsx`, `ModesSettings.tsx` — UI hidden unless
  `useOsType() === "windows"`

## What the Windows implementation actually is

An earlier draft of this spec described Windows as using "cpal's
`build_input_stream`-on-an-output-`Device` pattern" and concluded the port was
mechanical cfg-widening. Reading the implementation showed that is only half
right, and the correction drives most of this design.

The **capture primitive** is indeed portable: `build_loopback_stream_typed`
(`recorder.rs:965`) calls `device.build_input_stream(...)` — cpal's ordinary
input-stream constructor — on an output device. That is exactly the primitive
cpal now exposes natively for macOS (Core Audio Process Tap) and Linux
(PipeWire/PulseAudio sink monitors). So there is no "migrate Windows off WASAPI"
question; Windows is already on the portable API.

What surrounds that call is **generic real-time-audio-safety plumbing**, not
WASAPI-specific code:

- a pre-allocated buffer pool (`loopback_buffer_tx`/`rx`, 16×4096) so the audio
  callback never allocates
- a `SystemAudioSession` generation counter so samples captured before a restart
  are discarded rather than corrupting the next session
- a dropped-sample counter for backpressure accounting
- a dedicated `run_loopback_pump` thread decoupling the real-time callback from
  the resampling consumer
- a second `run_consumer` instance for the system lane's independent VAD

Every one of those solves a problem that exists on all three platforms. None
references a Windows API. They are `#[cfg(windows)]` only because that is the
platform the feature was first built for.

**Consequence:** the port is not "widen the gates so Linux/macOS reach cpal
directly." It is "**remove** the gates, so all three platforms share one
implementation." Since the app targets only Windows, macOS and Linux, most of
these gates can be deleted rather than converted to a three-way `any(...)`.

## Research findings

Two web-research passes (Aug 2026) plus direct source verification.

### macOS: cpal has native Process Tap support, at a steep OS cost

`cpal` added macOS loopback in v0.17.0, built on Apple's Core Audio Process Tap
API (`AudioHardwareCreateProcessTap`/`CATapDescription`). Usage is identical to
the existing Windows call site, so no Objective-C/Swift shim is needed.

Permission is a dedicated one-time TCC prompt under `kTCCServiceAudioCapture` —
distinct from, and lighter than, Screen Recording — tied to an
`NSAudioCaptureUsageDescription` Info.plist string, and marked by a purple
menu-bar dot rather than the orange screen-recording one. No hardened-runtime
entitlement is required (the app does not use App Sandbox; verified against
`Entitlements.plist`, which declares only microphone/audio-input).

**There is no public API to precheck this permission.** It is requested
implicitly on first capture attempt, so status can only be *observed* from an
attempt's outcome, never queried ahead of time.

The steep cost, confirmed against cpal's own compatibility table:

> `CoreAudio | macOS | 1.85 | macOS 14.2 (loopback recording requires 14.6+)`

That **macOS 14.2 floor applies to the entire CoreAudio backend**, not just
loopback. cpal 0.17+ references `AudioHardwareCreateProcessTap`
unconditionally, so on older macOS it fails both at link time and at runtime
with "Symbol not found" ([cpal#1241](https://github.com/RustAudio/cpal/issues/1241)).
The follow-up PR only *documented* the requirement; there is no weak-linking
mitigation, and nothing in the changelog since restores older support. The app
currently declares `minimumSystemVersion: "10.15"`.

Rejected alternatives: **ScreenCaptureKit** (`SCStreamConfiguration.capturesAudio`)
reaches back to macOS 13 but rides the Screen Recording TCC grant — a scarier
ask with a persistent orange indicator, for what the user experiences as audio
only. A **virtual audio driver** (BlackHole-style) needs a separate installer
plus manual Multi-Output Device setup in Audio MIDI Setup, failing the
"simple to grant access" requirement outright.

### Linux: cpal 0.18 backends, no permission model at all

cpal gained off-by-default `pipewire` and `pulseaudio` Cargo features in 0.18.0.
With both compiled in, cpal prioritizes PipeWire > PulseAudio > ALSA — the same
order the codebase already uses for mute control (`wpctl` > `pactl` > `amixer`
in `managers/audio.rs`). The PipeWire backend captures a sink's monitor via
PipeWire's own `STREAM_CAPTURE_SINK`, the direct analogue of WASAPI loopback;
the PulseAudio backend enumerates monitor sources the way `pactl list sources`
does and works identically against real PulseAudio or `pipewire-pulse`.

Plain ALSA enumeration does **not** expose per-sink monitors, so a new backend
genuinely is required — this cannot be done by extending the current ALSA-only
path.

**No permission prompt exists or applies.** PipeWire's portal-based access
control is a Flatpak/Snap sandboxing mechanism; a non-sandboxed
`.deb`/`.rpm`/AppImage process gets unrestricted access, exactly like today's
microphone capture. The Linux side is therefore pure capture plumbing.

Systems running neither PipeWire nor PulseAudio have no supported path. The ALSA
`snd-aloop` workaround requires the user to have already rerouted all system
output through a loopback device, which the app can neither configure nor
reliably detect — so that case reports "unavailable" rather than attempting
anything.

Risk: this backend is young (merged Feb/Mar 2026, released Jun 2026, still
receiving frequent fixes). Pin an exact version rather than a range.

### The cpal bump is a migration, not a version bump

Verified against `Cargo.lock` and the vendored fork:

- The pinned `cjpais/rodio` fork declares `cpal = { version = "0.16.0" }`.
  `audio_feedback.rs:119` passes a `cpal::Device` into
  `OutputStreamBuilder::from_device`, so bumping only the app's cpal produces
  two incompatible `Device` types and fails to compile.
- That fork is upstream rodio 0.20.1 plus exactly **one commit** — "update cpal
  to 0.16.0". There is no other fork-specific change to preserve.
- Upstream rodio 0.22.2 (Mar 2026) is itself only on cpal 0.17.
- cpal 0.16 → 0.18 additionally carries breaking changes to device naming,
  stream configuration and error types, which the existing `recorder.rs`,
  `device.rs` and `audio_feedback.rs` all touch.

### Follow-stream needs no work — verified

`--follow-stream` is already fully cross-platform and requires **zero** changes
for this feature:

- The transport uses `interprocess`'s `local_socket` abstraction, whose
  `GenericNamespaced` name type is supported on every platform — named pipes on
  Windows, the abstract namespace on Linux, and `/tmp/` paths on macOS/BSD.
  Both `#[cfg(windows)]` and `#[cfg(unix)]` branches are fully implemented
  (per-user identity via SID vs `geteuid()`; hardening via security descriptor
  vs socket file mode).
- The dual-speaker merge and `"me"`/`"them"` labelling in
  `managers/transcription.rs` carries no platform gates at all and is already
  unit-tested against `StreamSource::System`.

The only follow-stream-adjacent gates are `SystemAudioTranscription` and its
enablement check, which this design ungates along with everything else. Once
capture produces a second lane, follow-stream publishes it automatically.

## Decisions

1. **Remove the platform gates; don't widen them.** The system-audio machinery
   is fork-only, portable in substance, and needed on all three target
   platforms. Deleting `#[cfg(windows)]` from it yields one shared
   implementation rather than a three-way `any(...)` repeated ~70 times.

2. **Raise the macOS floor to 14.6.** Accepted deliberately, with eyes open:
   dropping macOS 10.15–14.5 excludes pre-2018 Macs that cannot run Sonoma,
   including Intel models `BUILD.md` explicitly supports. In exchange, the
   implementation is materially simpler — cpal provides the tap, and because
   14.6 is *at* the loopback requirement, **no runtime OS-version check is
   needed anywhere**; `SystemAudioAvailability` never reports a version failure
   on macOS. Users on 14.2–14.5 can reach 14.6 via a free same-major update.

3. **Fork rodio to bump cpal to 0.18.** Mirrors precedent exactly — the existing
   dependency is already a fork whose sole purpose is a cpal bump. Keeps one
   cpal version in the graph and no cross-version `Device` boundary. The
   alternative (two coexisting cpal versions, routing playback through
   `rodio::cpal`) was rejected: it doubles the compiled backends and encodes a
   subtle invariant that a future contributor would silently break.

4. **Three phases, one branch, shipped together.** The work splits into a
   dependency migration and two platform enablements, kept as separate
   *documents* so each can be reviewed and executed in sequence — but they are
   not separately shippable, and no intermediate state reaches users. That
   removes a class of otherwise-necessary work: a phase need not leave the
   feature safe or coherent for a platform a later phase completes. Commits
   should still build, to keep the tree bisectable.

   Phase A additionally owns everything **shared** between the two platforms —
   the availability enum and command, the de-gated Tauri commands, the
   availability-driven UI — so Phases B and C cannot drift from each other.
   An earlier draft duplicated that plumbing across both platform plans and it
   diverged immediately.

5. **Availability is a runtime question with a per-platform answer.** A new
   `get_system_audio_availability` command replaces today's hard-coded
   `#[cfg(not(windows))]` error string, reporting `Available`,
   `UnavailableNoSoundServer` (Linux only) or `PermissionDenied` (macOS only).
   The frontend gates on this rather than on OS name.

6. **macOS permission handling is necessarily reactive.** With no precheck API,
   the flow is: attempt capture → classify a TCC-denial-shaped failure → surface
   a "grant access" affordance opening System Settings. Modelled on the existing
   Windows microphone-permission pattern in `commands/audio.rs`, reused as a
   *pattern*, not shared code — the underlying mechanisms differ (registry read
   vs. observed failure).

7. **Linux needs no permission UX at all** — no consent flow, no deep link, no
   permission state.

8. **No fallbacks below the floor.** Linux without PipeWire/PulseAudio reports
   unavailable; no `snd-aloop` attempt. macOS below 14.6 cannot run the app at
   all, so the case does not arise.

9. **No cross-repo impact.** Entirely internal to `shorthand-app`'s capture
   pipeline; touches neither the `--follow-stream` protocol nor any entry point
   `shorthand-core` imports, so the root `CLAUDE.md` tagging obligations do not
   apply.

## Architecture

Three sequential **phases of one branch**, landing together. Nothing here
ships on its own, so no phase needs guards or UI states whose only purpose is
to make an intermediate state safe for users. Each commit should still build,
so the tree stays bisectable. Phase A owns everything shared between the two
platforms, so B and C cannot drift.

### Phase A — cpal 0.18 migration, de-platforming, shared plumbing

No user-visible change. Delivers: Windows system audio works exactly as before,
on cpal 0.18; the machinery compiles on all three platforms; and the shared
availability plumbing exists, reporting unavailable on Linux and macOS until
their phases land.

- Fork `cjpais/rodio` → bump its `cpal` to 0.18, repoint `Cargo.toml`.
- Bump the app's `cpal` 0.16 → pinned 0.18.x; fix the breaking API changes
  across `recorder.rs`, `device.rs`, `audio_feedback.rs`.
- Raise `minimumSystemVersion` 10.15 → 14.6; update `BUILD.md` and any CI
  matrix declaring a macOS deployment target.
- Delete `#[cfg(windows)]` from the portable system-audio machinery in
  `recorder.rs`, `managers/audio.rs`, `managers/transcription.rs`, so it
  compiles unconditionally. Device *resolution* remains unimplemented for
  Linux/macOS at this stage — `get_effective_system_audio_device` returns
  `None` there, so the feature is inert but the code is live.
- Windows regression pass is the gate: this plan must not change Windows
  behaviour at all.

### Phase B — Linux

- Enable cpal's `pipewire` + `pulseaudio` features under
  `[target.'cfg(target_os = "linux")'.dependencies]` only.
- Implement Linux device resolution in `get_effective_system_audio_device`,
  and a reachability probe (attempt `host_from_id` for PipeWire, then
  PulseAudio) behind `get_system_audio_availability`.
- Enforce availability in the enable/set-device commands, not merely in the UI —
  a persisted setting must not start capture on a machine with no sound server.
- `BUILD.md` prerequisites gain `libpipewire-0.3-dev`/`libpulse-dev` (and distro
  equivalents); `tauri.conf.json`'s `deb.depends`/`rpm.depends` gain the runtime
  libraries; the CI Linux job and `flake.nix` need the same.

### Phase C — macOS

- Implement macOS device resolution in `get_effective_system_audio_device`
  (output device → cpal loopback input stream, as on Windows).
- Add `NSAudioCaptureUsageDescription` to `Info.plist`. This string is shown
  verbatim in the OS consent dialog and *is* the permission UX.
- Observe permission state from capture attempts; expose it via
  `get_system_audio_availability` as `PermissionDenied`, and add an
  "open privacy settings" command. No OS-version check (Decision 2).
- Frontend: permission-denied CTA reusing the existing settings-link pattern.
- Guard against the known upstream bug where a Process Tap can silently degrade
  to all-zero buffers after long uptime, by detecting a prolonged silent stretch
  while capture is enabled and rebuilding the tap.

### Shared surface (owned by Phase A)

- `commands/audio.rs`: real per-platform logic replacing the
  `#[cfg(not(windows))]` errors, plus `get_system_audio_availability`.
- Frontend: `SystemAudioCapture.tsx`, `SystemAudioDeviceSelector.tsx` and
  `ModesSettings.tsx` gate on availability rather than `useOsType()`.
- i18n: new keys go wherever the existing `settings.advanced.systemAudio.*` keys
  live — confirm upstream-shared vs. fork-only per `AGENTS.md` before adding.

## Testing

The portable machinery (buffer pool, session generation, pump, merge) is already
exercised by existing tests and gains coverage automatically once ungated.
Availability decision logic is pure and unit-testable. Device resolution and
permission flows are OS-state-dependent and require manual verification:

- Windows regression after Plan A (no behaviour change)
- Linux with PipeWire; with PulseAudio only; with neither
- macOS 14.6+: grant, deny, deny-then-re-grant
- macOS: confirm the purple (not orange) capture indicator
- Verify the real cpal error surfaced on TCC denial matches what the classifier
  matches on — the OSStatus value assumed in Plan C is unverified until then

## Open questions carried into implementation

- Exact System Settings pane for `kTCCServiceAudioCapture` — resolve empirically
  on a 14.6+ machine; fall back to the Privacy & Security root with
  instructional text if no precise deep link exists.
- The exact error string/OSStatus cpal 0.18 surfaces on TCC denial.
- Silent-tap detection threshold.
- Whether `settings.advanced.systemAudio.*` keys are fork-only or upstream.

## Revision history

**2026-08-27** — substantially revised after reviewing the actual implementation
and an independent Codex review of the first-draft plans. Corrections:

- The first draft claimed Windows used a bare cpal call and that porting was
  mechanical cfg-widening. In fact the call is portable but is wrapped in
  RT-safety plumbing that was gated Windows-only for no intrinsic reason; the
  right move is gate *removal*, not widening. The draft also under-scoped the
  gate surface (~70 sites in `recorder.rs` alone, including
  `with_system_vad`/`with_system_audio_callback` whose *definitions* were gated,
  so the draft plans would not have compiled).
- The cpal bump was treated as a one-line change. It is a breaking migration
  that additionally conflicts with the pinned rodio fork's cpal 0.16 and raises
  the macOS floor from 10.15 to 14.2+ for *all* audio. Now a separate
  prerequisite plan, with the floor raised to 14.6 and rodio re-forked, both by
  explicit decision.
- Follow-stream was listed as an open concern; verified to need no work.
- Two plans became three.

**2026-08-27 (second pass)** — a further Codex review of the rewritten plans
found six more defects, all confirmed against the code and fixed:

- **Permission detection was reading a signal that is always "success."**
  `open()` swallows loopback failures by design (`recorder.rs:417-436`) and
  returns `Ok` while degrading to microphone-only, so a denied TCC prompt was
  indistinguishable from success. The flag already existed internally
  (`recorder.rs:516`) and was discarded at `recorder.rs:571`; Plan A now
  surfaces it as `AudioRecorder::open() -> Result<bool, _>` and
  `AudioRecordingManager::system_audio_active()`, which Plan C classifies on.
- **A third `cfg` *pair* was missed** at `recorder.rs:570-582`. Pairs must be
  merged, not half-deleted; Plan A now enumerates pairs before touching
  anything.
- **`get_preferred_loopback_config` was assumed portable.** Its own comment
  states a WASAPI-specific rationale for querying an *output* config, which
  need not hold where the loopback endpoint is an ordinary input device. Now
  falls back to the input config.
- **Linux device/config resolution was assumed to match Windows' shape.**
  cpal's PipeWire and PulseAudio hosts may expose monitors as input devices or
  require a named device rather than the synthetic default. Plan B now opens
  with a spike that measures this before any code is written.
- **Availability was enforced only in the command,** but startup reads
  persisted settings directly and never calls it. Plan B now also normalises
  the setting at startup.
- **The UI could observe a denial but never recover from one.** Availability
  was fetched once at mount and the denied state replaced the toggle, leaving
  no way to retry after granting. Both plans now refresh after every attempt,
  and the denied state carries an explicit retry.
- **The macOS silent-tap watchdog was cut, not fixed.** It timed a post-VAD
  callback that is legitimately idle whenever the app is not recording, so it
  would have fired on healthy systems and restarted the stream — defeating
  on-demand mic closure and discarding in-progress recordings. The bug it
  guarded against is unconfirmed on this codebase. Plan C Task 6 now records
  the deferral, why, and the four constraints any real implementation must
  meet.


**2026-08-27 (third pass)** — a further review, plus the decision that all three
phases ship on one branch. Changes:

- **Intermediate-state work was cut.** Two findings (Phase B alone exposing an
  unfinished macOS build; Phase C not independently landable) only mattered if
  phases shipped separately. They do not, so the idempotency scaffolding and
  cross-phase guards those would have required are gone.
- **Shared plumbing moved into Phase A** — availability enum and command,
  command de-gating, and UI gating — because duplicating it across B and C had
  already produced drift between them.
- **The macOS permission probe was fundamentally broken.**
  `update_system_audio_capture` returns `Ok` without opening anything when no
  stream is open (`managers/audio.rs:947`), which in the default on-demand mode
  is most of the time. Enabling the toggle therefore never attempted the tap,
  never triggered the consent prompt, and left permission unknowable. Phase C
  now adds a deliberate `probe_system_audio()` that opens, observes and
  restores — so the prompt fires at the moment of intent.
- **A denied enable is no longer persisted**, since the observation is
  process-local and would otherwise resurface after restart as a checked toggle
  that captures nothing.
- **`Device::name()` is deprecated** in cpal 0.18 in favour of `description()`
  and `id()`; under `clippy -D warnings` that is a build failure. Phase A
  migrates display to `description()` and explicitly defers changing the
  persisted key to `id()`, which would invalidate every saved device selection.
- **Sample-format ranking changed** to `F32 > F64 > integers by bit-depth
  descending`, so I24/U24/F64 can now be selected where I16 was before. The
  recorder's match arms rejected those, which would have meant silent capture
  failure on affected hardware; Phase A adds them.
- **`libpulse-dev` was dropped** — cpal's PulseAudio backend is a pure-Rust
  protocol implementation and links no native library, so the dependency would
  have restricted package installation for nothing.
- Frontend: availability is now refreshed after every capture attempt via a
  shared `useSystemAudioAvailability` hook, and `useSettings` gains the
  system-audio device list it must expose for the selector to build.
