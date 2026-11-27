//! Fork-only, experimental: type the live transcript into the focused window.
//!
//! While a Meeting or Assisted notes capture runs, the `live_type` shortcut
//! opens a window during which text that becomes *committed* in the live
//! stream is typed into whatever app has focus. Text committed before the
//! window opened is never typed: the per-lane committed text is tracked from
//! the start of each stream, and only the suffix that arrives while the
//! window is open goes out. That suffix is `--follow-stream delta`'s own rule
//! ([`crate::follow_stream::committed_suffix`]), applied in-process.
//!
//! Choices worth knowing:
//!
//! - **Independent of follow-stream publication.** Text is taken where the
//!   stream emits it (`emit_stream_text`), not from the hub, so it works with
//!   a mode's publication toggle off and uses no follower slot.
//! - **Direct typing, always.** Delivery is `clipboard::type_text_direct`,
//!   never a clipboard paste, so the user's clipboard is untouched. The
//!   capture's `paste_method` is not consulted: it governs where a *finished*
//!   transcript goes, and this fork ships it as `None` for Meetings and
//!   Assisted notes, so honouring it would make the feature do nothing by
//!   default. Turning the feature on and pressing its shortcut is the request
//!   to type.
//! - **Held keys buffer.** While the shortcut is physically down, typed
//!   characters would arrive with its modifiers held and fire the focused
//!   app's own shortcuts (Ctrl+Alt+letter). So text committed during a hold is
//!   buffered and typed the moment the key is released. A tap-latched window
//!   types as text arrives.
//! - **Grace.** Decoding lags the audio, so for [`GRACE`] after the window
//!   closes, newly committed text is still typed.
//! - **Labels.** Meetings prefix a speaker label (`Me: `/`Them: `) whenever
//!   the speaker changes, with a newline between speakers, as
//!   `--follow-stream text` does. Assisted notes records only the microphone
//!   and types unlabelled.
//! - The text is the model's raw committed text, as followers receive it: the
//!   Chinese-script conversion and other final-text processing apply only to
//!   the finished transcript.

