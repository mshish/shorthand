# Crash reports and usage counts — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Report panics, three explicit error kinds, release-health sessions and two usage metrics to Sentry from the desktop app, behind one consent toggle that first-run onboarding offers (pre-set on) and that existing installs never see turned on without asking.

**Architecture:** The official `sentry` Rust crate is initialised at process start with a transport wrapper that drops every envelope while a process-wide consent flag is false. `setup()` opens the flag from the persisted setting; the Settings toggle flips it live. All instrumentation is in one fork-only module, `src-tauri/src/shorthand/telemetry.rs`; the frontend adds one onboarding step and one toggle under `src/shorthand/telemetry/`. Upstream files are touched at single lines.

**Tech Stack:** Rust `sentry` 0.49 (`backtrace`, `contexts`, `debug-images`, `panic`, `release-health`, `metrics`, `transport`), `uuid` (already transitive at 1.21), React + i18next, tauri-specta bindings, Playwright.

**Spec:** `docs/superpowers/specs/2026-09-03-telemetry-design.md`

## Global Constraints

- Sentry DSN (compile-time constant): `https://1129753428c5ab96fa90c03f550a1cc4@o4512022807969792.ingest.us.sentry.io/4512023072473088`. Project `shorthand-app`, org `shorthand-f4`.
- Release string: `shorthand@<CARGO_PKG_VERSION>`. Environment: `production`. No client at all under `debug_assertions`.
- `telemetry_enabled` serde default `false`; `telemetry_install_id` default `None`. `get_default_settings()` returns the same. No migration writes `true`.
- With consent off, zero bytes leave the machine. The transport is the gate; sessions start on consent-on and end before consent-off.
- Never send: audio, transcripts, notes, file names or paths, API keys, hostname, name, email, IP. `send_default_pii: false`, `server_name: None`, no `log` integration.
- Exactly two metrics: `capture.completed` (counter; `mode`, `model`, `outcome`) and `capture.duration_seconds` (distribution; `mode`). `model` is the catalogue id or the literal `custom`.
- Fork-only strings go in `src/shorthand/locales/en.json`, never `src/i18n/locales/`. Copy is fixed by the spec § "Consent step" and § "Settings toggle".
- Edits to upstream files stay to the single lines named per task. New code lives in fork-only files.
- Commit prefix conventions: `feat:`, `fix:`, `docs:`, `test:`, `chore:`. Never `git add -A`.
- Working tree: `D:/tools/shorthand-repos/shorthand-app/.worktrees/telemetry`, branch `feat/telemetry`. Rust commands run from `src-tauri/`.

---

### Task 1: Settings fields

**Files:**

- Modify: `src-tauri/src/settings.rs` — struct `AppSettings` (~line 345, after `show_whats_new_on_update`), default fns (~line 556, after `default_update_checks_enabled`), `get_default_settings()` (~line 1023), tests module (after `frozen_v0_9_store_parses_strictly_then_migrates_schema_two_fields`, ~line 1495).

**Interfaces:**

- Produces: `AppSettings::telemetry_enabled: bool`, `AppSettings::telemetry_install_id: Option<String>`.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `settings.rs`, directly after `frozen_v0_9_store_parses_strictly_then_migrates_schema_two_fields`:

```rust
    /// Existing installs predate the consent toggle. A store without the key
    /// must load as "off": nothing may opt an existing user in.
    #[test]
    fn store_without_telemetry_keys_loads_as_opted_out() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.remove("telemetry_enabled");
        map.remove("telemetry_install_id");
        let settings: AppSettings = serde_json::from_value(stored).unwrap();
        assert!(!settings.telemetry_enabled);
        assert_eq!(settings.telemetry_install_id, None);
    }

    /// A fresh install is also off until the consent step writes a value.
    #[test]
    fn default_settings_are_opted_out_of_telemetry() {
        let settings = get_default_settings();
        assert!(!settings.telemetry_enabled);
        assert_eq!(settings.telemetry_install_id, None);
    }
```

`default_settings_json()` already exists in the tests module (used by `salvage_preserves_valid_fields_when_one_value_is_invalid`).

- [ ] **Step 2: Run tests to verify they fail**

Run from `src-tauri/`: `cargo test settings::tests::store_without_telemetry_keys_loads_as_opted_out settings::tests::default_settings_are_opted_out_of_telemetry`
Expected: compile error, `no field telemetry_enabled`.

- [ ] **Step 3: Add the fields**

In `AppSettings`, after `pub show_whats_new_on_update: bool,`:

```rust
    /// Fork-only. Consent to send crash reports and usage counts; see
    /// TELEMETRY.md. Defaults to `false` so a store that predates the toggle
    /// loads as opted out. Only the first-run consent step or the Settings
    /// toggle writes `true`.
    #[serde(default = "default_telemetry_enabled")]
    pub telemetry_enabled: bool,
    /// Fork-only. Random id attached to telemetry as `user.id` so release
    /// health can count installs. Set when consent turns on, cleared when it
    /// turns off, so opting back in produces an unlinked identity.
    #[serde(default)]
    pub telemetry_install_id: Option<String>,
```

After `fn default_update_checks_enabled()`:

```rust
fn default_telemetry_enabled() -> bool {
    false
}
```

In `get_default_settings()`, after `show_whats_new_on_update: default_show_whats_new_on_update(),`:

```rust
        telemetry_enabled: default_telemetry_enabled(),
        telemetry_install_id: None,
```

