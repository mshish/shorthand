//! Fork-only crash reports and usage counts, consent-gated. What is sent and
//! what never is are documented in TELEMETRY.md; this file is the only place
//! that talks to Sentry, so that document can be checked against one file.
//!
//! The gate is the transport: `GatedTransport` drops every envelope while
//! `CONSENT` is false, which covers events, sessions and metrics with one
//! check. The client exists from process start with the gate closed; `setup`
//! opens it from the stored setting; the Settings toggle flips it live.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};

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
        let inner =
            sentry::transports::DefaultTransportFactory.create_transport_with_options(options);
        Arc::new(GatedTransport {
            inner,
            gate: self.gate.clone(),
        })
    }
}

/// The options passed to `sentry::init()`. Extracted so a test can build a
/// client from them directly without touching the network — `sentry::init`
/// binds a global hub as a side effect, which a unit test must not do.
fn client_options() -> sentry::ClientOptions {
    // `ClientOptions` is `#[non_exhaustive]`, so it is built through its
    // setter methods rather than struct-literal syntax; `dsn` has no setter
    // that returns `Option` on failure (`.dsn(&str)` panics), so it is
    // assigned directly to the public field instead — see `dsn_constant_parses`
    // for the test that would catch a typo in `DSN` before a release does.
    // `server_name` is set to a constant, non-identifying placeholder rather
    // than left unset: `sentry_contexts::ContextIntegration::setup` fills
    // `server_name` from the machine hostname whenever it is `None`, and
    // that value lands on every event and as the `server.address` attribute
    // on every metric. Setting it here keeps `ContextIntegration` (which the
    // `contexts` feature still wants for OS/device/Rust context) from ever
    // seeing a `None` to fill in.
    // `auto_session_tracking` is off: `set_consent` is the sole owner of
    // `start_session`/`end_session`, so the SDK must not start one of its
    // own at `init` time, before consent has been read.
    let mut options = sentry::ClientOptions::new()
        .release(concat!("shorthand@", env!("CARGO_PKG_VERSION")))
        .environment("production")
        .send_default_pii(false)
        .server_name("shorthand")
        .auto_session_tracking(false)
        .session_mode(sentry::SessionMode::Application)
        .transport(GatedTransportFactory {
            gate: consent_flag(),
        });
    options.dsn = DSN.parse().ok();
    options
}

