# Plan A — cpal 0.18 migration and de-platforming

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the app to cpal 0.18 and make the fork-only system-audio machinery compile on all three platforms, with **zero user-visible change** — Windows system audio must behave exactly as it does today, and Linux/macOS gain the code without yet gaining the feature.

**Architecture:** Three coupled changes that only make sense together: (1) re-fork `cjpais/rodio` to bump its pinned cpal, because it exchanges `cpal::Device` values with the app and would otherwise produce two incompatible `Device` types; (2) bump the app's cpal 0.16 → 0.18 and fix the resulting breaking-API fallout; (3) delete `#[cfg(windows)]` from the portable system-audio machinery so it compiles unconditionally. Raising the macOS deployment floor to 14.6 is forced by (2) and is a deliberate, spec-recorded product decision.

**Tech Stack:** Rust, `cpal` 0.18.x, `rodio` (forked), Tauri 2.x.

**Spec:** `docs/superpowers/specs/2026-08-26-system-audio-capture-linux-macos-design.md`

**This plan is a prerequisite** for Plan B (Linux) and Plan C (macOS). Neither can start until this one is green.

## Global Constraints

- **Windows behaviour must not change.** This plan ships no feature. Any observable difference in Windows system-audio capture is a bug in this plan, not an improvement.
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
- Preserve behaviour exactly, especially the sample-format match arms in `build_stream` and `build_loopback_stream` (`recorder.rs:360-409`, `recorder.rs:856-898`) and the channel-averaging logic in `build_stream`'s callback (`recorder.rs:795-816`).
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

- [ ] **Step 1: Inventory the gates**

```bash
grep -n 'cfg(windows)\|cfg(not(windows))' \
  src-tauri/src/audio_toolkit/audio/recorder.rs \
  src-tauri/src/audio_toolkit/audio/mod.rs \
  src-tauri/src/managers/audio.rs \
  src-tauri/src/managers/transcription.rs \
  src-tauri/src/commands/audio.rs \
  src-tauri/src/lib.rs > /tmp/gates.txt
wc -l /tmp/gates.txt
```

Classify each line as portable-machinery (delete the gate) or platform-specific (leave alone). The portable set is: `SystemAudioCapture`, `LoopbackChunk`, `LoopbackPumpCmd`, `SystemAudioSession`, `build_loopback_stream`, `build_loopback_stream_typed`, `downmix_loopback`, `run_loopback_pump`, `with_system_vad`, `with_system_audio_callback`, the `system_audio` parameter of `open()`, every `system_*` channel/field/branch inside `open()`/`stop()`/`close()`, `PendingSystemAudioCapture`, `system_stream_router`, `pending_system_audio`, `get_effective_system_audio_device`, `update_system_audio_capture`, `set_system_stream_router`, `SystemAudioTranscription`, `StreamSource::System`.

- [ ] **Step 2: Delete the gates in `recorder.rs`**

Remove `#[cfg(windows)]` / `#[cfg(not(windows))]` from every portable-machinery site. Two sites need more than attribute deletion:

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

- [ ] **Step 3: Delete the gates in the remaining files**

Same treatment for `audio_toolkit/audio/mod.rs` (the `SystemAudioCapture` re-export), `managers/audio.rs` (import, `PendingSystemAudioCapture`, `create_audio_recorder`'s parameter and system-VAD block, the struct fields and their initializers, `get_effective_system_audio_device`, the `start_microphone_stream` call site, `update_system_audio_capture`, `set_system_stream_router`), `managers/transcription.rs:550,628`, `commands/audio.rs:5-8`, and the `SystemAudioTranscription` registration in `lib.rs`.

- [ ] **Step 4: Make device resolution return `None` off-Windows for now**

`get_effective_system_audio_device` in `managers/audio.rs` now compiles everywhere, but only Windows has a meaningful implementation. Plans B and C fill in the other two. Until then, make the absence explicit rather than accidental — at the top of the function body:

```rust
        // Linux and macOS device resolution land in Plans B and C. Until then
        // the machinery is compiled but inert on those platforms: no device
        // resolves, so `open()` runs microphone-only exactly as before.
        #[cfg(not(windows))]
        {
            let _ = (enabled, device_name);
            return None;
        }
```

- [ ] **Step 5: Keep the off-Windows command errors honest**

`change_system_audio_enabled_setting` and `set_system_audio_device` in `commands/audio.rs` still have `#[cfg(not(windows))]` arms returning "only available on Windows". Leave the arms in place for this plan (Plans B and C replace them), but correct the wording, since the gate is now about device resolution rather than the platform:

```rust
    #[cfg(not(windows))]
    Err("System audio capture is not yet available on this platform".to_string())
```

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

### Task 5: Windows regression verification

No files change. This is the gate that proves the plan shipped no behaviour change.

Run on a Windows machine with a working system-audio setup:

- [ ] **Build and launch**: `bun run tauri dev`
- [ ] **System audio still captures**: play audio from another app, enable system audio capture, record, and confirm the transcript contains both the microphone speaker and the played audio, labelled `me` and `them` as before.
- [ ] **Device selection still works**: change the system-audio output device in settings and confirm capture follows it.
- [ ] **Toggling off still works**: disable system audio, record, confirm microphone-only output and no errors in the log.
- [ ] **Follow-stream still emits both speakers**: run `handy --follow-stream` against a dual-speaker session and confirm `"speaker":"me"` and `"speaker":"them"` events both appear.
- [ ] **Audio feedback still plays**: confirm start/stop sounds play (this exercises the rodio/cpal boundary from Task 1) on both the default device and an explicitly selected output device.
- [ ] **No new warnings**: check the debug log for cpal-related warnings absent before the migration.

Any difference here is a defect in Tasks 1–4 and must be fixed before Plans B or C begin.