- [ ] **Step 4: Run the settings tests**

Run: `cargo test settings::`
Expected: all pass, including the frozen v0.9 store test (its fixture has no telemetry keys; serde defaults cover it).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "feat: add telemetry consent fields, off by default for existing stores"
```

---

### Task 2: Telemetry module — gated transport, init, consent

**Files:**

- Modify: `src-tauri/Cargo.toml` — `[dependencies]`, after `tauri-plugin-updater`.
- Create: `src-tauri/src/shorthand/telemetry.rs`
- Modify: `src-tauri/src/shorthand/mod.rs` — add `pub mod telemetry;`
- Modify: `src-tauri/src/lib.rs` — `run()` after `portable::init();` (~line 727); `collect_commands!` (~line 735); `setup()` after `let mut initial_settings = settings::get_settings(app_handle);` (~line 217).
- Modify: `src/stores/settingsStore.ts` — the command map (~line 118), after `show_whats_new_on_update`.
- Regenerate: `src/bindings.ts` (tauri-specta writes it on a debug run).

**Interfaces:**

- Produces:
  - `telemetry::init() -> Option<sentry::ClientInitGuard>`
  - `telemetry::set_consent(app: &tauri::AppHandle, enabled: bool)` — opens/closes the gate, manages the session and the install id, persists the id.
  - `telemetry::change_telemetry_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String>` (Tauri command; TS name `changeTelemetryEnabledSetting`).
  - `telemetry::consented() -> bool`.
- Consumes: `AppSettings::telemetry_enabled`, `AppSettings::telemetry_install_id` (Task 1); `settings::get_settings`, `settings::write_settings`.

- [ ] **Step 1: Add the dependency**

In `src-tauri/Cargo.toml` `[dependencies]`, after the `tauri-plugin-updater` line:

```toml
# Fork-only: crash reports and usage counts, consent-gated. See TELEMETRY.md.
# Features are listed explicitly: `logs` and the `log` integration are left
# out because log lines are where transcript text appears in this codebase.
sentry = { version = "0.49", default-features = false, features = [
    "backtrace",
    "contexts",
    "debug-images",
    "panic",
    "release-health",
    "metrics",
    "transport",
] }
uuid = { version = "1", features = ["v4"] }
```

Run: `cargo fetch` from `src-tauri/`. Expected: resolves without conflict (`uuid` 1.21 is already in the tree).

- [ ] **Step 2: Write the failing transport tests**

Create `src-tauri/src/shorthand/telemetry.rs` with the tests first:

```rust
//! Fork-only crash reports and usage counts, consent-gated. What is sent and
//! what never is are documented in TELEMETRY.md; this file is the only place
//! that talks to Sentry, so that document can be checked against one file.
//!
//! The gate is the transport: `GatedTransport` drops every envelope while
//! `CONSENT` is false, which covers events, sessions and metrics with one
//! check. The client exists from process start with the gate closed; `setup`
//! opens it from the stored setting; the Settings toggle flips it live.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Recording(Mutex<usize>);

    impl sentry::Transport for Recording {
        fn send_envelope(&self, _envelope: sentry::Envelope) {
            *self.0.lock().unwrap() += 1;
        }
    }

    fn envelope() -> sentry::Envelope {
        sentry::Envelope::from(sentry::protocol::Event::default())
    }

    #[test]
    fn gated_transport_drops_envelopes_while_closed_and_forwards_when_open() {
        let inner = Arc::new(Recording(Mutex::new(0)));
        let gate = Arc::new(AtomicBool::new(false));
        let transport = GatedTransport {
            inner: inner.clone(),
            gate: gate.clone(),
        };

        transport.send_envelope(envelope());
        assert_eq!(*inner.0.lock().unwrap(), 0, "closed gate must drop");

        gate.store(true, Ordering::Release);
        transport.send_envelope(envelope());
        assert_eq!(*inner.0.lock().unwrap(), 1, "open gate must forward");

        gate.store(false, Ordering::Release);
        transport.send_envelope(envelope());
        assert_eq!(*inner.0.lock().unwrap(), 1, "closing again must drop");
    }

    #[test]
    fn model_attribute_hides_custom_model_names() {
        assert_eq!(model_attribute("parakeet-tdt-0.6b-v3", false), "parakeet-tdt-0.6b-v3");
        assert_eq!(model_attribute("my-private-finetune", true), "custom");
    }

    #[test]
    fn install_id_is_generated_once_and_cleared_on_opt_out() {
        let first = next_install_id(None, true);
        assert!(first.is_some());
        assert_eq!(next_install_id(first.clone(), true), first, "kept while on");
        assert_eq!(next_install_id(first, false), None, "cleared when off");
        let again = next_install_id(None, true);
        assert!(again.is_some());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Add `pub mod telemetry;` to `src-tauri/src/shorthand/mod.rs` (after `pub mod obsidian;`).

Run: `cargo test shorthand::telemetry::`
Expected: compile errors for `GatedTransport`, `model_attribute`, `next_install_id`.

- [ ] **Step 4: Write the module**

Above the tests in `telemetry.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter};

use crate::settings;

/// Public by design; a DSN only identifies the project to send to.
const DSN: &str =
    "https://1129753428c5ab96fa90c03f550a1cc4@o4512022807969792.ingest.us.sentry.io/4512023072473088";

/// The consent flag as loaded from settings. Closed until `setup` reads the
/// store, so nothing sends during the window before settings exist.
static CONSENT: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// When the current capture began, for `capture.duration_seconds`.
static CAPTURE_STARTED: Mutex<Option<Instant>> = Mutex::new(None);

fn consent_flag() -> Arc<AtomicBool> {
    CONSENT
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

/// Whether the gate is open right now.
pub fn consented() -> bool {
    consent_flag().load(Ordering::Acquire)
}

/// Wraps the SDK's default transport and drops every envelope while the gate
/// is closed. One chokepoint for events, sessions and metrics alike.
struct GatedTransport {
    inner: Arc<dyn sentry::Transport>,
    gate: Arc<AtomicBool>,
}

impl sentry::Transport for GatedTransport {
    fn send_envelope(&self, envelope: sentry::Envelope) {
        if self.gate.load(Ordering::Acquire) {
            self.inner.send_envelope(envelope);
        }
    }

    fn flush(&self, timeout: std::time::Duration) -> bool {
        self.inner.flush(timeout)
    }

    fn shutdown(&self, timeout: std::time::Duration) -> bool {
        self.inner.shutdown(timeout)
    }
}

struct GatedTransportFactory {
    gate: Arc<AtomicBool>,
}

impl sentry::TransportFactory for GatedTransportFactory {
    fn create_transport_with_options(
        &self,
        options: sentry::TransportOptions,
    ) -> Arc<dyn sentry::Transport> {
        let inner = sentry::transports::DefaultTransportFactory
            .create_transport_with_options(options);
        Arc::new(GatedTransport {
            inner,
            gate: self.gate.clone(),
        })
    }
}

/// Creates the client. Returns `None` in debug builds, where nothing should
/// ever report. The guard must be held for the life of the process so the
/// transport flushes on exit; `run()` keeps it in a local.
pub fn init() -> Option<sentry::ClientInitGuard> {
    if cfg!(debug_assertions) {
        return None;
    }
    let options = sentry::ClientOptions {
        dsn: DSN.parse().ok(),
        release: Some(concat!("shorthand@", env!("CARGO_PKG_VERSION")).into()),
        environment: Some("production".into()),
        // Never the hostname.
        server_name: None,
        send_default_pii: false,
        auto_session_tracking: true,
        session_mode: sentry::SessionMode::Application,
        transport: Some(Arc::new(GatedTransportFactory {
            gate: consent_flag(),
        })),
        ..Default::default()
    };
    Some(sentry::init(options))
}

/// The next value of `telemetry_install_id` given the current one and the
/// consent being applied. Pure, so the on/off/on sequence is testable.
fn next_install_id(current: Option<String>, enabled: bool) -> Option<String> {
    match (enabled, current) {
        (false, _) => None,
        (true, Some(id)) => Some(id),
        (true, None) => Some(uuid::Uuid::new_v4().to_string()),
    }
}

/// Applies consent: opens or closes the gate, starts or ends the session,
/// and sets or clears the install id in both the Sentry scope and settings.
/// Called from `setup` with the stored value and from the toggle command.
pub fn set_consent(app: &AppHandle, enabled: bool) {
    let mut stored = settings::get_settings(app);
    let install_id = next_install_id(stored.telemetry_install_id.clone(), enabled);
    if stored.telemetry_install_id != install_id || stored.telemetry_enabled != enabled {
        stored.telemetry_install_id = install_id.clone();
        stored.telemetry_enabled = enabled;
        settings::write_settings(app, stored);
    }

    let gate = consent_flag();
    if enabled {
        sentry::configure_scope(|scope| {
            scope.set_user(install_id.map(|id| sentry::User {
                id: Some(id),
                ..Default::default()
            }));
        });
        gate.store(true, Ordering::Release);
        sentry::start_session();
    } else {
        // End first so the final session envelope is dropped, not sent.
        sentry::end_session();
        gate.store(false, Ordering::Release);
        sentry::configure_scope(|scope| scope.set_user(None));
    }
}

/// Fork-only Settings toggle. Lives here rather than in `shortcut/mod.rs`
/// so the upstream file gains no lines.
#[tauri::command]
#[specta::specta]
pub fn change_telemetry_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    set_consent(&app, enabled);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "telemetry_enabled", "value": enabled }),
    );
    Ok(())
}

/// The `model` attribute: the catalogue id, or `custom` so user-named
/// models never reach Sentry.
fn model_attribute(model_id: &str, is_custom: bool) -> String {
    if is_custom {
        "custom".to_string()
    } else {
        model_id.to_string()
    }
}

/// Marks the start of a capture. Called once per capture, next to
/// `mode::set_active`.
pub fn capture_started() {
    *CAPTURE_STARTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

/// Emits the two usage metrics for the capture that just produced (or failed
/// to produce) a transcription result. Cheap and safe to call with consent
/// off: the SDK buffers metrics and the transport drops them.
pub fn capture_completed(app: &AppHandle, ok: bool) {
    let mode = crate::shorthand::mode::Mode::from(crate::shorthand::mode::active(app));
    let mode = match mode {
        crate::shorthand::mode::Mode::Meeting => "meeting",
        crate::shorthand::mode::Mode::Dictation => "dictation",
        crate::shorthand::mode::Mode::AssistedNotes => "assisted_notes",
    };
    let settings = settings::get_settings(app);
    let is_custom = app
        .try_state::<crate::managers::model::ModelManager>()
        .and_then(|mm| mm.get_model_info(&settings.selected_model))
        .map(|info| info.is_custom)
        .unwrap_or(false);
    let model = model_attribute(&settings.selected_model, is_custom);

    sentry::metrics::counter("capture.completed", 1)
        .attribute("mode", mode)
        .attribute("model", model)
        .attribute("outcome", if ok { "ok" } else { "error" })
        .capture();

    let started = CAPTURE_STARTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(started) = started {
        sentry::metrics::distribution("capture.duration_seconds", started.elapsed().as_secs_f64())
            .attribute("mode", mode)
            .capture();
    }
}

/// Reports one of the enumerated error kinds. `detail` is sent verbatim, so a
/// caller passes it only after checking the text cannot carry a path or user
/// content; otherwise pass `None`.
pub fn report_error(kind: &'static str, detail: Option<&str>) {
    let message = match detail {
        Some(detail) => format!("{kind}: {detail}"),
        None => kind.to_string(),
    };
    sentry::with_scope(
        |scope| scope.set_tag("error.kind", kind),
        || sentry::capture_message(&message, sentry::Level::Error),
    );
}
```

Check the exact names before compiling: `crate::shorthand::mode::active(app)` returns `Mode` directly (see `mode.rs` line 105), so drop the `Mode::from` wrapper if it does; `ModelManager` is the type in `managers/model.rs` and `get_model_info` is at line 1251; `sentry::metrics::counter/distribution` builders take `.attribute(key, value)` and `.capture()` per the 0.49 docs. If `ModelManager` is registered under a different managed type name, use that.

- [ ] **Step 5: Run the module tests**

Run: `cargo test shorthand::telemetry::`
Expected: 3 pass.

- [ ] **Step 6: Wire `lib.rs`**

In `run()`, directly after `portable::init();`:

```rust
    // Fork-only: crash reports and usage counts. Held for the process
    // lifetime so the transport flushes on exit. Gate closed until setup.
    let _telemetry = shorthand::telemetry::init();
```

In `collect_commands![ ... ]`, add one entry (anywhere in the list; keep it next to the other `shorthand::` entries if there are any):

```rust
            shorthand::telemetry::change_telemetry_enabled_setting,
```

In `setup()`, directly after `let mut initial_settings = settings::get_settings(app_handle);`:

```rust
        shorthand::telemetry::set_consent(app_handle, initial_settings.telemetry_enabled);
```

Run: `cargo build` from `src-tauri/`. Expected: builds. Run `cargo clippy --all-targets -- -D warnings` and fix anything it reports in the new file.

- [ ] **Step 7: Regenerate bindings and map the store**

Run `bun run tauri dev` from the worktree root, wait for the window, quit. Confirm `src/bindings.ts` now contains `changeTelemetryEnabledSetting` and `telemetry_enabled: boolean` / `telemetry_install_id: string | null` in the `Settings` type. If the dev app cannot run on this machine, run any debug binary path that reaches `specta_builder.export` (it is unconditional under `debug_assertions` in `run()`), for example `cargo run -- --start-hidden` then quit from the tray.

In `src/stores/settingsStore.ts`, in the command map after the `show_whats_new_on_update` entry:

```ts
  telemetry_enabled: (value) =>
    commands.changeTelemetryEnabledSetting(value as boolean),
```

Run: `bun run build`. Expected: TypeScript clean.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/shorthand/telemetry.rs src-tauri/src/shorthand/mod.rs src-tauri/src/lib.rs src/bindings.ts src/stores/settingsStore.ts
git commit -m "feat: consent-gated Sentry client with transport gate and install id"
```

---

### Task 3: Instrument captures and the three error kinds

**Files:**

- Modify: `src-tauri/src/actions.rs` — line 588 (`mode::set_active`), and the line after `transcription_result` is computed (~line 1060, before `// Await WAV save and verify`).
- Modify: `src-tauri/src/managers/transcription.rs` — `load_model_with_device` (~line 876).
- Modify: `src-tauri/src/follow_stream/server.rs` — the `Err(error)` arm at ~line 94.

**Interfaces:**

- Consumes: `telemetry::capture_started()`, `telemetry::capture_completed(&AppHandle, bool)`, `telemetry::report_error(&'static str, Option<&str>)` (Task 2).

- [ ] **Step 1: Capture start and completion**

In `actions.rs`, directly after `crate::shorthand::mode::set_active(app, binding_id);` (line 588):

```rust
        crate::shorthand::telemetry::capture_started();
```

Directly after the `let transcription_result = match mic_stream_result { ... }.map(...)` statement ends (the `;` before the `// Await WAV save and verify` comment):

```rust
                    crate::shorthand::telemetry::capture_completed(&ah, transcription_result.is_ok());
                    if let Err(err) = &transcription_result {
                        // Engine error text: model/engine messages, no paths.
                        crate::shorthand::telemetry::report_error("transcription", Some(err));
                    }
```

Check the `Err` type of `transcription_result`: if it is `String`, `Some(err)` works via deref; if it is `anyhow::Error`, use `Some(&err.to_string())`. Then read the engine's error construction sites in `managers/transcription.rs` (`transcribe`, `finalize_stream_detailed`) and confirm none formats a file path into the message. If one does, pass `None` here and note why in a comment.

- [ ] **Step 2: Model load failure**

In `managers/transcription.rs`, `load_model_with_device` returns `Result<()>`. Rename the existing body into a private `load_model_with_device_inner` with the same signature and make the public function:

```rust
    pub fn load_model_with_device(
        &self,
        model_id: &str,
        device_index: Option<usize>,
    ) -> Result<()> {
        let result = self.load_model_with_device_inner(model_id, device_index);
        if result.is_err() {
            // Kind only: load errors name the model file on disk.
            crate::shorthand::telemetry::report_error("model_load", None);
        }
        result
    }
```

This is the one place the task adds more than a line to an upstream file; the rename keeps the diff to a wrapper rather than edits inside the body.

- [ ] **Step 3: Follow-stream listener failure**

In `follow_stream/server.rs`, inside the `Err(error) => {` arm after `log::error!(...)`:

```rust
                crate::shorthand::telemetry::report_error(
                    "follow_stream_listen",
                    // The io error kind only: the message can name the
                    // per-user socket path.
                    Some(&format!("{:?}", error.kind())),
                );
```

If `error` is not an `io::Error`, send `None`.

- [ ] **Step 4: Build, clippy, test**

Run from `src-tauri/`: `cargo build && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: clean; all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/actions.rs src-tauri/src/managers/transcription.rs src-tauri/src/follow_stream/server.rs
git commit -m "feat: report capture metrics and the three enumerated error kinds"
```

---

### Task 4: TELEMETRY.md, README, strings

**Files:**

- Create: `TELEMETRY.md`
- Modify: `README.md` — insert a `## Privacy` section after `## What Shorthand keeps` (line 62 block ends before `## Platform status`, line 71).
- Modify: `CLAUDE.md` — add one bullet to the "open it only when the work calls for it" list.
- Modify: `src/shorthand/locales/en.json` — add keys, keeping the file's alphabetical order.
- Modify: `D:/tools/shorthand-repos/CLAUDE.md` (workspace map, not in this repo) — fix the plugin path.

- [ ] **Step 1: Write `TELEMETRY.md`**

```markdown
# Crash reports and usage counts

Shorthand can send crash reports and a small number of usage counts to
[Sentry](https://sentry.io), so that problems get found and fixed. It is off
until you say otherwise: the first-run setup asks, with the switch pre-set to
on, and the answer can be changed at any time under **Settings → App → Send
crash reports and usage counts**. Installs that predate this switch stay off
unless you turn it on.

This file is the source of truth for what is sent. The code that sends it is
one module, `src-tauri/src/shorthand/telemetry.rs`, so the two can be checked
against each other.

## What is sent

**Crash and error reports**

- A Rust panic: the panic message and stack frames.
- One of three named failures, with a short kind and, where the text cannot
  contain a path, the engine's error message:
  `model_load` (kind only), `transcription`, `follow_stream_listen` (the I/O
  error kind only).
- With every report: Shorthand version, operating system name and version,
  CPU architecture, Rust version, and the time.

**Usage counts**

- `capture.completed`: one count per finished capture, with the mode
  (meeting, dictation, assisted notes), the transcription model's catalogue
  id (or `custom` for a model you added yourself), and whether it succeeded.
- `capture.duration_seconds`: how long the capture ran, with the mode.
- Sessions: that the app started and ended, and whether it crashed. This is
  what gives an active-install count and a crash-free rate per version.

**Identity**

- A random id generated when you turn the switch on. It links sessions from
  the same install and nothing else. Turning the switch off deletes it;
  turning it on again generates a new one.

## What is never sent

Audio, transcripts, notes, file names or paths, API keys, your computer's
name, your name, email address or IP address. IP addresses are additionally
not stored on the receiving side (Sentry's "prevent storing of IP addresses"
is on for the organisation).

There are no log breadcrumbs: Shorthand's logs can contain transcript text,
so they are deliberately kept out.

## How the switch works

Nothing is sent while the switch is off. The gate is at the network layer,
so events, sessions and usage counts alike are dropped rather than queued.
Development builds never report at all.
```

- [ ] **Step 2: README and CLAUDE.md**

In `README.md`, before `## Platform status`:

```markdown
## Privacy

Transcription is local. With your permission, Shorthand sends crash reports
and a few usage counts (which mode ran, which model, how long) to Sentry so
problems get fixed; it never sends audio, transcripts, notes, file paths, keys
or anything that identifies you. First-run setup asks; the switch is under
Settings → App. [TELEMETRY.md](TELEMETRY.md) lists exactly what is sent.
```

In `CLAUDE.md`, add to the bulleted list:

```markdown
- `TELEMETRY.md` — before touching `src-tauri/src/shorthand/telemetry.rs`,
  the consent step, or any log or error message that could reach Sentry. It
  is the user-facing promise of what is and is not sent; a change that makes
  it untrue is a privacy bug, not a docs bug.
```

In `D:/tools/shorthand-repos/CLAUDE.md`, change the plugin row's directory from `../obsidian-shorthand/` to `shorthand-obsidian-plugin/` and its description from "Not in this tree — a sibling at `D:/tools/obsidian-shorthand`" to "In this tree at `shorthand-obsidian-plugin/`". Leave the rest of the row.

- [ ] **Step 3: Strings**

Add to `src/shorthand/locales/en.json`, each in its alphabetical position:

```json
  "onboarding.telemetry.continue": "Continue",
  "onboarding.telemetry.intro": "Shorthand can send crash reports and a few usage counts so problems get found and fixed. It is anonymous, and you can change this any time under Settings → App.",
  "onboarding.telemetry.link": "See exactly what is sent",
  "onboarding.telemetry.never.body": "Audio, transcripts, notes, file names or paths, API keys, your name, email address or IP address.",
  "onboarding.telemetry.never.heading": "What is never sent",
  "onboarding.telemetry.sends.errors": "Crash and error reports: what failed and where in the app, with your operating system and Shorthand version.",
  "onboarding.telemetry.sends.heading": "What is sent",
  "onboarding.telemetry.sends.usage": "Usage counts: how many captures finish, in which mode, with which model, and how long they ran.",
  "onboarding.telemetry.title": "Help improve Shorthand",
  "onboarding.telemetry.toggle": "Send crash reports and usage counts",
  "settings.app.telemetry.description": "Anonymous crash reports and a few usage counts. Never audio, transcripts, notes or personal details.",
  "settings.app.telemetry.label": "Send crash reports and usage counts",
```

Run: `bun run check:fork-translations && bun run check:locale-drift && bun run check:translations`
Expected: all pass. Then, per `src/shorthand/locales/README.md`, run `cargo build` from `src-tauri/` because `build.rs` reads this file. Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add TELEMETRY.md README.md CLAUDE.md src/shorthand/locales/en.json
git commit -m "docs: state what telemetry sends and add its strings"
```

The workspace `CLAUDE.md` is outside this repository and unversioned; no commit.

---

### Task 5: Settings toggle

**Files:**

- Create: `src/shorthand/telemetry/TelemetryToggle.tsx`
- Modify: `src/shorthand/settings/AppSettings.tsx` — import and one line after `<ShowTrayIcon ... />`.

**Interfaces:**

- Consumes: `Settings.telemetry_enabled` via `useSettings().getSetting/updateSetting` (bound to the command in Task 2).
- Produces: `TelemetryToggle` component with `descriptionMode` and `grouped` props like the other toggles.

- [ ] **Step 1: Write the component**

```tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

interface TelemetryToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Fork-only. The consent switch for crash reports and usage counts; the
 * first-run step in `TelemetryOnboarding.tsx` writes the same setting.
 * Reads `?? false` deliberately: an absent key means an existing install,
 * which is opted out. TELEMETRY.md says what the switch controls.
 */
export const TelemetryToggle: React.FC<TelemetryToggleProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("telemetry_enabled") ?? false;

  return (
    <ToggleSwitch
      checked={enabled}
      onChange={(next) => updateSetting("telemetry_enabled", next)}
      isUpdating={isUpdating("telemetry_enabled")}
      label={t("settings.app.telemetry.label")}
      description={t("settings.app.telemetry.description")}
      descriptionMode={descriptionMode}
      grouped={grouped}
    />
  );
};
```

- [ ] **Step 2: Register it**

In `AppSettings.tsx`, add the import `import { TelemetryToggle } from "@/shorthand/telemetry/TelemetryToggle";` and, after `<ShowTrayIcon descriptionMode="tooltip" grouped={true} />`:

```tsx
<TelemetryToggle descriptionMode="tooltip" grouped={true} />
```

- [ ] **Step 3: Verify**

Run: `bun run lint && bun run build && bun run check:settings`
Expected: clean; `check:settings` reports the new component reachable.

- [ ] **Step 4: Commit**

```bash
git add src/shorthand/telemetry/TelemetryToggle.tsx src/shorthand/settings/AppSettings.tsx
git commit -m "feat: telemetry consent toggle in the App settings section"
```

---

### Task 6: First-run consent step

**Files:**

- Create: `src/shorthand/telemetry/TelemetryOnboarding.tsx`
- Modify: `src/App.tsx` — `OnboardingStep` union (line 25), `handleAccessibilityComplete` (~line 255), the step render chain (~line 300).

**Interfaces:**

- Produces: `TelemetryOnboarding` with `onComplete: () => void`.
- Consumes: `useSettings().getSetting/updateSetting`, `ShorthandWordmark` from `@/shorthand/brand`, `ToggleSwitch`, `openUrl` from `@tauri-apps/plugin-opener`.

- [ ] **Step 1: Write the step**

```tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ShorthandWordmark } from "@/shorthand/brand";
import { ToggleSwitch } from "@/components/ui/ToggleSwitch";
import { useSettings } from "@/hooks/useSettings";