use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::StreamSource;
use crate::notices::{self, Notice, NoticeCode, NoticeSource};
use crate::settings::{self, AppSettings, ShortcutActivation};
use crate::shorthand::mode::{self, Mode};
use log::{debug, warn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// The shortcut's binding id.
pub const BINDING_ID: &str = "live_type";

/// How long text keeps being typed after the window closes.
pub const GRACE: Duration = Duration::from_millis(1000);

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct LiveTypeSettings {
    /// Also requires `AppSettings::experimental_enabled`; see
    /// [`binding_active`].
    pub enabled: bool,
    /// How the shortcut opens and closes the typing window, with the same
    /// three behaviours as the capture shortcuts.
    pub shortcut_activation: ShortcutActivation,
}

impl Default for LiveTypeSettings {
    fn default() -> Self {
        Self {
            // Experimental, and enabling registers a global shortcut.
            enabled: false,
            // Hold to type for a moment, tap to keep typing.
            shortcut_activation: ShortcutActivation::HoldOrToggle,
        }
    }
}

/// Whether the shortcut should be registered. The gate every registration
/// site checks, next to the other modes' own.
pub fn binding_active(settings: &AppSettings) -> bool {
    settings.experimental_enabled && settings.live_type.enabled
}

fn lane(source: StreamSource) -> usize {
    match source {
        StreamSource::Mic => 0,
        StreamSource::System => 1,
    }
}

// ---------------------------------------------------------------------------
// Shortcut activation (pure)

#[derive(Debug, PartialEq, Eq)]
enum Press {
    Activate,
    Deactivate,
    /// The window could have opened, but a precondition failed.
    Refused,
    /// Auto-repeat of a key already down.
    Ignored,
}

#[derive(Default)]
struct Activation {
    active: bool,
    key_down: bool,
    /// When the press that opened the window happened, for a hold to be told
    /// apart from a tap on release. `None` when a release cannot close it.
    opened_at: Option<Instant>,
}

impl Activation {
    fn press(&mut self, mode: ShortcutActivation, can_activate: bool, now: Instant) -> Press {
        if self.key_down {
            return Press::Ignored;
        }
        self.key_down = true;
        if self.active {
            self.active = false;
            self.opened_at = None;
            return Press::Deactivate;
        }
        if !can_activate {
            return Press::Refused;
        }
        self.active = true;
        self.opened_at = (mode != ShortcutActivation::Toggle).then_some(now);
        Press::Activate
    }

    /// Whether the release closed the window.
    fn release(
        &mut self,
        mode: ShortcutActivation,
        hold_threshold: Duration,
        now: Instant,
    ) -> bool {
        self.key_down = false;
        let Some(opened_at) = self.opened_at.take() else {
            return false;
        };
        let closes = match mode {
            ShortcutActivation::Toggle => false,
            ShortcutActivation::PushToTalk => true,
            ShortcutActivation::HoldOrToggle => now.duration_since(opened_at) >= hold_threshold,
        };
        if closes && self.active {
            self.active = false;
            return true;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// Committed-text tracking and rendering (pure)

#[derive(Default, Debug, PartialEq, Eq)]
enum Window {
    #[default]
    Closed,
    Open,
    Grace(Instant),
}

#[derive(Default)]
struct Typer {
    /// Each lane's committed text since its stream began: the baseline the
    /// next suffix is measured from.
    committed: [String; 2],
    window: Window,
    /// `Some` in a Meeting: each lane's speaker label.
    labels: Option<[String; 2]>,
    last_lane: Option<usize>,
}

impl Typer {
    fn stream_started(&mut self, lane: usize) {
        self.committed[lane].clear();
        self.window = Window::Closed;
    }

    fn open(&mut self, labels: Option<[String; 2]>) {
        self.window = Window::Open;
        self.labels = labels;
        self.last_lane = None;
    }

    fn close(&mut self, now: Instant) {
        if self.window == Window::Open {
            self.window = Window::Grace(now + GRACE);
        }
    }

    fn typing(&self, now: Instant) -> bool {
        match self.window {
            Window::Closed => false,
            Window::Open => true,
            Window::Grace(until) => now < until,
        }
    }

    /// Records a lane's committed text and returns what to type, if any.
    fn observe(&mut self, lane: usize, committed: &str, now: Instant) -> Option<String> {
        let previous = &mut self.committed[lane];
        let suffix = crate::follow_stream::committed_suffix(Some(previous), committed);
        if suffix.is_empty() {
            return None;
        }
        let suffix = suffix.to_string();
        if committed.len() == previous.len() + suffix.len() {
            previous.push_str(&suffix);
        } else {
            *previous = committed.to_string();
        }
        if !self.typing(now) {
            return None;
        }

        let mut out = String::new();
        let mut text = suffix.as_str();
        if let Some(labels) = &self.labels {
            if self.last_lane != Some(lane) {
                if self.last_lane.is_some() {
                    out.push('\n');
                }
                out.push_str(&labels[lane]);
                out.push_str(": ");
                text = text.trim_start();
            }
        }
        self.last_lane = Some(lane);
        out.push_str(text);
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// Activation + typing, glued (pure)

#[derive(Default)]
struct LiveType {
    activation: Activation,
    typer: Typer,
    /// Text committed while the key is physically down; see the module doc.
    held: String,
}

impl LiveType {
    fn observe(&mut self, lane: usize, committed: &str, now: Instant) -> Option<String> {
        let text = self.typer.observe(lane, committed, now)?;
        if self.activation.key_down {
            self.held.push_str(&text);
            return None;
        }
        Some(text)
    }

    fn press(
        &mut self,
        mode: ShortcutActivation,
        can_activate: bool,
        labels: Option<[String; 2]>,
        now: Instant,
    ) -> Press {
        let press = self.activation.press(mode, can_activate, now);
        match press {
            Press::Activate => self.typer.open(labels),
            Press::Deactivate => self.typer.close(now),
            Press::Refused | Press::Ignored => {}
        }
        press
    }

    /// Returns the buffered text to type now that the key is up.
    fn release(
        &mut self,
        mode: ShortcutActivation,
        hold_threshold: Duration,
        now: Instant,
    ) -> Option<String> {
        if self.activation.release(mode, hold_threshold, now) {
            self.typer.close(now);
        }
        (!self.held.is_empty()).then(|| std::mem::take(&mut self.held))
    }

    fn stream_started(&mut self, lane: usize) {
        self.typer.stream_started(lane);
        self.activation.active = false;
        self.activation.opened_at = None;
        self.held.clear();
    }
}

// ---------------------------------------------------------------------------
// App glue

static STATE: Lazy<Mutex<LiveType>> = Lazy::new(Default::default);

fn state() -> MutexGuard<'static, LiveType> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Called by the transcription manager whenever a lane's committed text
/// changes on a live stream.
pub fn observe_committed(app: &AppHandle, source: StreamSource, committed: &str) {
    let text = state().observe(lane(source), committed, Instant::now());
    if let Some(text) = text {
        deliver(app, text);
    }
}

/// Called when a lane's live stream starts: a new capture, so a new baseline
/// and a closed window.
pub fn on_stream_start(source: StreamSource) {
    state().stream_started(lane(source));
}

#[derive(Debug, PartialEq, Eq)]
enum Unavailable {
    /// No Meeting or Assisted notes capture is recording.
    NoCapture,
    /// The capture's model only transcribes once it ends.
    NeedsStreaming,
}

fn check_available(app: &AppHandle) -> Result<Mode, Unavailable> {
    let recording = app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|audio| audio.is_recording());
    let capture_mode = mode::active(app);
    if !recording || !matches!(capture_mode, Mode::Meeting | Mode::AssistedNotes) {
        return Err(Unavailable::NoCapture);
    }
    let model = crate::shorthand::dictation::resolve_settings(app).selected_model;
    let streaming = app
        .state::<Arc<ModelManager>>()
        .get_model_info(&model)
        .is_some_and(|info| info.supports_streaming);
    if !streaming {
        return Err(Unavailable::NeedsStreaming);
    }
    Ok(capture_mode)
}

fn speaker_labels(app: &AppHandle) -> [String; 2] {
    let strings = crate::tray_i18n::get_app_transcript_translations(app);
    [strings.live_type_me, strings.live_type_them]
}

/// The `live_type` shortcut's key events.
pub fn on_shortcut(app: &AppHandle, is_pressed: bool) {
    let settings = settings::get_settings(app);
    if !binding_active(&settings) {
        return;
    }
    let activation = settings.live_type.shortcut_activation;
    let now = Instant::now();

    if !is_pressed {
        let held = state().release(
            activation,
            Duration::from_millis(settings.hold_threshold_ms),
            now,
        );
        if let Some(text) = held {
            deliver(app, text);
        }
        return;
    }

    let available = check_available(app);
    let labels = match available {
        Ok(Mode::Meeting) => Some(speaker_labels(app)),
        _ => None,
    };
    let press = state().press(activation, available.is_ok(), labels, now);
    debug!("live type: {press:?} ({available:?})");
    match press {
        Press::Activate => notices::clear_matching(
            app,
            NoticeSource::Model,
            Some(NoticeCode::LiveTypeNeedsStreaming),
        ),
        Press::Refused if available == Err(Unavailable::NeedsStreaming) => {
            notices::raise_without_overlay(
                app,
                Notice::new(NoticeCode::LiveTypeNeedsStreaming, NoticeSource::Model),
            )
        }
        _ => {}
    }
}

/// Types on one worker thread, in commit order, so the stream's own thread
/// never waits on input injection.
fn deliver(app: &AppHandle, text: String) {
    static WORKER: OnceLock<Mutex<mpsc::Sender<String>>> = OnceLock::new();
    let sender = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<String>();
        let app = app.clone();
        std::thread::spawn(move || {
            while let Ok(mut text) = rx.recv() {
                // Whatever queued behind a slow injection goes out as one.
                while let Ok(more) = rx.try_recv() {
                    text.push_str(&more);
                }
                let settings = settings::get_settings(&app);
                if let Err(e) = crate::clipboard::type_text_direct(&text, &app, &settings) {
                    warn!("live type: typing failed: {e}");
                }
            }
        });
        Mutex::new(tx)
    });
    let sent = sender
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(text);
    if sent.is_err() {
        warn!("live type: typing worker is gone");
    }
}

// ---------------------------------------------------------------------------
// Settings

/// Registers or unregisters the shortcut when [`binding_active`] changes
/// between `before` and `after`.
fn apply_registration(app: &AppHandle, before: bool, after: &AppSettings) -> Result<(), String> {
    let now = binding_active(after);
    if before == now {
        return Ok(());
    }
    let Some(binding) = after.bindings.get(BINDING_ID).cloned() else {
        return Ok(());
    };
    if now {
        crate::shortcut::register_shortcut(app, binding)
    } else {
        crate::shortcut::unregister_shortcut(app, binding)
    }
}

/// Keeps the registration in step after `experimental_enabled` changed, which
/// is the other half of [`binding_active`].
pub fn sync_after_experimental_change(app: &AppHandle, before: &AppSettings) {
    let after = settings::get_settings(app);
    if let Err(e) = apply_registration(app, binding_active(before), &after) {
        warn!("live type: could not update the shortcut: {e}");
    }
    crate::secure_input::reconcile_fallback(app);
}

#[tauri::command]
#[specta::specta]
pub fn change_live_type_settings(
    app: AppHandle,
    live_type: LiveTypeSettings,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let before = binding_active(&settings);
    settings.live_type = live_type;
    // Registration first, as `change_assisted_notes_settings` does: a
    // shortcut another app owns must leave the switch off, not saved on with
    // nothing listening.
    apply_registration(&app, before, &settings)?;
    settings::write_settings(&app, settings);
    crate::secure_input::reconcile_fallback(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOLD: Duration = Duration::from_millis(300);

    fn labels() -> Option<[String; 2]> {
        Some(["Me".to_string(), "Them".to_string()])
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn text_committed_before_activation_is_never_typed() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        assert_eq!(live.observe(0, "earlier words ", t0), None);
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(
            live.observe(0, "earlier words and new ones", t0).as_deref(),
            Some("and new ones")
        );
    }

    #[test]
    fn meetings_label_each_speaker_change() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::Toggle, true, labels(), t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(live.observe(0, "Hello ", t0).as_deref(), Some("Me: Hello "));
        assert_eq!(
            live.observe(0, "Hello there.", t0).as_deref(),
            Some("there.")
        );
        assert_eq!(live.observe(1, " Hi.", t0).as_deref(), Some("\nThem: Hi."));
        assert_eq!(
            live.observe(0, "Hello there. Bye.", t0).as_deref(),
            Some("\nMe: Bye.")
        );
    }

    #[test]
    fn unlabelled_captures_type_the_bare_suffix() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(live.observe(0, " note", t0).as_deref(), Some(" note"));
    }

    #[test]
    fn typing_continues_for_the_grace_period_then_stops() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        // Second tap closes the window.
        live.press(ShortcutActivation::Toggle, true, None, at(t0, 100));
        live.release(ShortcutActivation::Toggle, HOLD, at(t0, 150));
        assert_eq!(
            live.observe(0, "late", at(t0, 900)).as_deref(),
            Some("late")
        );
        assert_eq!(live.observe(0, "late too", at(t0, 1200)), None);
    }

    #[test]
    fn a_hold_buffers_until_release_and_closes_the_window() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::HoldOrToggle, true, None, t0);
        assert_eq!(live.observe(0, "while held", at(t0, 200)), None);
        assert_eq!(
            live.release(ShortcutActivation::HoldOrToggle, HOLD, at(t0, 500))
                .as_deref(),
            Some("while held")
        );
        // A hold closes the window: only the grace period remains.
        assert_eq!(
            live.observe(0, "while held, tail", at(t0, 1000)).as_deref(),
            Some(", tail")
        );
        assert_eq!(live.observe(0, "while held, tail end", at(t0, 1600)), None);
    }

    #[test]
    fn a_quick_tap_latches_until_the_next_press() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::HoldOrToggle, true, None, t0);
        live.release(ShortcutActivation::HoldOrToggle, HOLD, at(t0, 100));
        assert_eq!(live.observe(0, "a", at(t0, 5000)).as_deref(), Some("a"));
        assert_eq!(
            live.press(ShortcutActivation::HoldOrToggle, true, None, at(t0, 6000)),
            Press::Deactivate
        );
    }

    #[test]
    fn push_to_talk_closes_on_every_release() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::PushToTalk, true, None, t0);
        live.release(ShortcutActivation::PushToTalk, HOLD, at(t0, 10));
        assert_eq!(live.observe(0, "x", at(t0, 2000)), None);
    }

    #[test]
    fn auto_repeat_and_refusals_change_nothing() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        assert_eq!(
            live.press(ShortcutActivation::Toggle, false, None, t0),
            Press::Refused
        );
        assert_eq!(
            live.press(ShortcutActivation::Toggle, true, None, t0),
            Press::Ignored
        );
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(live.observe(0, "nothing", t0), None);
    }

    #[test]
    fn a_new_stream_resets_the_baseline_and_closes_the_window() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        live.observe(0, "first capture", t0);
        live.stream_started(0);
        assert_eq!(live.observe(0, "first capture again", t0), None);
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(
            live.observe(0, "first capture again!", t0).as_deref(),
            Some("!")
        );
    }

    #[test]
    fn a_rewritten_prefix_types_the_replacement() {
        let t0 = Instant::now();
        let mut live = LiveType::default();
        live.observe(0, "hello", t0);
        live.press(ShortcutActivation::Toggle, true, None, t0);
        live.release(ShortcutActivation::Toggle, HOLD, t0);
        assert_eq!(live.observe(0, "hullo", t0).as_deref(), Some("hullo"));
        assert_eq!(live.observe(0, "hullo you", t0).as_deref(), Some(" you"));
    }
}
