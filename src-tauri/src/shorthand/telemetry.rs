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

/// Creates the client. Returns `None` in debug builds, where nothing should
/// ever report. The guard must be held for the life of the process so the
/// transport flushes on exit; `run()` keeps it in a local.
pub fn init() -> Option<sentry::ClientInitGuard> {
    if cfg!(debug_assertions) {
        return None;
    }
    // `ClientOptions` is `#[non_exhaustive]`, so it is built through its
    // setter methods rather than struct-literal syntax; `dsn` has no setter
    // that returns `Option` on failure (`.dsn(&str)` panics), so it is
    // assigned directly to the public field instead — see `dsn_constant_parses`
    // for the test that would catch a typo in `DSN` before a release does.
    // `server_name` is left unset (defaults to `None`): never the hostname.
    // `auto_session_tracking` is off: `set_consent` is the sole owner of
    // `start_session`/`end_session`, so the SDK must not start one of its
    // own at `init` time, before consent has been read.
    let mut options = sentry::ClientOptions::new()
        .release(concat!("shorthand@", env!("CARGO_PKG_VERSION")))
        .environment("production")
        .send_default_pii(false)
        .auto_session_tracking(false)
        .session_mode(sentry::SessionMode::Application)
        .transport(GatedTransportFactory {
            gate: consent_flag(),
        });
    options.dsn = DSN.parse().ok();
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
/// to produce) a transcription result. Cheap and safe to call with consent
/// off: the SDK buffers metrics and the transport drops them.
pub fn capture_completed(app: &AppHandle, ok: bool) {
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
    let message = match detail {
        Some(detail) => format!("{kind}: {detail}"),
        None => kind.to_string(),
    };
    sentry::with_scope(
        |scope| scope.set_tag("error.kind", kind),
        || sentry::capture_message(&message, sentry::Level::Error),
    );
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