const TELEMETRY_DOC_URL =
  "https://github.com/mshish/shorthand/blob/main/TELEMETRY.md";

interface TelemetryOnboardingProps {
  onComplete: () => void;
}

/**
 * Fork-only first-run consent for crash reports and usage counts. Shown to
 * new installs only, between permissions and model download. The switch is
 * pre-set to on; nothing is sent until Continue writes the choice, because
 * the stored default is off. TELEMETRY.md is the copy's source of truth.
 */
const TelemetryOnboarding: React.FC<TelemetryOnboardingProps> = ({
  onComplete,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  // A relaunch mid-onboarding shows the previously chosen position.
  const [enabled, setEnabled] = useState<boolean>(
    getSetting("telemetry_enabled") ?? true,
  );
  const [saving, setSaving] = useState(false);

  const handleContinue = async () => {
    setSaving(true);
    try {
      await updateSetting("telemetry_enabled", enabled);
    } finally {
      setSaving(false);
    }
    onComplete();
  };

  return (
    <div className="h-screen flex flex-col items-center justify-center p-8 select-none cursor-default">
      <div className="max-w-xl w-full space-y-6">
        <ShorthandWordmark />
        <h1 className="text-2xl font-semibold">
          {t("onboarding.telemetry.title")}
        </h1>
        <p className="text-mid-gray">{t("onboarding.telemetry.intro")}</p>
        <div className="space-y-2">
          <h2 className="font-medium">
            {t("onboarding.telemetry.sends.heading")}
          </h2>
          <ul className="list-disc pl-5 space-y-1 text-sm">
            <li>{t("onboarding.telemetry.sends.errors")}</li>
            <li>{t("onboarding.telemetry.sends.usage")}</li>
          </ul>
        </div>
        <div className="space-y-2">
          <h2 className="font-medium">
            {t("onboarding.telemetry.never.heading")}
          </h2>
          <p className="text-sm">{t("onboarding.telemetry.never.body")}</p>
        </div>
        <ToggleSwitch
          checked={enabled}
          onChange={setEnabled}
          label={t("onboarding.telemetry.toggle")}
          description=""
          descriptionMode="inline"
        />
        <button
          type="button"
          className="text-sm underline text-mid-gray hover:text-text"
          onClick={() => openUrl(TELEMETRY_DOC_URL)}
        >
          {t("onboarding.telemetry.link")}
        </button>
        <div className="flex justify-end">
          <button
            type="button"
            data-testid="telemetry-continue"
            disabled={saving}
            className="px-4 py-2 rounded-lg bg-logo-primary text-white disabled:opacity-50"
            onClick={handleContinue}
          >
            {t("onboarding.telemetry.continue")}
          </button>
        </div>
      </div>
    </div>
  );
};

export default TelemetryOnboarding;
```

Match the class names to what `Onboarding.tsx` and `AccessibilityOnboarding.tsx` use for their wrapper, heading and primary button so the step looks like its neighbours; the fork's `src/shorthand/ui` may already export a `Button` — prefer it over a raw `<button>` if it exists. Keep `data-testid="telemetry-continue"` for Task 7.

- [ ] **Step 2: Wire `App.tsx`**

Change line 25:

```ts
type OnboardingStep = "accessibility" | "telemetry" | "model" | "done";
```

Add the import next to the onboarding imports:

```ts
import TelemetryOnboarding from "@/shorthand/telemetry/TelemetryOnboarding";
```

In `handleAccessibilityComplete`, replace `setOnboardingStep(isReturningUser ? "done" : "model");` with:

```ts
setOnboardingStep(isReturningUser ? "done" : "telemetry");
```

Add after `handleAccessibilityComplete`:

```ts
const handleTelemetryComplete = () => {
  setOnboardingStep("model");
};
```

In the render chain, between the `accessibility` and `model` branches:

```tsx
  } else if (onboardingStep === "telemetry") {
    content = <TelemetryOnboarding onComplete={handleTelemetryComplete} />;
```

- [ ] **Step 3: Verify**

Run: `bun run lint && bun run build && bun run check:fork-translations`
Expected: clean. ESLint's no-hardcoded-strings rule must pass; every visible string goes through `t()`.

- [ ] **Step 4: Commit**

```bash
git add src/shorthand/telemetry/TelemetryOnboarding.tsx src/App.tsx
git commit -m "feat: first-run consent step for crash reports and usage counts"
```

---

### Task 7: Playwright coverage for the consent step

**Files:**

- Create: `tests/telemetry-onboarding.spec.ts`

**Interfaces:**

- Consumes: `data-testid="telemetry-continue"` (Task 6); command names `get_app_settings`, `change_telemetry_enabled_setting` as invoked through `window.__TAURI_INTERNALS__.invoke`.

- [ ] **Step 1: Write the spec**

This is the first fork-only Playwright spec that stubs the Tauri bridge, as `docs/FRONTEND_TESTING.md` prescribes. It records every `invoke` so assertions read the calls rather than the UI.

```ts
import { test, expect, type Page } from "@playwright/test";

/**
 * Fork-only. Stubs `window.__TAURI_INTERNALS__` so the app boots under plain
 * Vite, with a fresh profile (`onboarding_completed: false`) on a platform
 * with no permission step, and records every command the UI invokes.
 */
async function bootFreshProfile(page: Page, telemetryEnabled?: boolean) {
  await page.addInitScript((telemetryEnabled) => {
    const calls: Array<{ cmd: string; args: unknown }> = [];
    const settings: Record<string, unknown> = {
      onboarding_completed: false,
      selected_model: "",
      bindings: {},
      post_process_providers: [],
      post_process_prompts: [],
      custom_words: [],
      ...(telemetryEnabled === undefined
        ? {}
        : { telemetry_enabled: telemetryEnabled }),
    };
    (
      window as unknown as { __TAURI_INTERNALS__: unknown }
    ).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, windows: [] },
      plugins: { os: { platform: "linux" } },
      transformCallback: () => 0,
      invoke: async (cmd: string, args: unknown) => {
        calls.push({ cmd, args });
        switch (cmd) {
          case "get_app_settings":
            return { status: "ok", data: settings };
          case "change_telemetry_enabled_setting":
            settings.telemetry_enabled = (args as { enabled: boolean }).enabled;
            return { status: "ok", data: null };
          case "plugin:os|platform":
            return "linux";
          case "plugin:event|listen":
            return 0;
          default:
            return { status: "ok", data: null };
        }
      },
    };
    (window as unknown as { __calls: unknown }).__calls = calls;
  }, telemetryEnabled);
  await page.goto("/");
}

