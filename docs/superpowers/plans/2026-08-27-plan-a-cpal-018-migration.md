# Plan A — cpal 0.18 migration and de-platforming

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the app to cpal 0.18 and make the fork-only system-audio machinery compile on all three platforms, with **zero user-visible change** — Windows system audio must behave exactly as it does today, and Linux/macOS gain the code without yet gaining the feature.

**Architecture:** Three coupled changes that only make sense together: (1) re-fork `cjpais/rodio` to bump its pinned cpal, because it exchanges `cpal::Device` values with the app and would otherwise produce two incompatible `Device` types; (2) bump the app's cpal 0.16 → 0.18 and fix the resulting breaking-API fallout; (3) delete `#[cfg(windows)]` from the portable system-audio machinery so it compiles unconditionally. Raising the macOS deployment floor to 14.6 is forced by (2) and is a deliberate, spec-recorded product decision.

**Tech Stack:** Rust, `cpal` 0.18.x, `rodio` (forked), Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**Phase 1 of 3, all on one branch.** Plans A, B and C are sequential phases of a single piece of work that ships together — nothing here is released on its own. So this phase may leave Linux and macOS system audio *inert* (compiled but resolving no device); Phases B and C fill that in. Do not add guards, fallbacks or UI states whose only purpose is to make an intermediate phase safe for users — no intermediate phase reaches users. Each **commit** should still build, so the tree stays bisectable.

## Global Constraints

