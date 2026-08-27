# Plan C — System audio capture on macOS

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture system (output) audio on macOS as a second speaker-labelled lane alongside the microphone, with a permission flow a user can complete without instructions — one system prompt at the moment they ask for the feature, and if declined, one button that takes them to the right place and one that retries.

**Architecture:** Phases A and B did nearly all the platform-neutral work: the machinery compiles everywhere, `get_system_audio_host()` already returns CoreAudio (the host with Process Tap loopback) on macOS, `get_effective_system_audio_device` already resolves through it, and the availability plumbing and UI gating exist. What is left is entirely macOS-specific — the Info.plist consent string, and the permission flow. Because Phase A set the app minimum to macOS 14.6, at or above every version requirement involved, **no runtime OS-version check is needed anywhere.**

**Tech Stack:** Rust, `cpal` 0.18.x (CoreAudio Process Tap), Tauri 2.x, React/TypeScript.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Phase 3 of 3, all on one branch.** Phases A and B must be complete and green first — B's Task 4 rewrite of `get_effective_system_audio_device` removes Phase A's placeholder for *all* platforms, so macOS device resolution already works when this phase starts. All three phases ship together.

## Global Constraints

- **No OS-version gating.** The app minimum is macOS 14.6 (set in Phase A), which is at the loopback requirement. Any version check would be dead code.
- **No entitlement change.** Process Tap capture is gated by the `NSAudioCaptureUsageDescription` Info.plist key alone. This app does not use App Sandbox — `Entitlements.plist` declares only microphone/audio-input — and hardened runtime plus that key is sufficient.
- **Permission state can only be observed, never queried.** There is no precheck API for `kTCCServiceAudioCapture`. Any code that appears to ask the OS for current status is wrong.
- **No React unit tests.** Per `docs/FRONTEND_TESTING.md` this repo deliberately has no vitest/jest harness. Frontend verification is manual.
- Ship as a `.app` bundle (Tauri's default). A bare executable requesting this permission class cannot be managed from System Settings on current macOS.
- All `cargo` commands use `--manifest-path src-tauri/Cargo.toml`.

---

### Task 1: The consent string

**Files:**
- Modify: `src-tauri/Info.plist`

**Interfaces:** none — but this string is shown verbatim in the OS consent dialog and is the single most user-visible artifact of this phase. It *is* the permission UX.

- [ ] **Step 1: Add the key**

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

Expected: the key and string appear verbatim. If the build fails at the signing step, that is the pre-existing `signCommand`/Trusted Signing issue documented in `BUILD.md` — it does not affect whether the plist was written, so check for the `.app` anyway.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Info.plist
git commit -m "feat(macos): add NSAudioCaptureUsageDescription consent string"
```

---

### Task 1b: Spike — is a denied tap detectable at all?

**Files:** none permanent. Produces a **finding** that decides how Tasks 3 and 4 are written.

**Why this must come first.** Everything downstream assumes that when macOS denies the audio-capture permission, the loopback stream fails to open — so `system_audio_active()` goes false and we can classify it. That assumption is **unverified**, and there is a plausible alternative: the stream opens successfully and silently delivers all-zero samples. cpal's CoreAudio backend has been reported to behave that way. If that is what happens, every open-time signal reports success, availability stays `Available`, the CTA never renders, and the user is left with a toggle that appears on and captures silence. No amount of care in Tasks 3–5 recovers from building on the wrong answer.

Do this on a real Mac at macOS 14.6+, after Task 1 (the consent string must exist for a prompt to appear at all).

- [ ] **Step 1: Instrument a denied attempt**

Reset consent, then run a debug build and enable system audio:

```bash
tccutil reset AudioCapture <bundle-id>
```

Decline the prompt. With `--debug` logging on, record:

1. Did `build_loopback_stream` return `Err`? If so, the exact error text and OSStatus.
2. If it returned `Ok`, did the stream's error callback fire later? With what?
3. If neither, do samples arrive at the loopback data callback, and are they all zero?

Add temporary logging to `build_loopback_stream_typed`'s data callback (peak absolute sample per N callbacks) if needed to answer 3. Remove it afterwards.

- [ ] **Step 2: Repeat with permission granted**

Grant in System Settings, repeat, and record the same three answers with audio playing. You need the contrast: "all-zero samples" only means denial if a granted tap produces non-zero ones.

- [ ] **Step 3: Decide the classifier and write it down**

Edit Task 3's `observe_probe_outcome` and Task 2's probe to match what you measured:

- **Open fails on denial** → the plan as written is correct; proceed unchanged.
- **Open succeeds, samples are silent** → an open-time flag cannot classify. The probe must instead play a brief known sound and sample the lane, or the design must drop automatic detection and always offer the "grant access" affordance rather than gating it behind a detected denial. Prefer the latter if the former proves unreliable: an always-available settings link is worse UX than a correct auto-detect, but far better than a CTA that never appears.
- **Something else** → record it and reason from there.

Whatever you find, write the answer into Task 3 as a comment so the next reader does not re-derive it.

---

### Task 2: Probe the tap deliberately when the user enables it

**Files:**
- Modify: `src-tauri/src/managers/audio.rs`

**Interfaces:**
- Produces: `pub fn probe_system_audio(&self, device_name: Option<String>) -> bool` on `AudioRecordingManager` — opens the capture stream with the requested system-audio configuration so the loopback attempt (and therefore the OS consent prompt) actually happens, reports whether the lane came up, and restores the prior stream state.
- Consumes: Task 1b's finding on whether a denied tap is detectable at open time.

**Why this exists — the obvious approach silently does nothing.** `update_system_audio_capture` only restarts the stream when one was already open:

```rust
        *self.pending_system_audio.lock().unwrap() = Some(PendingSystemAudioCapture {
            enabled,
            device_name,
        });
        let restart_result = if was_open {
            self.start_microphone_stream()
        } else {
            Ok(())          // <-- managers/audio.rs:947
        };
        *self.pending_system_audio.lock().unwrap() = None;   // config discarded
```

On-demand microphone mode is the **default**, so when a user flips the toggle while idle, nothing opens, no tap is attempted, no consent prompt appears, and `system_audio_active()` is false. A permission check reading that would report "denied" for a prompt the user was never shown, and the retry button would reproduce it forever.

**And note the last line**: the pending configuration is cleared whether or not it was used. So a probe that simply calls `start_microphone_stream()` afterwards resolves `pending_system_audio == None` and falls back to the *persisted* `settings.system_audio_enabled` — which has not been written yet at that point in the command. It would open microphone-only and report a denial that never happened. The probe must therefore carry the requested configuration itself rather than relying on state the caller already discarded.

- [ ] **Step 1: Write the failing test**

The probe itself touches real devices, so test the decision it encodes — that a probe is only needed when no stream is already open:

```rust
#[cfg(test)]
mod system_audio_probe_tests {
    use super::*;

    #[test]
    fn probing_is_needed_when_no_stream_is_open() {
        assert!(needs_probe_open(false));
    }

    #[test]
    fn probing_reuses_an_already_open_stream() {
        // Opening again would tear down and rebuild a live capture for nothing.
        assert!(!needs_probe_open(true));
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_probe_tests
```

- [ ] **Step 3: Implement**

```rust
/// Whether `probe_system_audio` must open a stream itself, given whether one
/// is already open.
fn needs_probe_open(stream_already_open: bool) -> bool {
    !stream_already_open
}

impl AudioRecordingManager {
    /// Forces a loopback open attempt so the OS consent prompt fires now,
    /// and reports whether the system-audio lane actually came up.
    ///
    /// Necessary because `update_system_audio_capture` is a no-op when no
    /// stream is open, which in on-demand mode (the default) is most of the
    /// time. Takes the requested configuration explicitly rather than reading
    /// persisted settings: at the point the enable command calls this, the new
    /// value has not been written yet, and `pending_system_audio` has already
    /// been cleared. Restores the prior open/closed state before returning, so
    /// a probe never leaves the microphone running behind the user's back.
    pub fn probe_system_audio(&self, device_name: Option<String>) -> bool {
        let was_open = *self.is_open.lock().unwrap();

        // Supply the configuration `start_microphone_stream` will look for.
        // Without this it falls back to the persisted setting, which is still
        // `false` here, and the probe would open microphone-only and report a
        // denial that never happened.
        *self.pending_system_audio.lock().unwrap() = Some(PendingSystemAudioCapture {
            enabled: true,
            device_name,
        });

        if needs_probe_open(was_open) {
            // Cancel any pending lazy close so it cannot race our teardown.
            self.close_generation.fetch_add(1, Ordering::SeqCst);
        } else {
            // A stream is already open, but it was opened without the system
            // lane — reopen so the tap is actually attempted.
            self.stop_microphone_stream();
        }

        let opened = self.start_microphone_stream();
        *self.pending_system_audio.lock().unwrap() = None;

        if let Err(error) = opened {
            warn!("System audio probe could not open the capture stream: {error}");
            if needs_probe_open(was_open) {
                return false;
            }
            // Best effort: put the user's stream back the way it was.
            let _ = self.start_microphone_stream();
            return false;
        }

        let active = self.system_audio_active();

        if needs_probe_open(was_open) {
            self.stop_microphone_stream();
        }

        active
    }
}
```

Note the lock discipline: read `is_open` into a local and let the guard drop before calling `start_microphone_stream`/`stop_microphone_stream`, both of which take that same lock. Holding it across either call deadlocks.

- [ ] **Step 4: Run the tests and watch them pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_probe_tests
```

- [ ] **Step 5: Verify and commit**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
git add src-tauri/src/managers/audio.rs
git commit -m "feat(macos): probe the loopback tap when system audio is enabled"
```

---

### Task 3: Observe permission state and fold it into availability

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (state registration)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Produces:
  - `MacosSystemAudioCaptureState(pub Mutex<PermissionAccess>)` — Tauri-managed, holding the last *observed* outcome.
  - `get_system_audio_availability` (Phase A Task 6) gains its macOS branch.

**Two traps to know before writing code.**

*You cannot detect denial from whether the enable command returned `Ok`.* `AudioRecorder::open()` deliberately swallows loopback failures (`recorder.rs:417-436`): it logs a warning and continues microphone-only and **successful**. A denied prompt yields `Ok`. The real signal is Phase A's `system_audio_active()`, reported by Task 2's probe.

*The observation must not be trusted across restarts.* It lives in process memory and resets to `Unknown`, which maps to `Available`. If the enable also persisted `system_audio_enabled = true`, the next launch shows a checked toggle that does nothing. Task 4 prevents that by not persisting a failed enable.

**A third trap, and the reason Task 1b exists.** This whole design assumes a denied tap makes the *open* fail, so that `system_audio_active()` goes false. That is **unverified**. `system_audio_active()` is set from `build_loopback_stream` succeeding — which is `stream.play()` returning, before any sample arrives. If macOS instead grants a stream that silently delivers zeros when permission is missing (a failure mode reported against cpal's CoreAudio backend), the flag stays true, availability reports `Available`, and the CTA never appears. Do **not** build this task on the assumption: run Task 1b's spike first and let its answer decide the classifier.

**On classification precision.** Even given a detectable failure, the exact error macOS produces is unverified. Rather than bet on an unconfirmed OSStatus, this treats "the lane did not come up after a real attempt" as "needs permission", with copy that reads correctly either way. Once the real signature is known the classifier can be tightened with no UI change.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod macos_system_audio_permission_tests {
    use super::*;

    #[test]
    fn a_live_lane_after_a_real_attempt_is_allowed() {
        assert_eq!(observe_probe_outcome(true), PermissionAccess::Allowed);
    }

    #[test]
    fn a_dead_lane_after_a_real_attempt_is_denied() {
        // Deliberately coarse: macOS offers no way to ask why a tap failed,
        // and at our minimum OS a declined prompt is the likeliest cause.
        assert_eq!(observe_probe_outcome(false), PermissionAccess::Denied);
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
        // Before any attempt we must not accuse the user of declining
        // something they were never asked about.
        assert_eq!(
            macos_availability(PermissionAccess::Unknown),
            SystemAudioAvailability::Available
        );
    }
}
```

- [ ] **Step 2: Run and watch it fail**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_permission_tests
```

- [ ] **Step 3: Implement**

```rust
/// The last observed outcome of a macOS system-audio probe.
///
/// macOS exposes no way to query `kTCCServiceAudioCapture`, so this is the
/// only source of truth available: what happened last time we tried. It is
/// process-local and deliberately not persisted — a stale "denied" would
/// outlive a grant made in System Settings.
pub struct MacosSystemAudioCaptureState(pub std::sync::Mutex<PermissionAccess>);

/// Maps a probe's outcome to an observed permission state. The input is
/// `AudioRecordingManager::probe_system_audio()`, which performs a real open
/// attempt — so `false` means the OS refused, not that we never asked.
fn observe_probe_outcome(loopback_live: bool) -> PermissionAccess {
    if loopback_live {
        PermissionAccess::Allowed
    } else {
        PermissionAccess::Denied
    }
}

/// Availability from the last observed permission state. `Unknown` means we
/// have not probed yet, which is not a denial.
fn macos_availability(observed: PermissionAccess) -> SystemAudioAvailability {
    match observed {
        PermissionAccess::Denied => SystemAudioAvailability::PermissionDenied,
        PermissionAccess::Allowed | PermissionAccess::Unknown => {
            SystemAudioAvailability::Available
        }
    }
}
```

Then give `get_system_audio_availability` (Phase A Task 6) its macOS branch. Phase A left `let _ = &app;` there as the seam:

```rust
#[tauri::command]
#[specta::specta]
pub async fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability {
    #[cfg(target_os = "macos")]
    {
        let observed = *app
            .state::<MacosSystemAudioCaptureState>()
            .0
            .lock()
            .unwrap();
        return macos_availability(observed);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = &app;
        tokio::task::spawn_blocking(|| {
            availability_from_host_probe(crate::audio_toolkit::get_system_audio_host().is_some())
        })
        .await
        .unwrap_or(SystemAudioAvailability::UnavailableNoSoundServer)
    }
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml macos_system_audio_permission_tests
```

Expected: PASS (4 tests).

- [ ] **Step 5: Register the state**

In `src-tauri/src/lib.rs`, register it alongside the app's other managed state, gated `#[cfg(target_os = "macos")]`, initialised to `PermissionAccess::Unknown`. Match the surrounding code's actual `.manage(...)` style rather than copying a shape blindly.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src
git commit -m "feat(macos): observe system audio permission from a real probe"
```

---

### Task 4: Wire the probe into the enable command

**Files:**
- Modify: `src-tauri/src/commands/audio.rs` (`change_system_audio_enabled_setting`)

**Interfaces:**
- Consumes: `probe_system_audio` (Task 2), `observe_probe_outcome` (Task 3).

- [ ] **Step 1: Probe on enable, and do not persist a failed enable**

In `change_system_audio_enabled_setting`, after the existing `update_system_audio_capture` call succeeds and before the settings write, add:

```rust
        // macOS only: force a real loopback attempt so the consent prompt
        // fires now, and record what happened. Without this the tap is never
        // touched in on-demand mode and permission stays unknowable.
        #[cfg(target_os = "macos")]
        let mut enabled = enabled;
        #[cfg(target_os = "macos")]
        if enabled {
            let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
            let probe_device = settings.system_audio_device.clone();
            let live = tokio::task::spawn_blocking(move || {
                manager.probe_system_audio(probe_device)
            })
                .await
                .map_err(|error| format!("audio task join failed: {error}"))?;

            *app.state::<MacosSystemAudioCaptureState>().0.lock().unwrap() =
                observe_probe_outcome(live);

            if !live {
                // Don't persist an enabled state the OS just refused: the
                // observation is process-local, so on the next launch the
                // toggle would read as on while capturing nothing.
                warn!("System audio was refused; leaving the setting disabled");
                enabled = false;
            }
        }
```

then let the existing `settings.system_audio_enabled = enabled; write_settings(...)` run as before, now writing the corrected value.

`probe_system_audio` opens and closes cpal streams, so it must go through `spawn_blocking` — calling it inline would block the webview/main run loop exactly as the existing `spawn_blocking` calls in this file avoid doing.

- [ ] **Step 2: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings \
  && cargo test --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/audio.rs
git commit -m "feat(macos): prompt for system audio permission on enable"
```

---

### Task 5: Privacy settings link and the denied-state UI

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (`collect_commands![...]`)
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx`
- Modify: the fork's i18n catalogue (located in Step 3 — do not guess)

**Interfaces:**
- Produces: `pub fn open_system_audio_privacy_settings() -> Result<(), String>`
- Consumes: the `useSystemAudioAvailability` hook from Phase A Task 6.

- [ ] **Step 1: Add the settings-link command**

Next to the existing `open_microphone_privacy_settings`:

```rust
#[tauri::command]
#[specta::specta]
pub fn open_system_audio_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // `kTCCServiceAudioCapture` is a newer bucket than Screen Recording and
        // its exact System Settings anchor is unverified (see Task 6). This URL
        // opens Privacy & Security itself, which is correct regardless; if
        // Task 6 finds a working per-pane anchor, tighten it here.
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

Register `commands::audio::open_system_audio_privacy_settings,` in `collect_commands![...]`, then regenerate bindings with a brief `bun run tauri dev`.

- [ ] **Step 2: Find the right i18n file**

```bash
grep -rn "systemAudio" src/i18n/locales/en/translation.json src/shorthand/locales/en.json src/shorthand/english-copy.json
```

Per `AGENTS.md`'s i18n rules, fork-only keys belong in `src/shorthand/locales/en.json`, never `src/i18n/locales/` — the `check:locale-drift` gate enforces this. Add, matching the file's existing key convention:

```json
"settings.advanced.systemAudio.permissionNeeded": "Shorthand needs permission to record audio playing on this Mac. If you declined the prompt, you can grant it in System Settings.",
"settings.advanced.systemAudio.tryAgain": "Try again"
```

- [ ] **Step 3: Add the denied branch**

Phase A already gave `SystemAudioCapture.tsx` the `useSystemAudioAvailability` hook and a toggle that refreshes after each attempt. Add, after the existing early-return and before the toggle's `return`:

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
              // Re-attempt: granting in System Settings cannot change our
              // observed state on its own, because the only way to learn the
              // new state is to try again.
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

**The retry button is not optional.** This branch replaces the toggle, so without it a user who grants permission in System Settings has no way back — the observed state only changes when an attempt is made, and nothing else here would make one.

`accessibility.openSettings` is an existing key already used for the Windows microphone-permission button (`AccessibilityOnboarding.tsx:354`) — reuse it rather than adding a second "Open Settings" string.

- [ ] **Step 4: Verify the gates**

```bash
bun run build && bun run lint && bun run check:translations && bun run check:locale-drift && bun run check:fork-translations
```

Expected: all pass. The locale-drift gate exists precisely to catch a fork-only key added to upstream's catalogue.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src src/bindings.ts src/
git commit -m "feat(macos): add grant-access and retry when system audio is denied"
```

---

### Task 6: Silent-tap watchdog — DEFERRED, do not implement

**Status: cut.** An earlier draft specified a background thread that rebuilt the tap after ten minutes without system-audio samples, working around an upstream report that Core Audio Process Taps can silently degrade to all-zero buffers after long uptime. Review showed the design was **actively harmful**, so it is deferred rather than shipped broken.

Why it was cut, so nobody re-adds it unexamined:

- **It measured the wrong signal.** It would have timed `with_system_audio_callback`, which is fed *post-VAD* and returns early while the app is not recording (`recorder.rs:1289`). An enabled-but-idle app therefore looks permanently "silent", so the watchdog would fire on healthy systems as a matter of course.
- **Firing is destructive.** Its recovery called `start_microphone_stream()`, defeating on-demand microphone closure entirely, and — if it fired mid-session — restarting the whole mic+system stream and discarding the recording in progress. `AudioRecorder` has no per-lane reopen, so there is no cheap version.
- **The bug is unconfirmed here.** We have not reproduced the upstream report on this codebase.

If Task 7's long-session check shows real degradation, implement it properly rather than reviving the sketch:

1. Measure at the **raw loopback callback or the pump** (`build_loopback_stream_typed`'s data callback, or `run_loopback_pump`), which sees every frame regardless of VAD and recording state.
2. Distinguish "the tap is delivering silence" from "the tap has stopped delivering". The former is normal; only the latter is the bug.
3. Never restart while `is_recording()` is true. Defer to the next idle moment.
4. Prefer rebuilding only the system lane. If that needs a per-lane reopen on `AudioRecorder`, that is the actual work — and its cost is a reason to be sure the bug is real first.

---

### Task 7: Manual verification matrix

No files change. None of this can run in CI. Run on a real Mac at macOS 14.6 or later.

- [ ] **First-run consent fires at the toggle**: with a fresh TCC state (`tccutil reset AudioCapture <bundle-id>`) and the app in its default on-demand microphone mode, enable system audio capture **without recording anything**. The system dialog must appear immediately, showing the Task 1 string verbatim. If no prompt appears until you start a recording, Task 2's probe is not working.
- [ ] **The right indicator**: while capturing, confirm a **purple** menu-bar dot — not the orange screen-recording one. Orange means something is using ScreenCaptureKit and the design's central premise is wrong; stop and report.
- [ ] **The probe leaves no stream running**: after enabling while idle, confirm the microphone is not left open (no mic indicator, and the log shows the probe's stop).
- [ ] **Both speakers transcribe**: play audio from another app, record, confirm the transcript contains `me` and `them` lanes as on Windows.
- [ ] **Follow-stream**: run `handy --follow-stream` during a dual-speaker session; confirm both `"speaker":"me"` and `"speaker":"them"` events appear. No code change should have been needed.
- [ ] **Deny path**: reset TCC, decline the prompt, confirm the CTA renders and the toggle did **not** persist as enabled.
- [ ] **Denial does not survive restart as a lie**: after denying, restart the app and confirm the toggle reads as off rather than on-but-broken.
- [ ] **Settings link**: click it and record **which pane actually opens** and whether the audio-capture row is reachable. If a more precise anchor exists, tighten `open_system_audio_privacy_settings` (Task 5) and re-verify; if not, confirm the copy is enough to guide the user.
- [ ] **Re-grant path**: grant from System Settings, return to the app, click **Try again**, confirm capture works and availability flips to `available`. This is the path that strands users if the retry button is missing.
- [ ] **Capture the real denial error**: with `--debug`, record the exact error text and OSStatus surfaced on a denied attempt and add it as a comment above `observe_probe_outcome` (Task 3). If reliably distinguishable, tighten that function to treat only that signature as `Denied`, and update its test.
- [ ] **Microphone unaffected**: confirm ordinary dictation still works with system audio both on and off, in both on-demand and always-on microphone modes.
- [ ] **Long-session check (decides Task 6)**: leave capture enabled and idle for >15 minutes with nothing playing, then play audio and record. If it still captures, the tap did not degrade — write that result into Task 6 so nobody re-litigates it. If it captures nothing, the upstream bug is real here: report it and implement Task 6 to the four constraints listed there.
- [ ] **Lints**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && bun run lint`.