async function recordedCalls(page: Page) {
  return page.evaluate(
    () =>
      (window as unknown as { __calls: Array<{ cmd: string; args: unknown }> })
        .__calls,
  );
}

test.describe("telemetry consent step", () => {
  test("a fresh profile reaches the step with the switch on and Continue writes true", async ({
    page,
  }) => {
    await bootFreshProfile(page);
    const cont = page.getByTestId("telemetry-continue");
    await expect(cont).toBeVisible();
    await expect(page.getByRole("checkbox")).toBeChecked();
    await cont.click();
    const calls = await recordedCalls(page);
    expect(calls).toContainEqual({
      cmd: "change_telemetry_enabled_setting",
      args: { enabled: true },
    });
  });

  test("switching off before Continue writes false", async ({ page }) => {
    await bootFreshProfile(page);
    await page.getByRole("checkbox").click();
    await page.getByTestId("telemetry-continue").click();
    const calls = await recordedCalls(page);
    expect(calls).toContainEqual({
      cmd: "change_telemetry_enabled_setting",
      args: { enabled: false },
    });
  });

  test("a relaunch mid-onboarding shows the stored choice", async ({
    page,
  }) => {
    await bootFreshProfile(page, false);
    await expect(page.getByRole("checkbox")).not.toBeChecked();
  });
});
```

The stubbed command list will need extending: run the spec, read the first command that the app invokes and the stub does not answer sensibly (the recorded `calls` array shows them), and add a canned response. Commands that only need to succeed fall through to the default arm. `ToggleSwitch` may render an `<input type="checkbox">` or a `role="switch"` button; check `src/components/ui/ToggleSwitch.tsx` and adjust the locator (`getByRole("switch")`) to match. If other commands must return shaped data (`get_available_models`, audio devices), return empty arrays under `{ status: "ok", data: [] }`.

- [ ] **Step 2: Run it**

Run: `bunx playwright install chromium` once, then `bun run test:playwright -- tests/telemetry-onboarding.spec.ts`
Expected: 3 pass. Iterate on the stub until they do; do not weaken the assertions.

- [ ] **Step 3: Commit**

```bash
git add tests/telemetry-onboarding.spec.ts
git commit -m "test: cover the telemetry consent step under a stubbed Tauri bridge"
```

---

### Task 8: Server-side privacy setting, full verification, smoke run

**Files:** none in the repo.

- [ ] **Step 1: Prevent IP storage in Sentry**

In Sentry, org `shorthand-f4` → Settings → Security & Privacy → turn on **Prevent Storing of IP Addresses**. Via the Sentry MCP, search for an organisation-settings update tool and set `storeCrashReports`/IP-storage-related option if exposed; otherwise do it in the web UI at `https://shorthand-f4.sentry.io/settings/security-and-privacy/`. Confirm the toggle shows on. `TELEMETRY.md` states this, so it must be true before merge.