- **Windows behaviour must not change.** This phase ships no feature. Any observable difference in Windows system-audio capture is a bug in this phase, not an improvement.
- This phase owns everything **shared** between Linux and macOS — the availability enum and command, the de-gated Tauri commands, and the availability-driven frontend. Phases B and C then only add what is specific to their platform. Anything both would otherwise write belongs here instead, so the two cannot drift.
- Pin cpal to an exact version (`=0.18.x`), not a range — the Linux backends are young and receiving frequent fixes.
- The system-audio machinery is **fork-only** (commit `e22a920`, absent from `upstream/main`), so restructuring it carries no upstream merge cost. Gates on it should be **deleted**, not converted to `any(windows, target_os = "linux", target_os = "macos")`.
- Do **not** add a React unit-test harness. Per `docs/FRONTEND_TESTING.md` the repo deliberately has no vitest/jest — only Playwright smoke tests in `tests/app.spec.ts`. Frontend verification in these plans is manual.
- macOS minimum becomes **14.6** (cpal's CoreAudio floor is 14.2; loopback needs 14.6; we set one number at the higher value so no runtime version check is ever needed).
- All `cargo` commands run from `src-tauri/` or use `--manifest-path src-tauri/Cargo.toml` — there is no `Cargo.toml` at the repo root.

---

### Task 1: Fork rodio and bump its cpal

**Files:**
- Create: a new fork of `cjpais/rodio` under your own account (e.g. `mshish/rodio`), branch `update-cpal-018`
- Modify: `src-tauri/Cargo.toml:61` (the `rodio` git dependency)

**Interfaces:**
- Produces: a rodio git revision whose `cpal` dependency is `0.18`, so the app and rodio resolve to a single shared cpal and `cpal::Device` remains one type across the `audio_feedback.rs` boundary.

Context you need: the current dependency is `rodio = { git = "https://github.com/cjpais/rodio.git" }`, resolving to rev `fed3029`. That fork is upstream rodio **0.20.1 plus exactly one commit** ("update cpal to 0.16.0"). There is no other fork-specific change, so this task repeats that same one-line move for 0.18.

- [ ] **Step 1: Fork and clone**

Fork `https://github.com/cjpais/rodio` on GitHub, then:

```bash
git clone https://github.com/<your-account>/rodio.git /tmp/rodio-fork
cd /tmp/rodio-fork
git checkout -b update-cpal-018 fed3029
```

- [ ] **Step 2: Bump the cpal dependency**

In `/tmp/rodio-fork/Cargo.toml`, change line 92 from:

```toml
cpal = { version = "0.16.0", optional = true }
```

to:

```toml
cpal = { version = "0.18.2", optional = true }
```

(Substitute the exact latest `0.18.x` from crates.io if newer, and use the same value everywhere in this plan.)

- [ ] **Step 3: Verify rodio itself still builds**

```bash
cd /tmp/rodio-fork && cargo check --features playback
```

Expected: compiles. If rodio's own code fails against cpal 0.18's API changes, fix those failures here — that is exactly the work this fork exists to absorb. Consult the [cpal upgrade guide](https://docs.rs/crate/cpal/0.18.2/source/UPGRADING.md) for the specific renames.

- [ ] **Step 4: Push the fork**

```bash
cd /tmp/rodio-fork && git commit -am "update cpal to 0.18" && git push -u origin update-cpal-018
```

Record the resulting commit SHA — the next step pins it.

- [ ] **Step 5: Repoint the app at the new fork**

In `src-tauri/Cargo.toml`, change line 61 from:

```toml
rodio = { git = "https://github.com/cjpais/rodio.git" }
```

to (substituting your account and the SHA from Step 4):

```toml
rodio = { git = "https://github.com/<your-account>/rodio.git", rev = "<sha>" }
```

Pin by `rev`, not branch — the existing dependency's lack of a pin is a reproducibility gap worth not repeating.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build: point rodio at fork with cpal 0.18"
```

---

### Task 2: Bump the app's cpal and fix the breaking changes

**Files:**
- Modify: `src-tauri/Cargo.toml:51` (`cpal = "0.16.0"`)
- Modify (as compile errors dictate): `src-tauri/src/audio_toolkit/audio/recorder.rs`, `src-tauri/src/audio_toolkit/audio/device.rs`, `src-tauri/src/audio_feedback.rs`, `src-tauri/src/commands/audio.rs`, `src-tauri/src/managers/audio.rs`

**Interfaces:**
- Produces: the whole app compiling against cpal 0.18 with unchanged behaviour.

cpal 0.16 → 0.18 carries breaking changes to device naming, stream configuration and error types. Rather than guess which, this task is driven by the compiler plus the upstream upgrade guide.

- [ ] **Step 1: Read the upgrade guide first**

Open [the cpal upgrade guide](https://docs.rs/crate/cpal/0.18.2/source/UPGRADING.md) and note every entry covering 0.16→0.17 and 0.17→0.18. Write the list down before touching code — you will match compile errors against it.

- [ ] **Step 2: Bump the version**

In `src-tauri/Cargo.toml`, change line 51:

```toml
cpal = "0.16.0"
```

to:

```toml
cpal = "=0.18.2"
```

- [ ] **Step 3: Get the full error list**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tee /tmp/cpal-migration-errors.txt
```

Expected: FAIL, with errors concentrated in `recorder.rs` (stream building, sample formats, error types), `device.rs` (enumeration/naming) and `audio_feedback.rs` (device handoff to rodio).

- [ ] **Step 4: Fix each error against the guide**

Work through `/tmp/cpal-migration-errors.txt`. For each error, apply the mechanical change the upgrade guide prescribes. Constraints while doing so:

- Change only what the compiler requires. This is a migration, not a refactor — resist tidying adjacent code.
- Preserve the channel-averaging logic in `build_stream`'s callback (`recorder.rs:795-816`) exactly.

Two changes are **not** compiler-driven and will not appear in that error list. Both must be handled here or they become runtime bugs:

**`Device::name()` is deprecated in favour of `id()` and `description()`.** The guide's rule: `description()` for anything shown to a user, `id()` for anything persisted or matched against. It only warns rather than errors — but `clippy -D warnings` (Step 5) turns it into a failure, so every call site must move. `device.rs`'s `list_input_devices`/`list_output_devices` populate `CpalDeviceInfo.name`, and that value is both displayed *and* persisted (`selected_microphone`, `system_audio_device`, `clamshell_microphone` all store a device name and match on it later). Keep this migration mechanical for now — swap `name()` for `description()` so behaviour is unchanged — and do **not** switch the persisted key to `id()` in this phase: that would silently invalidate every existing user's saved device selection. Note it as a follow-up instead.

**Sample-format selection was re-ranked to `F32 > F64 > integers by bit-depth descending > DSD`.** I32 and I24 now outrank I16, so hardware that previously negotiated I16 may now return I24, U24 or F64. The existing match arms in `build_stream` (`recorder.rs:360-409`) and `build_loopback_stream` (`recorder.rs:856-898`) handle only U8/I8/I16/I32/F32 and return "Unsupported sample format" otherwise — which after this bump means *capture silently fails on affected devices*. Add arms for the newly-reachable formats:

```rust
                    cpal::SampleFormat::I24 => AudioRecorder::build_stream::<cpal::I24>( /* ...same args... */ ),
                    cpal::SampleFormat::U24 => AudioRecorder::build_stream::<cpal::U24>( /* ...same args... */ ),
                    cpal::SampleFormat::F64 => AudioRecorder::build_stream::<f64>( /* ...same args... */ ),
```

Both generic functions are bounded `T: Sample + SizedSample + Send + 'static, f32: cpal::FromSample<T>`, which these satisfy, so no other change is needed. Confirm the exact type names against cpal 0.18's `SampleFormat` before writing them.
- If a change looks like it alters runtime behaviour rather than just types (e.g. a changed default buffer size or a renamed method with different semantics), stop and note it — that is a finding for the human, not something to absorb silently.

- [ ] **Step 5: Verify a clean build and clean lints**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings \
  && cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Expected: all three pass.

- [ ] **Step 6: Run the existing Rust test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS, with no test newly failing relative to the pre-bump baseline. If you did not capture a baseline before Step 2, do it now on a stash and compare.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src
git commit -m "build: migrate to cpal 0.18"
```

---

### Task 3: Raise the macOS deployment floor to 14.6

**Files:**
- Modify: `src-tauri/tauri.conf.json:42` (`"minimumSystemVersion": "10.15"`)
- Modify: `BUILD.md` (macOS prerequisites section)

**Interfaces:** none — packaging metadata and docs.

This is forced by cpal 0.18's CoreAudio backend, which references `AudioHardwareCreateProcessTap` unconditionally and fails to link or run below macOS 14.2. Setting 14.6 (rather than 14.2) means the loopback requirement is also always satisfied, so no runtime version check is needed anywhere in Plans B/C.

- [ ] **Step 1: Raise the declared minimum**

In `src-tauri/tauri.conf.json`, change:

```json
"minimumSystemVersion": "10.15",
```

to:

```json
"minimumSystemVersion": "14.6",
```

- [ ] **Step 2: Document the floor and why**

In `BUILD.md`'s macOS section, add a note directly under the `#### macOS` heading:

```markdown
> [!IMPORTANT]
> Shorthand requires **macOS 14.6 or later**. The audio backend (cpal 0.18)
> links `AudioHardwareCreateProcessTap`, which does not exist before macOS
> 14.2, and system-audio capture needs 14.6. Older macOS cannot run the app
> at all — not merely without system audio.
```

Also review the existing "Intel Mac (x86_64)" subsection: it stays accurate (Intel Macs that run 14.6 are still supported), but confirm nothing in it implies support for older releases.

- [ ] **Step 3: Check for other declarations of the old floor**

```bash
grep -rn "10\.15\|MACOSX_DEPLOYMENT_TARGET" --include=*.yml --include=*.json --include=*.toml --include=*.nix --include=*.md . | grep -v node_modules | grep -v target
```

Update any other place that names the old minimum. (At the time of writing, CI workflows pin runner images — `macos-26`, `macos-latest` — and declare no explicit deployment target, so there is likely nothing to change there; verify rather than assume.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json BUILD.md
git commit -m "build(macos): raise minimum macOS to 14.6 for cpal 0.18"
```

---

### Task 4: Remove the platform gates from the system-audio machinery

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/recorder.rs` (~70 `#[cfg(windows)]` sites)
- Modify: `src-tauri/src/audio_toolkit/audio/mod.rs:9-10`
- Modify: `src-tauri/src/managers/audio.rs`
- Modify: `src-tauri/src/managers/transcription.rs:550,628`
- Modify: `src-tauri/src/commands/audio.rs:5-8`
- Modify: `src-tauri/src/lib.rs` (the `SystemAudioTranscription` state registration)

**Interfaces:**
- Produces: `SystemAudioCapture`, `AudioRecorder::open(device, system_audio)`, `with_system_vad`, `with_system_audio_callback`, `SystemAudioTranscription` and `StreamSource::System` all available unconditionally on every target. `AudioRecordingManager::update_system_audio_capture(&self, enabled: bool, device_name: Option<String>, stream_router: Option<Arc<StreamRouter>>) -> Result<(), anyhow::Error>` compiles everywhere.

The gates being deleted fall into two groups. **Delete** the ones on portable machinery. **Keep** any gate on genuinely platform-specific code (the Windows registry permission reads in `commands/audio.rs`, the `wpctl`/`pactl`/`amixer` mute helpers in `managers/audio.rs`) — those are not part of this task.

- [ ] **Step 1: Inventory the gates — and find every PAIR first**

A lone `#[cfg(windows)]` can simply be deleted. A **pair** — `#[cfg(windows)] X` followed by `#[cfg(not(windows))] Y` — cannot: deleting one half leaves two conflicting definitions or a missing binding, and deleting both loses `Y`'s logic. Every pair must be *merged* by hand, keeping the Windows form.

Find the pairs before touching anything, because they are the only sites that need thought:

```bash
grep -n 'cfg(not(windows))' src-tauri/src/audio_toolkit/audio/recorder.rs \
  src-tauri/src/managers/audio.rs src-tauri/src/managers/transcription.rs \
  src-tauri/src/commands/audio.rs src-tauri/src/lib.rs
```

Each hit is one half of a pair. At the time of writing `recorder.rs` has three in the system-audio path — Step 2 walks all three — but **verify against the current file rather than trusting that count**, and handle any the grep turns up that Step 2 does not name.

Then inventory the rest:

```bash
grep -n 'cfg(windows)' \
  src-tauri/src/audio_toolkit/audio/recorder.rs \
  src-tauri/src/audio_toolkit/audio/mod.rs \
  src-tauri/src/managers/audio.rs \
  src-tauri/src/managers/transcription.rs \
  src-tauri/src/commands/audio.rs \
  src-tauri/src/lib.rs > /tmp/gates.txt
wc -l /tmp/gates.txt
```

Classify each line as portable-machinery (delete the gate) or platform-specific (leave alone). The portable set is: `SystemAudioCapture`, `LoopbackChunk`, `LoopbackPumpCmd`, `SystemAudioSession`, `build_loopback_stream`, `build_loopback_stream_typed`, `downmix_loopback`, `run_loopback_pump`, `with_system_vad`, `with_system_audio_callback`, the `system_audio` parameter of `open()`, every `system_*` channel/field/branch inside `open()`/`stop()`/`close()`, `PendingSystemAudioCapture`, `system_stream_router`, `pending_system_audio`, `get_effective_system_audio_device`, `update_system_audio_capture`, `set_system_stream_router`, `SystemAudioTranscription`, `StreamSource::System`.

- [ ] **Step 2: Delete the gates in `recorder.rs`, merging the three pairs**

Remove `#[cfg(windows)]` from every portable-machinery site. **Three** sites are pairs and need merging rather than deletion:

`open()`'s dual return (`recorder.rs:451-454`) currently reads:

```rust
                #[cfg(windows)]
                return Ok((stream, loopback_stream, sample_rate));
                #[cfg(not(windows))]
                Ok((stream, sample_rate))
```

Collapse to the single three-tuple form:

```rust
                Ok((stream, loopback_stream, sample_rate))
```

and delete the now-duplicated `#[cfg(not(windows))]` arm of the `match init_result` below it, keeping the three-tuple arm.

`stop()`'s dual `system` binding (`recorder.rs:698-712`) currently reads:

```rust
        #[cfg(windows)]
        let system = match system_response { ... };
        #[cfg(not(windows))]
        let system = Vec::new();
```

Keep only the `match system_response` form.

**Pair 3 — `open()`'s init result (`recorder.rs:570-582`).** This is the pair most easily missed, and Task 5 depends on getting it right. It currently reads:

```rust
        match init_rx.recv() {
            Ok(Ok(system_audio_active)) => {
                #[cfg(not(windows))]
                let _ = system_audio_active;
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                #[cfg(windows)]
                {
                    self.system_cmd_tx = system_audio_active.then_some(system_cmd_tx);
                    self.loopback_pump_tx = system_audio_active.then_some(loopback_pump_tx);
                }
                self.worker_handle = Some(worker);
                Ok(())
            }
```

Merge to the Windows form, dropping the `let _ =` discard:

```rust
        match init_rx.recv() {
            Ok(Ok(system_audio_active)) => {
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                self.system_cmd_tx = system_audio_active.then_some(system_cmd_tx);
                self.loopback_pump_tx = system_audio_active.then_some(loopback_pump_tx);
                self.worker_handle = Some(worker);
                Ok(())
            }
```

- [ ] **Step 3: Delete the gates in the remaining files**

Same treatment for `audio_toolkit/audio/mod.rs` (the `SystemAudioCapture` re-export), `managers/audio.rs` (import, `PendingSystemAudioCapture`, `create_audio_recorder`'s parameter and system-VAD block, the struct fields and their initializers, `get_effective_system_audio_device`, the `start_microphone_stream` call site, `update_system_audio_capture`, `set_system_stream_router`), `managers/transcription.rs:550,628`, `commands/audio.rs:5-8`, and the `SystemAudioTranscription` registration in `lib.rs`.

- [ ] **Step 4: Make device resolution return `None` off-Windows for now**

`get_effective_system_audio_device` in `managers/audio.rs` now compiles everywhere, but only Windows has a meaningful implementation. Plans B and C fill in the other two. Until then, make the absence explicit rather than accidental.

Note the shape carefully: the early return must come **after** the existing `if !enabled` guard (which already uses `enabled`), and the existing body must be wrapped in `#[cfg(windows)]`. Placing an unconditional `return None` above live code instead would trip `unreachable_code` and fail `clippy -D warnings`.

```rust
    fn get_effective_system_audio_device(
        &self,
        enabled: bool,
        device_name: Option<&str>,
    ) -> Option<SystemAudioCapture> {
        if !enabled {
            return None;
        }

        // Linux and macOS device resolution land in Plans B and C. Until then
        // the machinery is compiled but inert on those platforms: no device
        // resolves, so `open()` runs microphone-only exactly as before.
        #[cfg(not(windows))]
        {
            let _ = device_name;
            None
        }

        #[cfg(windows)]
        {
            // ...the existing body, unchanged...
        }
    }
```

- [ ] **Step 5: De-gate the Tauri commands entirely**

`change_system_audio_enabled_setting` and `set_system_audio_device` in `commands/audio.rs` each split into a `#[cfg(windows)]` body and a `#[cfg(not(windows))]` arm returning "only available on Windows". Delete both `#[cfg]` arms in each, keeping the former Windows body as the only body — every type it touches is now available everywhere.

The commands are safe to run on Linux/macOS at this point because device resolution returns `None` there (Step 4), so they configure a lane that never opens. Task 6 adds the availability gate that makes that state legible.

- [ ] **Step 6: Verify all three targets compile**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: PASS on your development platform. If you have access to another platform (or a cross-check target), run it there too — the whole point of this task is that the code compiles off-Windows, and only a non-Windows build proves it.

- [ ] **Step 7: Run the test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS. Note that `managers/transcription.rs`'s dual-speaker merge tests (which exercise `StreamSource::System`) now run on every platform rather than Windows only — that is the intended effect.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src
git commit -m "refactor(audio): make system-audio machinery platform-neutral"
```

---

### Task 5: Surface whether the loopback stream actually opened

**Files:**
- Modify: `src-tauri/src/audio_toolkit/audio/recorder.rs` (`open()`, `get_preferred_loopback_config`)
- Modify: `src-tauri/src/managers/audio.rs` (`start_microphone_stream`, `update_system_audio_capture`)
- Modify: `src-tauri/src/audio_toolkit/utils.rs` and `src-tauri/src/audio_toolkit/mod.rs`

**Interfaces:**
- Produces:
  - `AudioRecorder::open(...) -> Result<bool, Box<dyn std::error::Error>>` — the `bool` is "the system-audio loopback stream opened successfully", already computed internally today and currently thrown away.
  - `AudioRecordingManager::system_audio_active(&self) -> bool` — the last open's loopback outcome.
  - `pub fn get_system_audio_host() -> Option<cpal::Host>` — the host loopback capture should use.

**Why this task exists.** `open()` deliberately swallows loopback failures: `recorder.rs:417-436` catches a failed `build_loopback_stream`, logs a warning and continues microphone-only, so `open()` returns `Ok` whether or not system audio actually started. That is correct behaviour — a broken loopback must never break dictation — but it means **no caller can tell whether system audio is working**. Plan C's permission detection depends entirely on that signal, and without this task it would silently observe "success" on every denied attempt. The value already exists (`init_tx.send(Ok(loopback_stream.is_some()))`, `recorder.rs:516`); it is discarded at `recorder.rs:571`. This task plumbs it out.

- [ ] **Step 1: Return the flag from `open()`**

Change the signature:

```rust
    /// Opens the capture stream(s). The returned flag reports whether the
    /// system-audio loopback stream opened; `false` means capture is running
    /// microphone-only, which is a normal degraded state rather than an error
    /// (a missing device, or a denied OS permission, both land here).
    pub fn open(
        &mut self,
        device: Option<Device>,
        system_audio: Option<SystemAudioCapture>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
```

In the `Ok(Ok(system_audio_active))` arm merged in Task 4 Step 2, return the flag instead of `()`:

```rust
                self.worker_handle = Some(worker);
                Ok(system_audio_active)
```

Fix the two other `Ok(())` returns in this function: the already-open early return at `recorder.rs:256` should report the current state rather than inventing one — return the flag you now store (see Step 2).

- [ ] **Step 2: Remember the flag on the recorder**

Add a field to `AudioRecorder`, defaulting to `false`, set from the init result in `open()`:

```rust
    /// Whether the most recent `open()` brought up the loopback stream.
    system_audio_active: bool,
```

Have the "already open" early return in `open()` return `Ok(self.system_audio_active)`, and reset it to `false` in `close()`.

- [ ] **Step 3: Make loopback config resolution platform-aware**

`get_preferred_loopback_config` (`recorder.rs:976-984`) currently reads:

```rust
    #[cfg(windows)]
    fn get_preferred_loopback_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // WASAPI render endpoints reject input-config enumeration even though
        // cpal can open them for shared-mode loopback. Their output default is
        // therefore the authoritative loopback format.
        Ok(device.default_output_config()?)
    }
```

That comment states a **WASAPI-specific** reason, so the `default_output_config()` call must not be assumed correct elsewhere — on a backend where the loopback device is an ordinary input (a PulseAudio monitor source, for instance), querying an output config can fail outright. Ungate the function and make the fallback explicit:

```rust
    fn get_preferred_loopback_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // WASAPI render endpoints reject input-config enumeration even though
        // cpal can open them for shared-mode loopback, so their output default
        // is the authoritative loopback format. Other backends may expose the
        // loopback endpoint as an ordinary input device instead, where the
        // output query fails — fall back to the input config rather than
        // assuming either shape.
        match device.default_output_config() {
            Ok(config) => Ok(config),
            Err(output_error) => device.default_input_config().map_err(|input_error| {
                format!(
                    "no loopback config: output query failed ({output_error}), \
                     input query failed ({input_error})"
                )
                .into()
            }),
        }
    }
```

Plan B verifies empirically which branch each Linux backend actually takes.

- [ ] **Step 4: Add the loopback host resolver**

Append to `src-tauri/src/audio_toolkit/utils.rs`:

```rust
/// Returns the CPAL host to use for system-audio (loopback) capture, or `None`
/// when this machine cannot support it.
///
/// Deliberately NOT `get_cpal_host()`: that pins ALSA on Linux for microphone
/// capture and playback, and ALSA cannot see the per-sink monitor sources
/// loopback needs. On Windows and macOS the default host is already correct.
///
/// The Linux arm returns `None` until Phase B enables cpal's `pipewire` and
/// `pulseaudio` features — referencing those `HostId` variants before the
/// features exist would not compile.
pub fn get_system_audio_host() -> Option<cpal::Host> {
    #[cfg(target_os = "linux")]
    {
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(get_cpal_host())
    }
}
```

macOS gets a real host immediately — its default host is CoreAudio, which is the one with Process Tap loopback — so Phase C has no host work to do. Only Linux needs the deferred arm, and only because of the feature gate.

Export it from `src-tauri/src/audio_toolkit/mod.rs` next to `get_cpal_host`.

- [ ] **Step 5: Thread the flag through the manager**

In `managers/audio.rs`, `start_microphone_stream` calls `rec.open(...)` in two places (the initial attempt and the retry after re-resolving a stale device). Capture the returned flag from whichever call succeeds and store it on `AudioRecordingManager`:

```rust
    /// Whether the current stream has a live system-audio lane. `false` when
    /// system audio is disabled, its device is missing, or the OS refused it.
    system_audio_active: Arc<AtomicBool>,
```

Add the accessor:

```rust
    pub fn system_audio_active(&self) -> bool {
        self.system_audio_active.load(Ordering::Acquire)
    }
```

- [ ] **Step 6: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml \
  && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings \
  && cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all pass. Every `open()` call site must now handle a `bool` return.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src
git commit -m "feat(audio): report whether the loopback stream opened"
```

---

### Task 6: Shared availability plumbing

**Files:**
- Modify: `src-tauri/src/commands/audio.rs`
- Modify: `src-tauri/src/lib.rs` (`collect_commands![...]`, near line 770)
- Modify: `src/components/settings/advanced/SystemAudioCapture.tsx:22-24`
- Modify: `src/components/settings/advanced/SystemAudioDeviceSelector.tsx:26-30`
- Modify: `src/shorthand/settings/ModesSettings.tsx` (~line 415)
- Test: `src-tauri/src/commands/audio.rs` (inline `#[cfg(test)]` module)

**Interfaces:**
- Produces:
  - `SystemAudioAvailability` — `Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type`, `#[serde(rename_all = "snake_case")]`, variants `Available`, `UnavailableNoSoundServer`, `PermissionDenied`.
  - `pub async fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability`

This lives here, not in B or C, because both platforms need it and duplicating it across two phases is how the two drift. Phases B and C then only supply their own answer to "is it available", not the mechanism.

At the end of this task the feature reports **unavailable on Linux and macOS** — correct, since neither resolves a device yet — and unchanged on Windows.

- [ ] **Step 1: Write the failing test**

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

- [ ] **Step 3: Implement**

Near the existing `PermissionAccess` enum in `src-tauri/src/commands/audio.rs`:

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

/// Async deliberately: constructing a PulseAudio host can block for seconds
/// while it waits on the server socket, and Tauri runs non-async commands on
/// the main thread — a synchronous version would stall the UI.
#[tauri::command]
#[specta::specta]
pub async fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability {
    let _ = &app; // Phase C reads macOS permission state from here.
    tokio::task::spawn_blocking(|| {
        availability_from_host_probe(crate::audio_toolkit::get_system_audio_host().is_some())
    })
    .await
    .unwrap_or(SystemAudioAvailability::UnavailableNoSoundServer)
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cargo test --manifest-path src-tauri/Cargo.toml system_audio_availability_tests
```

Expected: PASS (2 tests).

- [ ] **Step 5: Register and regenerate bindings**

Add `commands::audio::get_system_audio_availability,` to `collect_commands![...]` in `src-tauri/src/lib.rs`, then `bun run tauri dev` briefly and confirm `src/bindings.ts` gained the command and the `SystemAudioAvailability` type.

- [ ] **Step 6: Gate the three UI surfaces on availability, not OS**

All three currently early-return on `useOsType() !== "windows"`. Replace that check in each with a shared hook so the probe runs once rather than three times. Add `src/hooks/useSystemAudioAvailability.ts`:

```tsx
import { useCallback, useEffect, useState } from "react";
import { commands, type SystemAudioAvailability } from "@/bindings";

/// `null` while the probe is in flight. `refresh` must be called after any
/// capture attempt: on macOS (Phase C) permission state is only ever learned
/// by attempting, so a stale value would hide a denial.
export function useSystemAudioAvailability() {
  const [availability, setAvailability] =
    useState<SystemAudioAvailability | null>(null);

  const refresh = useCallback(
    () => commands.getSystemAudioAvailability().then(setAvailability),
    [],
  );

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { availability, refresh };
}
```

In each of `SystemAudioCapture.tsx`, `SystemAudioDeviceSelector.tsx` and `ModesSettings.tsx`, replace the `osType` early-return with:

```tsx
  const { availability, refresh: refreshAvailability } =
    useSystemAudioAvailability();

  if (availability === null || availability === "unavailable_no_sound_server") {
    return null;
  }
```

and remove the now-unused `useOsType` import where nothing else needs it. In `ModesSettings.tsx`, read the surrounding code first — if its `osType` check guards more than system audio, change only the system-audio part.

In `SystemAudioCapture.tsx`, also make the toggle refresh afterwards, so a macOS denial in Phase C becomes visible:

```tsx
      onChange={async (enabled) => {
        await updateSetting("system_audio_enabled", enabled);
        await refreshAvailability();
      }}
```

- [ ] **Step 7: Verify the frontend**

```bash
bun run build && bun run lint
```

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src src/bindings.ts src/hooks src/components src/shorthand
git commit -m "feat(audio): gate system audio on runtime availability"
```

---

### Task 7: Windows regression verification

No files change. This is the gate that proves the phase shipped no behaviour change.

Run on a Windows machine with a working system-audio setup:

- [ ] **Build and launch**: `bun run tauri dev`
- [ ] **System audio still captures**: play audio from another app, enable system audio capture, record, and confirm the transcript contains both the microphone speaker and the played audio, labelled `me` and `them` as before.
- [ ] **Device selection still works**: change the system-audio output device in settings and confirm capture follows it.
- [ ] **Toggling off still works**: disable system audio, record, confirm microphone-only output and no errors in the log.
- [ ] **Follow-stream still emits both speakers**: run `handy --follow-stream` against a dual-speaker session and confirm `"speaker":"me"` and `"speaker":"them"` events both appear.
- [ ] **Audio feedback still plays**: confirm start/stop sounds play (this exercises the rodio/cpal boundary from Task 1) on both the default device and an explicitly selected output device.
- [ ] **The loopback flag reports honestly** (Task 5): with system audio enabled and working, confirm `system_audio_active()` is true; then select a system-audio device and physically remove/disable it so the loopback stream fails to open, and confirm the flag reports false while dictation still works microphone-only. This signal is what Plan C's permission detection rests on, so a false "true" here would silently break that plan.
- [ ] **No new warnings**: check the debug log for cpal-related warnings absent before the migration.
- [ ] **Device selection survived the `name()` → `description()` swap**: confirm previously-saved microphone and system-audio device selections still resolve, rather than silently falling back to the default. A mismatch here means the persisted key changed, which Task 2 explicitly forbids in this phase.

Then, on Linux and macOS — the point is only that nothing is broken or misleading, not that the feature works:

- [ ] **The app builds and runs**, and ordinary dictation works.
- [ ] **System audio reports unavailable**: the toggle, device selector and modes row do not render, and no error is surfaced. This is the expected inert state until Phases B and C.

Any Windows difference is a defect in Tasks 1–6 and must be fixed before Phase B or C begins.