/// Creates the client. Returns `None` in debug builds, where nothing should
/// ever report. The guard must be held for the life of the process so the
/// transport flushes on exit; `run()` keeps it in a local.
pub fn init() -> Option<sentry::ClientInitGuard> {
    if cfg!(debug_assertions) {
        return None;
    }
    Some(sentry::init(client_options()))
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

/// Opens the gate: sets the Sentry scope user from `install_id` and starts a
/// session. Shared by `set_consent`'s enable branch and `apply_stored`.
fn open_gate(install_id: Option<String>) {
    sentry::configure_scope(|scope| {
        scope.set_user(install_id.map(|id| sentry::User {
            id: Some(id),
            ..Default::default()
        }));
    });
    consent_flag().store(true, Ordering::Release);
    sentry::start_session();
}

/// Closes the gate: ends and flushes the session before shutting the gate,
/// then clears the Sentry scope user. Shared by `set_consent`'s disable
/// branch and (eventually) any other path that must revoke consent.
fn close_gate() {
    // The session-end envelope only gets enqueued by `end_session`; the
    // periodic flusher can run as late as 60 seconds later, which may be
    // after the gate below has shut. So flush explicitly here, while
    // consent still holds and the gate is still open, then close it.
    sentry::end_session();
    if let Some(client) = sentry::Hub::current().client() {
        client.flush(Some(std::time::Duration::from_secs(2)));
    }
    consent_flag().store(false, Ordering::Release);
    sentry::configure_scope(|scope| scope.set_user(None));
}

/// Applies consent: opens or closes the gate, starts or ends the session,
/// and sets or clears the install id in both the Sentry scope and settings.
/// For an explicit choice only — the Settings toggle and the consent step.
/// Startup does not call this; it calls `apply_stored`, which never writes.
pub fn set_consent(app: &AppHandle, enabled: bool) {
    let mut stored = settings::get_settings(app);
    let install_id = next_install_id(stored.telemetry_install_id.clone(), enabled);
    if stored.telemetry_install_id != install_id || stored.telemetry_enabled != Some(enabled) {
        stored.telemetry_install_id = install_id.clone();
        stored.telemetry_enabled = Some(enabled);
        settings::write_settings(app, stored);
    }

    if enabled {
        open_gate(install_id);
    } else {
        close_gate();
    }
}

/// Whether the stored setting amounts to consent. Pure: only `Some(true)` is
/// consent — `None` (never asked) and `Some(false)` are both "do not open
/// the gate", so startup treats them alike without writing anything.
fn stored_consent(stored: Option<bool>) -> bool {
    stored == Some(true)
}

/// Applies the stored answer at startup. Never records one: for `None` (never
/// asked) or `Some(false)`, this does nothing and writes nothing, leaving a
/// fresh install's `None` exactly as it was so the consent step still sees
/// "never asked". Only for `Some(true)` does it open the gate — generating
/// and persisting an install id first if one is missing, since that id is
/// what the scope user and `start_session` need, and it is the one write
/// this function makes.
pub fn apply_stored(app: &AppHandle) {
    let stored = settings::get_settings(app);
    if !stored_consent(stored.telemetry_enabled) {
        return;
    }

    let install_id = match stored.telemetry_install_id.clone() {
        Some(id) => Some(id),
        None => {
            let id = next_install_id(None, true);
            let mut updated = stored;
            updated.telemetry_install_id = id.clone();
            settings::write_settings(app, updated);
            id
        }
    };

    open_gate(install_id);
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

/// Resolves `model_attribute` from a catalogue lookup that may have found
/// nothing. Fails closed: `None` (the model manager wasn't in `AppHandle`
/// state, or the selected id has no catalogue entry) is treated the same
/// as `is_custom = true`, so an id the catalogue doesn't recognise — which
/// is exactly the shape of a user-named custom model — never reaches
/// Sentry verbatim just because the lookup came back empty.
fn model_attribute_for(is_custom: Option<bool>, model_id: &str) -> String {
    model_attribute(model_id, is_custom.unwrap_or(true))
}

/// Marks the start of a capture. Called once per capture, next to
/// `mode::set_active`.
pub fn capture_started() {
    *CAPTURE_STARTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

/// Emits the two usage metrics for the capture that just produced (or failed
/// to produce) a transcription result. Safe to call with consent off: it
/// returns immediately, so nothing is ever handed to the SDK's own metrics
/// batcher before consent, and there is nothing sitting in it to flush the
/// moment the switch turns on.
pub fn capture_completed(app: &AppHandle, ok: bool) {
    if !consented() {
        return;
    }
    let mode = crate::shorthand::mode::active(app);
    let mode = match mode {
        crate::shorthand::mode::Mode::Meeting => "meeting",
        crate::shorthand::mode::Mode::Dictation => "dictation",
        crate::shorthand::mode::Mode::AssistedNotes => "assisted_notes",
    };
    let settings = settings::get_settings(app);
    let is_custom = app
        .try_state::<Arc<crate::managers::model::ModelManager>>()
        .and_then(|mm| mm.get_model_info(&settings.selected_model))
        .map(|info| info.is_custom);
    let model = model_attribute_for(is_custom, &settings.selected_model);

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
    // Nothing is handed to the SDK before consent, so nothing can sit in its
    // batcher when the switch turns on.
    if !consented() {
        return;
    }
    let message = match detail {
        Some(detail) => format!("{kind}: {detail}"),
        None => kind.to_string(),
    };
    sentry::with_scope(
        |scope| scope.set_tag("error.kind", kind),
        || sentry::capture_message(&message, sentry::Level::Error),
    );
}

/// Maps a transcription-engine error's `Display` text to a fixed reason
/// code. `actions.rs` passes this, never the raw message, to `report_error`:
/// the engine's text can vary and is not reviewed for what it might carry,
/// unlike the other two capture points' fixed `kind` tags.
pub fn transcription_reason(err: &str) -> &'static str {
    let err = err.to_lowercase();
    if err.contains("panicked") {
        "engine_panic"
    } else if err.contains("timed out") {
        "finalize_timeout"
    } else if err.contains("transcription failed") {
        "engine_error"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::Transport as _;
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
        assert_eq!(
            model_attribute("parakeet-tdt-0.6b-v3", false),
            "parakeet-tdt-0.6b-v3"
        );
        assert_eq!(model_attribute("my-private-finetune", true), "custom");
    }

    #[test]
    fn model_attribute_for_fails_closed_on_unresolved_lookup() {
        assert_eq!(
            model_attribute_for(None, "unrecognized-model-id"),
            "custom",
            "an absent model manager or unmatched catalogue entry must not leak the raw id"
        );
        assert_eq!(
            model_attribute_for(Some(false), "parakeet-tdt-0.6b-v3"),
            "parakeet-tdt-0.6b-v3"
        );
        assert_eq!(
            model_attribute_for(Some(true), "my-private-finetune"),
            "custom"
        );
    }

    #[test]
    fn dsn_constant_parses() {
        DSN.parse::<sentry::types::Dsn>()
            .expect("DSN constant must be a valid Sentry DSN");
    }

    /// C1: `sentry_contexts::ContextIntegration::setup` fills `server_name`
    /// from the machine hostname whenever it is still `None` when the
    /// client is built, and that value lands on every event and metric.
    /// C2: `debug-images` was dropped from the Cargo features list because
    /// it attaches every loaded module's absolute path — on a per-user
    /// Windows install, a path containing the account name — to every
    /// event.
    ///
    /// `sentry::apply_defaults` (what `sentry::init` calls before
    /// `Client::with_options`, and what a bare `Client::with_options` skips)
    /// is applied explicitly here so the default integrations Cargo
    /// features actually add — including `ContextIntegration::setup`, whose
    /// hostname-filling behaviour C1 fixes against — run exactly as they do
    /// in `init()`. Constructing a client this way does not send anything:
    /// the gate is closed and nothing is flushed.
    #[test]
    fn server_name_is_the_placeholder_not_the_hostname() {
        let client = sentry::Client::with_options(sentry::apply_defaults(client_options()));

        assert_eq!(
            client.options().server_name.as_deref(),
            Some("shorthand"),
            "server_name must be the constant placeholder, not the hostname \
             ContextIntegration would otherwise fill in"
        );

        // Remaining default integrations, given this crate's Cargo features
        // (backtrace, contexts, panic — debug-images removed): `contexts`
        // (`ContextIntegration`), `panic` (`PanicIntegration`),
        // `attach-stacktrace` and `process-stacktrace` (both from
        // `backtrace`). `debug-images` must not be among them.
        let integration_names: Vec<&str> = client
            .options()
            .integrations
            .iter()
            .map(|integration| integration.name())
            .collect();
        assert!(
            !integration_names.contains(&"debug-images"),
            "debug-images must stay off: it attaches loaded-module paths, \
             which on Windows contain the account name; got {integration_names:?}"
        );
    }

    #[test]
    fn transcription_reason_maps_known_text_and_falls_back_to_other() {
        assert_eq!(
            transcription_reason("thread panicked at engine.rs:42"),
            "engine_panic"
        );
        assert_eq!(
            transcription_reason("Timed out waiting 30s for live transcription to finalize"),
            "finalize_timeout",
            "must match the real message shape from managers/transcription.rs case-insensitively"
        );
        assert_eq!(
            transcription_reason("transcription failed: no audio"),
            "engine_error"
        );
        assert_eq!(transcription_reason("something else entirely"), "other");
    }

    #[test]
    fn stored_consent_is_true_only_for_explicit_opt_in() {
        assert!(
            !stored_consent(None),
            "never asked must not be treated as consent"
        );
        assert!(
            !stored_consent(Some(false)),
            "an explicit opt-out must not be treated as consent"
        );
        assert!(
            stored_consent(Some(true)),
            "an explicit opt-in must be treated as consent"
        );
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