- [ ] **Step 2: Run every gate**

From the worktree root:

```bash
bun run lint && bun run build && bun run check:translations && bun run check:branding && bun run check:locale-drift && bun run check:fork-translations && bun run check:settings && bun run test:unit && bun run test:playwright
```

From `src-tauri/`:

```bash
cargo fmt -- --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Expected: all clean. Fix, do not skip.

- [ ] **Step 3: Smoke a release build end to end**

`bun run tauri build` (or `cargo build --release` from `src-tauri/` and run the binary) with a fresh profile: move `settings_store.json` aside from the app data dir first. Walk onboarding: the consent step appears after permissions, the switch is on, Continue advances to model download. Then in Sentry → Project `shorthand-app` → Releases, confirm a session for `shorthand@<version>` appears within a few minutes, and under Metrics that `capture.completed` arrives after one dictation. Flip the toggle off in Settings → App, run a capture, confirm no new session or metric arrives. Restore the original `settings_store.json`.

Record what was observed in the pull request description. If the dev machine cannot run a release build, say so there rather than claiming the smoke passed.

- [ ] **Step 4: Open the pull request**

```bash
git push -u origin feat/telemetry
gh pr create --title "feat: consent-gated crash reports and usage counts" --body "$(cat <<'EOF'
Implements docs/superpowers/specs/2026-09-03-telemetry-design.md.

- Official `sentry` Rust crate, one fork-only module, transport-level consent gate.
- First-run consent step (switch pre-set on), Settings → App toggle, existing installs stay off.
- Two metrics, three error kinds, panics, release-health sessions. TELEMETRY.md is the promise.

Smoke: <what was observed in Sentry, or why it could not be run>

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-review

- **Spec coverage:** settings fields (T1); SDK, features, gate, sessions, install id, DSN/release/environment, debug-off (T2); metrics and the three error kinds with per-site content review (T3); TELEMETRY.md, README, CLAUDE.md, workspace map fix, strings (T4); toggle and settings-coverage gate (T5); consent step and App.tsx transitions (T6); Playwright (T7); server-side IP setting and gates (T8). Out-of-scope items are not planned, as the spec says.
- **Type consistency:** `set_consent(&AppHandle, bool)`, `capture_started()`, `capture_completed(&AppHandle, bool)`, `report_error(&'static str, Option<&str>)`, `consented()` are named identically in T2 and T3. TS command `changeTelemetryEnabledSetting` matches the Rust `change_telemetry_enabled_setting`. Test id `telemetry-continue` matches T6 and T7.
- **Known unknowns called out in place:** exact `mode::active` return type, the `Err` type of `transcription_result`, `ToggleSwitch`'s role, and the Tauri commands the app invokes at boot under the stub. Each task says how to resolve its own.
