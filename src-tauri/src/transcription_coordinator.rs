use crate::actions::ACTION_MAP;
use crate::managers::audio::AudioRecordingManager;
use crate::shorthand::capture_command::{self, CaptureState};
use crate::shorthand::mode::{self, Mode};
use log::{debug, error, warn};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

const DEBOUNCE: Duration = Duration::from_millis(30);
const RELEASE_GRACE: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PttAction {
    Passthrough,
    DeferRelease,
    CancelRelease,
}

struct PendingRelease {
    binding_id: String,
    hotkey_string: String,
    deadline: Instant,
}

/// A press that arrived while the pipeline was still busy processing the
/// previous transcription. Toggle-style triggers (SIGUSR2, CLI flags, some
/// pedal setups) flip state on every edge, so dropping a busy press desyncs
/// the parity: the next edge starts a recording nobody will ever stop.
struct PendingPress {
    binding_id: String,
    hotkey_string: String,
}

/// What to do with an input that arrives while the pipeline is busy
/// (`Stage::Processing`). `remembered` is whether a press for the same binding
/// is already waiting for the pipeline to drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusyAction {
    /// Ignore the input entirely.
    Ignore,
    /// Remember the press; start recording when the pipeline finishes.
    Remember,
    /// This input cancels a previously remembered press (toggle parity: two
    /// presses during one busy window net to no-op; PTT: the key was already
    /// released, so the remembered press must not fire).
    Forget,
}

fn classify_busy_input(is_pressed: bool, push_to_talk: bool, remembered: bool) -> BusyAction {
    match (push_to_talk, is_pressed) {
        // Toggle: presses alternate remember/forget to preserve parity.
        (false, true) if remembered => BusyAction::Forget,
        (false, true) => BusyAction::Remember,
        // Toggle mode ignores releases.
        (false, false) => BusyAction::Ignore,
        // PTT: a press while busy means the user is holding the key — start as
        // soon as the pipeline drains. A release while busy means the tap is
        // already over; forget the remembered press (or ignore if none).
        (true, true) => BusyAction::Remember,
        (true, false) if remembered => BusyAction::Forget,
        (true, false) => BusyAction::Ignore,
    }
}

/// Pipeline lifecycle.
///
/// `Recording`/`Processing` carry the whole capture context — not just
/// `binding_id` — so nothing downstream has to re-derive it. Before this,
/// "what is happening right now" was spread across this `Stage`, the
/// follow-stream hub's own active session, `AudioRecordingManager::is_recording`,
/// the `shorthand::mode` cell, and a `LAST_BEGUN_SESSION` atomic; each fix to
/// one of them re-derived state from the others, and each re-derivation
/// became its own race (see actions.rs's `FollowStreamSessionGuard` history
/// and this module's `capture_state_for_decision`). `Stage` is now the single
/// owner of `mode` and `publication_session` for the capture in flight, and
/// every other module receives them explicitly instead of re-reading a
/// separate source that a newer capture could already have overwritten.
///
/// `publication_session` is `Option<u64>` because a mode whose own
/// `follow_stream_enabled` is off records without `hub.begin()` ever being
/// called — `None` means "recording, but nothing was published", not
/// "unknown". It starts `None` in `Recording` (the id isn't known until
/// `hub.begin()` actually runs, inside the `Effect::Start` this produces) and
/// is filled in by `on_start_result` once that call reports what it
/// allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    Idle,
    Recording {
        binding_id: String,
        mode: Mode,
        publication_session: Option<u64>,
    },
    Processing {
        binding_id: String,
        mode: Mode,
        publication_session: Option<u64>,
    },
}

/// A keyboard/signal edge for a transcribe binding.
struct InputEvent {
    binding_id: String,
    hotkey_string: String,
    is_pressed: bool,
    push_to_talk: bool,
    /// External triggers (SIGUSR2, CLI flags) rather than physical keys.
    /// They fire on every edge by design and must never be debounced —
    /// dropping one desyncs toggle parity and wedges recording on.
    external: bool,
}

/// A side effect decided by [`CoordinatorState`]; the coordinator thread is
/// the only executor. Keeping decisions pure lets tests drive the exact
/// production transitions without a Tauri `AppHandle` or real timers.
#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Start {
        binding_id: String,
        hotkey_string: String,
    },
    Stop {
        binding_id: String,
        hotkey_string: String,
        /// Carried straight from `Stage`'s own copy (see its doc comment) so
        /// the executor can pass it into `ShortcutAction::stop` explicitly.
        publication_session: Option<u64>,
    },
}

/// Commands processed sequentially by the coordinator thread.
enum Command {
    Input(InputEvent),
    Cancel {
        recording_was_active: bool,
    },
    ProcessingFinished,
    /// An explicit `--start-assisted-notes` / `--stop-assisted-notes`
    /// command. Handled by [`decide_explicit_capture`] rather than folded
    /// into `Command::Input`/`on_input`: `on_input` carries the "remembered
    /// press" replay machinery (busy-pipeline remember/forget), which exists
    /// so a toggle's parity survives a busy window. An explicit command must
    /// never be replayed like that — a stale one firing after the caller has
    /// moved on would reintroduce the exact toggle-retry bug this command
    /// exists to close (see `shorthand::capture_command`'s module doc) — so
    /// it is decided once, directly, against whatever `Stage` is current
    /// when it is dequeued, and never queued for later.
    ExplicitCapture {
        op: capture_command::ExplicitOp,
        /// The binding to start (e.g. `"assisted_notes"`); also used to
        /// derive the command's `Mode` via `mode::mode_for_binding`. Stopping
        /// instead targets whatever binding `Stage::Recording` actually
        /// names, so the `_with_post_process` distinction is never lost (see
        /// `decide_explicit_capture`).
        binding_id: String,
        // No `mode_enabled` field here: it used to be resolved by the
        // sender before this command was placed on the channel, which meant
        // a settings change landing between enqueue and dequeue was decided
        // against a snapshot already stale by the time this command actually
        // ran. It is read fresh from Settings inside the coordinator loop
        // instead, at the same moment `Stage` itself is read, right before
        // calling `decide_explicit_capture`.
    },
}

fn classify_ptt_event(
    pending_release_binding: Option<&str>,
    is_pressed: bool,
    push_to_talk: bool,
    binding_id: &str,
    recording_binding: Option<&str>,
) -> PttAction {
    if !push_to_talk {
        return PttAction::Passthrough;
    }

    if is_pressed {
        if pending_release_binding == Some(binding_id) {
            PttAction::CancelRelease
        } else {
            PttAction::Passthrough
        }
    } else if recording_binding == Some(binding_id) && pending_release_binding.is_none() {
        PttAction::DeferRelease
    } else {
        PttAction::Passthrough
    }
}

/// Pure lifecycle state machine: owns every transition decision (PTT grace,
/// debounce, busy-pipeline remember/forget, cancel, drain). Produces
/// [`Effect`]s instead of touching the app, so unit tests exercise the real
/// production logic.
struct CoordinatorState {
    stage: Stage,
    last_press: Option<Instant>,
    pending_release: Option<PendingRelease>,
    pending_press: Option<PendingPress>,
}

impl CoordinatorState {
    fn new() -> Self {
        Self {
            stage: Stage::Idle,
            last_press: None,
            pending_release: None,
            pending_press: None,
        }
    }

    /// Deadline of the deferred release, if any — drives `recv_timeout`.
    fn grace_deadline(&self) -> Option<Instant> {
        self.pending_release.as_ref().map(|p| p.deadline)
    }

    fn on_input(&mut self, input: InputEvent, now: Instant) -> Option<Effect> {
        let pending_release_binding = self
            .pending_release
            .as_ref()
            .map(|pending| pending.binding_id.as_str());
        let recording_binding = match &self.stage {
            Stage::Recording { binding_id, .. } => Some(binding_id.as_str()),
            _ => None,
        };

        match classify_ptt_event(
            pending_release_binding,
            input.is_pressed,
            input.push_to_talk,
            &input.binding_id,
            recording_binding,
        ) {
            PttAction::CancelRelease => {
                self.pending_release = None;
                return None;
            }
            PttAction::DeferRelease => {
                self.pending_release = Some(PendingRelease {
                    binding_id: input.binding_id,
                    hotkey_string: input.hotkey_string,
                    deadline: now + RELEASE_GRACE,
                });
                return None;
            }
            PttAction::Passthrough => {}
        }

        // Debounce rapid-fire press events (key repeat / double-tap).
        // Push-to-talk releases may be deferred above to absorb X11 auto-repeat.
        // External triggers are exempt: each one is a deliberate edge from the
        // user's own integration, and dropping it desyncs toggle parity.
        if input.is_pressed && !input.external {
            if self
                .last_press
                .is_some_and(|t| now.duration_since(t) < DEBOUNCE)
            {
                debug!("Debounced press for '{}'", input.binding_id);
                return None;
            }
            self.last_press = Some(now);
        }

        // A busy pipeline can't accept lifecycle changes now: classify the
        // input against any already-remembered press instead of dropping it
        // silently.
        if let Stage::Processing { .. } = self.stage {
            // Only one press can be remembered. Once a binding has claimed it,
            // inputs for a different binding are ignored — the same rule as a
            // different binding pressed while recording — rather than silently
            // replacing the remembered press and breaking its parity.
            if let Some(pending) = &self.pending_press {
                if pending.binding_id != input.binding_id {
                    debug!(
                        "Ignoring input for '{}': '{}' is already pending",
                        input.binding_id, pending.binding_id
                    );
                    return None;
                }
            }
            let remembered = self.pending_press.is_some();
            match classify_busy_input(input.is_pressed, input.push_to_talk, remembered) {
                BusyAction::Remember => {
                    debug!(
                        "Remembering press for '{}': pipeline busy",
                        input.binding_id
                    );
                    self.pending_press = Some(PendingPress {
                        binding_id: input.binding_id,
                        hotkey_string: input.hotkey_string,
                    });
                }
                BusyAction::Forget => {
                    debug!("Forgetting remembered press for '{}'", input.binding_id);
                    self.pending_press = None;
                }
                BusyAction::Ignore => {
                    debug!("Ignoring input for '{}': pipeline busy", input.binding_id);
                }
            }
            return None;
        }

        if input.push_to_talk {
            if input.is_pressed {
                if matches!(self.stage, Stage::Idle) {
                    return Some(self.begin_recording(input.binding_id, input.hotkey_string));
                }
            } else if matches!(&self.stage, Stage::Recording { binding_id, .. } if binding_id == &input.binding_id)
            {
                return Some(self.begin_processing(input.binding_id, input.hotkey_string));
            }
        } else if input.is_pressed {
            match &self.stage {
                Stage::Idle => {
                    return Some(self.begin_recording(input.binding_id, input.hotkey_string));
                }
                Stage::Recording { binding_id, .. } if binding_id == &input.binding_id => {
                    return Some(self.begin_processing(input.binding_id, input.hotkey_string));
                }
                _ => debug!(
                    "Ignoring press for '{}': another binding is recording",
                    input.binding_id
                ),
            }
        }
        None
    }

    /// The `RELEASE_GRACE` window elapsed with no cancelling press arriving:
    /// fire the deferred release iff we are still recording that binding.
    fn on_grace_expired(&mut self) -> Option<Effect> {
        let pending = self.pending_release.take()?;
        if matches!(&self.stage, Stage::Recording { binding_id, .. } if binding_id == &pending.binding_id)
        {
            Some(self.begin_processing(pending.binding_id, pending.hotkey_string))
        } else {
            None
        }
    }

    fn on_cancel(&mut self, recording_was_active: bool) {
        self.pending_release = None;
        // An explicit cancel abandons any remembered start too — the user
        // asked for silence, not a deferred recording.
        self.pending_press = None;
        // Don't reset during processing — wait for the pipeline to finish.
        if !matches!(self.stage, Stage::Processing { .. })
            && (recording_was_active || matches!(self.stage, Stage::Recording { .. }))
        {
            self.stage = Stage::Idle;
        }
    }

    fn on_processing_finished(&mut self) -> Option<Effect> {
        self.stage = Stage::Idle;
        let pending = self.pending_press.take()?;
        debug!(
            "Pipeline drained; starting remembered press for '{}'",
            pending.binding_id
        );
        Some(self.begin_recording(pending.binding_id, pending.hotkey_string))
    }

    /// Reconcile the optimistic `Stage::Recording` after the executor reports
    /// whether recording actually began (microphone access can be denied, or
    /// a concurrent cancel can race this same start), and, when it did, fold
    /// in the `publication_session` `hub.begin()` allocated — `Stage` didn't
    /// know it yet when `begin_recording` made the optimistic transition,
    /// since `hub.begin()` runs inside the `Effect` this call reports on,
    /// not before it.
    ///
    /// Returns the session id the caller must terminate, if any.
    /// `action.start()` starts the recorder before calling `hub.begin()`
    /// (see actions.rs), so a cancel racing in between can stop the recorder
    /// and find no active hub session to end, and `hub.begin()` then
    /// publishes one anyway for a capture that is already dead. When that
    /// happens `started` is `false` but `publication_session` is `Some`, and
    /// without this return value that session would never learn it is over
    /// — left open until some unrelated later `begin()` happens to notice it
    /// as orphaned (see `FollowStreamHub::begin`), which can be an
    /// arbitrarily long wait and leaves a follower stuck on a `begin` with
    /// no terminal in the meantime. This method itself has no `AppHandle`
    /// and cannot call the hub directly, so it only reports which id, if
    /// any, the impure caller (`run_effect`) must end.
    fn on_start_result(
        &mut self,
        binding_id: &str,
        started: bool,
        publication_session: Option<u64>,
    ) -> Option<u64> {
        let Stage::Recording {
            binding_id: recording_id,
            publication_session: session,
            ..
        } = &mut self.stage
        else {
            return None;
        };
        if recording_id != binding_id {
            return None;
        }
        if started {
            *session = publication_session;
            None
        } else {
            self.stage = Stage::Idle;
            publication_session
        }
    }

    /// Optimistic transition to `Recording`; rolled back via
    /// [`CoordinatorState::on_start_result`] if the effect fails to start
    /// recording for real.
    fn begin_recording(&mut self, binding_id: String, hotkey_string: String) -> Effect {
        let mode = mode::mode_for_binding(&binding_id);
        self.stage = Stage::Recording {
            binding_id: binding_id.clone(),
            mode,
            // Not known yet -- see `on_start_result`.
            publication_session: None,
        };
        Effect::Start {
            binding_id,
            hotkey_string,
        }
    }

    fn begin_processing(&mut self, binding_id: String, hotkey_string: String) -> Effect {
        // Every caller only reaches here while `self.stage` is `Recording`
        // for this exact binding (see each call site's own guard above), so
        // its `mode`/`publication_session` are simply carried forward rather
        // than re-derived. The fallback branch is defensive only: it should
        // be unreachable given those guards.
        let (mode, publication_session) = match &self.stage {
            Stage::Recording {
                mode,
                publication_session,
                ..
            } => (*mode, *publication_session),
            _ => (mode::mode_for_binding(&binding_id), None),
        };
        self.stage = Stage::Processing {
            binding_id: binding_id.clone(),
            mode,
            publication_session,
        };
        Effect::Stop {
            binding_id,
            hotkey_string,
            publication_session,
        }
    }
}

/// Classifies `stage` into the `CaptureState` [`capture_command::decide`]
/// needs. `processing_mode` (the caller reads it via `mode::active`) is
/// this function's only external input, kept as a parameter rather than an
/// `AppHandle` read so classification stays pure and testable like the rest
/// of this module.
///
/// `Stage::Processing` maps to its own `CaptureState::Processing(processing_mode)`
/// — never to `Idle`, and never folded into `Recording` the way an earlier
/// version of this classifier did. Not `Idle`: even though the recording it
/// followed has already stopped by the time any command reaches this
/// classifier (`AudioRecordingManager::stop_recording` runs to completion
/// synchronously inside `Effect::Stop`, handled on this same single thread
/// before the next command is ever read, so the mic is provably closed
/// already), `Stage::Processing` itself hasn't cleared — `on_processing_finished`
/// resets `stage` to `Idle` unconditionally once the pipeline drains, and
/// mapping this window to `Idle` here would let an explicit `Start` forward
/// immediately and land a second `Stage::Recording` on top of it, which that
/// unconditional reset would then silently stomp back to `Idle` out from
/// under a capture that is genuinely running. Not folded into `Recording`:
/// `decide` treats `Recording(mode)` as `NoOp` for a same-mode `Start`, which
/// is correct there because that capture really is still running — but the
/// mic behind `Processing` is already closed, so a `NoOp` there would report
/// success while starting nothing, exactly the silent-command bug this
/// command exists to close. Its own `Processing` variant lets `decide` refuse
/// every `Start` observed here as `Busy`, same mode or not, so the decision
/// is always an observable record rather than a no-op that can never be told
/// apart from "nothing needed to happen".
fn capture_state_for_decision(stage: &Stage, processing_mode: Mode) -> CaptureState {
    match stage {
        Stage::Idle => CaptureState::Idle,
        // `Stage` now carries its own `mode`, computed once by
        // `begin_recording`/`begin_processing` from the same
        // `mode::mode_for_binding` this used to call again here — reading it
        // back is the same value, not a behaviour change.
        Stage::Recording { mode, .. } => CaptureState::Recording(*mode),
        Stage::Processing { .. } => CaptureState::Processing(processing_mode),
    }
}

/// What handling an explicit capture command resolved to: an [`Effect`] to
/// execute, or an outcome with no effect at all.
#[derive(Debug, PartialEq, Eq)]
enum ExplicitOutcome {
    Effect(Effect),
    NoOp,
    Refuse(capture_command::RefusalReason),
}

/// Decides — and, for `Forward`, performs — the `CoordinatorState` transition
/// for an explicit start/stop command. Mutates `state.stage` only when it
/// returns `ExplicitOutcome::Effect`, exactly like `CoordinatorState::on_input`
/// does for a toggle press, but by calling `begin_recording`/`begin_processing`
/// directly instead of going through `on_input`'s PTT/debounce/busy-remember
/// branches — see the `Command::ExplicitCapture` doc for why those must be
/// bypassed here.
///
/// Reuses the pure `capture_command::decide` for the actual policy, fed by
/// `capture_state_for_decision` above instead of an external snapshot. Since
/// both the classification and the execution happen inside the single
/// command dequeued from the coordinator's channel, a second explicit command
/// sent immediately after the first is guaranteed to observe the first one's
/// already-applied `Stage` transition — closing the decide-then-toggle race
/// where two retries could each observe `Idle` and one would start what the
/// other then stopped.
fn decide_explicit_capture(
    state: &mut CoordinatorState,
    op: capture_command::ExplicitOp,
    binding_id: &str,
    mode_enabled: bool,
    processing_mode: Mode,
) -> ExplicitOutcome {
    let mode = mode::mode_for_binding(binding_id);
    let capture = capture_state_for_decision(&state.stage, processing_mode);
    match capture_command::decide(op, mode, mode_enabled, capture) {
        capture_command::Decision::Forward => match op {
            capture_command::ExplicitOp::Start => ExplicitOutcome::Effect(
                state.begin_recording(binding_id.to_string(), "CLI".to_string()),
            ),
            // Stop must end whatever binding is actually recording, not the
            // canonical one passed in: the `_with_post_process` variant
            // differs in whether the finished transcript gets post-processed,
            // and using the wrong binding here would silently drop that step.
            capture_command::ExplicitOp::Stop => match &state.stage {
                Stage::Recording {
                    binding_id: active_id,
                    mode: active_mode,
                    ..
                } if *active_mode == mode => {
                    let active_id = active_id.clone();
                    ExplicitOutcome::Effect(state.begin_processing(active_id, "CLI".to_string()))
                }
                // `decide` said Forward off the conservative Processing
                // mapping above, but there is no live binding left for a
                // second stop to act on — the first one already ran. Nothing
                // to do; the in-flight pipeline finishes on its own.
                _ => ExplicitOutcome::NoOp,
            },
        },
        capture_command::Decision::NoOp => ExplicitOutcome::NoOp,
        capture_command::Decision::Refuse(reason) => ExplicitOutcome::Refuse(reason),
    }
}

/// Serialises all transcription lifecycle events through a single thread
/// to eliminate race conditions between keyboard shortcuts, signals, and
/// the async transcribe-paste pipeline. The thread is a thin shell: it
/// transports commands to the pure [`CoordinatorState`] and executes the
/// returned [`Effect`]s.
pub struct TranscriptionCoordinator {
    tx: Sender<Command>,
}

pub fn is_transcribe_binding(id: &str) -> bool {
    matches!(
        id,
        "transcribe"
            | "transcribe_with_post_process"
            | "dictate"
            | "dictate_with_post_process"
            | "assisted_notes"
            | "assisted_notes_with_post_process"
    )
}

impl TranscriptionCoordinator {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut state = CoordinatorState::new();

                loop {
                    let cmd = if let Some(deadline) = state.grace_deadline() {
                        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                            Ok(cmd) => cmd,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if let Some(effect) = state.on_grace_expired() {
                                    run_effect(&app, &mut state, effect);
                                }
                                continue;
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    } else {
                        match rx.recv() {
                            Ok(cmd) => cmd,
                            Err(_) => break,
                        }
                    };

                    match cmd {
                        Command::Input(input) => {
                            if let Some(effect) = state.on_input(input, Instant::now()) {
                                run_effect(&app, &mut state, effect);
                            }
                        }
                        Command::Cancel {
                            recording_was_active,
                        } => state.on_cancel(recording_was_active),
                        Command::ProcessingFinished => {
                            if let Some(effect) = state.on_processing_finished() {
                                run_effect(&app, &mut state, effect);
                            }
                        }
                        Command::ExplicitCapture { op, binding_id } => {
                            // Resolved here, at dequeue time, rather than by
                            // the sender before this command was placed on
                            // the channel — see `Command::ExplicitCapture`'s
                            // doc comment for the stale-snapshot window that
                            // would otherwise open between enqueue and
                            // dequeue.
                            let mode_enabled =
                                crate::settings::get_settings(&app).assisted_notes.enabled;
                            let outcome = decide_explicit_capture(
                                &mut state,
                                op,
                                &binding_id,
                                mode_enabled,
                                mode::active(&app),
                            );
                            run_explicit_outcome(&app, &mut state, op, &binding_id, outcome);
                        }
                    }
                }
                debug!("Transcription coordinator exited");
            }));
            if let Err(e) = result {
                error!("Transcription coordinator panicked: {e:?}");
            }
        });

        Self { tx }
    }

    /// Send a keyboard/signal input event for a transcribe binding.
    /// For signal-based toggles, use `is_pressed: true` and `push_to_talk: false`.
    pub fn send_input(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
    ) {
        self.send(binding_id, hotkey_string, is_pressed, push_to_talk, false);
    }

    /// Send an external trigger (SIGUSR2, CLI flag). Always a toggle press,
    /// always exempt from debounce — see [`InputEvent::external`].
    pub fn send_external_input(&self, binding_id: &str, source: &str) {
        self.send(binding_id, source, true, false, true);
    }

    fn send(
        &self,
        binding_id: &str,
        hotkey_string: &str,
        is_pressed: bool,
        push_to_talk: bool,
        external: bool,
    ) {
        if self
            .tx
            .send(Command::Input(InputEvent {
                binding_id: binding_id.to_string(),
                hotkey_string: hotkey_string.to_string(),
                is_pressed,
                push_to_talk,
                external,
            }))
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_cancel(&self, recording_was_active: bool) {
        if self
            .tx
            .send(Command::Cancel {
                recording_was_active,
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }

    pub fn notify_processing_finished(&self) {
        if self.tx.send(Command::ProcessingFinished).is_err() {
            warn!("Transcription coordinator channel closed");
        }
    }

    /// Send an explicit `--start-assisted-notes` / `--stop-assisted-notes`
    /// command. `binding_id` is the canonical binding to start (e.g.
    /// `"assisted_notes"`); a stop instead targets whatever binding is
    /// actually recording, decided inside the coordinator loop where
    /// `Stage` is authoritative — see [`Command::ExplicitCapture`]. Whether
    /// the mode is enabled is likewise resolved inside that loop, not here,
    /// so it can't go stale between this call and the command being
    /// dequeued.
    pub fn send_explicit_capture(&self, op: capture_command::ExplicitOp, binding_id: &str) {
        if self
            .tx
            .send(Command::ExplicitCapture {
                op,
                binding_id: binding_id.to_string(),
            })
            .is_err()
        {
            warn!("Transcription coordinator channel closed");
        }
    }
}

fn run_effect(app: &AppHandle, state: &mut CoordinatorState, effect: Effect) {
    match effect {
        Effect::Start {
            binding_id,
            hotkey_string,
        } => {
            let (started, publication_session) = start(app, &binding_id, &hotkey_string);
            if let Some(orphaned_session) =
                state.on_start_result(&binding_id, started, publication_session)
            {
                // A cancel raced this start between the recorder stopping
                // and `hub.begin()` allocating a session for it (see
                // `on_start_result`'s doc comment); terminate that exact
                // session by id rather than "whatever the hub considers
                // active now" so a follower is never left with a `begin`
                // that never receives a terminal. `hub.cancel` is itself
                // scoped to this id (see `FollowStreamHub::finish_with`), so
                // this is a harmless no-op if some other path already closed
                // or superseded it.
                if let Some(hub) = crate::follow_stream::hub(app) {
                    hub.cancel(orphaned_session);
                }
            }
        }
        Effect::Stop {
            binding_id,
            hotkey_string,
            publication_session,
        } => stop(app, &binding_id, &hotkey_string, publication_session),
    }
}

/// Executes what [`decide_explicit_capture`] decided: runs its effect (if
/// any), or reports the no-op/refusal it settled on. Split from that
/// function so the decision itself stays pure and unit-testable without an
/// `AppHandle`, matching `on_input`/`run_effect`'s own split.
fn run_explicit_outcome(
    app: &AppHandle,
    state: &mut CoordinatorState,
    op: capture_command::ExplicitOp,
    binding_id: &str,
    outcome: ExplicitOutcome,
) {
    match outcome {
        ExplicitOutcome::Effect(effect) => run_effect(app, state, effect),
        ExplicitOutcome::NoOp => {
            debug!("{op:?} '{binding_id}': already in the requested state");
        }
        ExplicitOutcome::Refuse(reason) => {
            warn!("{op:?} '{binding_id}' refused: {reason:?}");
            let mode = mode::mode_for_binding(binding_id);
            if let Some(hub) = crate::follow_stream::hub(app) {
                hub.refused(mode.into(), reason.into());
            }
            // Same courtesy `--toggle-assisted-notes` already gives: raise the
            // app so the user can see the setting that refused it. A `Busy`
            // refusal gets none — the fix is not in Settings, and raising the
            // window would interrupt whatever capture is already running.
            if matches!(reason, capture_command::RefusalReason::ModeDisabled) {
                crate::show_main_window(app);
            }
        }
    }
}

/// Execute a start effect; returns whether recording actually began, so the
/// state machine can roll back its optimistic transition on failure, along
/// with whatever follow-stream session `action.start` allocated (`None` if
/// its mode's publication setting is off, or nothing began at all) — see
/// `on_start_result` for how the latter reaches `Stage`.
fn start(app: &AppHandle, binding_id: &str, hotkey_string: &str) -> (bool, Option<u64>) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return (false, None);
    };
    let publication_session = action.start(app, binding_id, hotkey_string);
    let recording = app
        .try_state::<Arc<AudioRecordingManager>>()
        .is_some_and(|a| a.is_recording());
    if !recording {
        debug!("Start for '{binding_id}' did not begin recording; staying idle");
    }
    (recording, publication_session)
}

fn stop(app: &AppHandle, binding_id: &str, hotkey_string: &str, publication_session: Option<u64>) {
    let Some(action) = ACTION_MAP.get(binding_id) else {
        warn!("No action in ACTION_MAP for '{binding_id}'");
        return;
    };
    action.stop(app, binding_id, hotkey_string, publication_session);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the `Stage::Recording` these tests expect right after a plain
    /// `begin_recording` -- `mode` derived the same way production does, and
    /// `publication_session` unknown (`None`) because none of these tests
    /// take it through `on_start_result`.
    fn recording_stage(binding_id: &str) -> Stage {
        Stage::Recording {
            binding_id: binding_id.to_string(),
            mode: mode::mode_for_binding(binding_id),
            publication_session: None,
        }
    }

    /// See `recording_stage`; the `Stage::Processing` counterpart these tests
    /// reach via `begin_processing` from a `Recording` stage that itself
    /// never had a session recorded onto it.
    fn processing_stage(binding_id: &str) -> Stage {
        Stage::Processing {
            binding_id: binding_id.to_string(),
            mode: mode::mode_for_binding(binding_id),
            publication_session: None,
        }
    }

    #[test]
    fn is_transcribe_binding_recognises_meeting_and_dictation_bindings() {
        assert!(is_transcribe_binding("transcribe"));
        assert!(is_transcribe_binding("transcribe_with_post_process"));
        assert!(is_transcribe_binding("dictate"));
        assert!(is_transcribe_binding("dictate_with_post_process"));
        assert!(is_transcribe_binding("assisted_notes"));
        assert!(is_transcribe_binding("assisted_notes_with_post_process"));
        assert!(!is_transcribe_binding("cancel"));
        assert!(!is_transcribe_binding("test"));
    }

    #[test]
    fn push_to_talk_release_while_recording_defers_release() {
        assert_eq!(
            classify_ptt_event(None, false, true, "transcribe", Some("transcribe")),
            PttAction::DeferRelease
        );
    }

    #[test]
    fn push_to_talk_press_matching_pending_release_cancels_release() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                true,
                "transcribe",
                Some("transcribe")
            ),
            PttAction::CancelRelease
        );
    }

    #[test]
    fn toggle_mode_press_and_release_pass_through() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                false,
                "transcribe",
                Some("transcribe")
            ),
            PttAction::Passthrough
        );
        assert_eq!(
            classify_ptt_event(None, false, false, "transcribe", Some("transcribe")),
            PttAction::Passthrough
        );
    }

    #[test]
    fn press_for_different_binding_than_pending_release_passes_through() {
        assert_eq!(
            classify_ptt_event(
                Some("transcribe"),
                true,
                true,
                "transcribe_with_post_process",
                Some("transcribe")
            ),
            PttAction::Passthrough
        );
    }

    #[test]
    fn press_matching_pending_release_cancels_without_recording_state() {
        assert_eq!(
            classify_ptt_event(Some("transcribe"), true, true, "transcribe", None),
            PttAction::CancelRelease
        );
    }

    // ---------------------------------------------------------------------
    // Busy-pipeline input classification.
    //
    // Toggle-style triggers (SIGUSR2, CLI flags, pedals that signal on both
    // edges) flip state on every edge. Dropping a press that arrives while
    // the previous pipeline is still processing desyncs the parity: the next
    // edge then starts a recording no one will stop, leaving the overlay
    // waiting for input with the button long released.
    // ---------------------------------------------------------------------

    #[test]
    fn toggle_press_during_processing_remembers_start() {
        assert_eq!(
            classify_busy_input(true, false, false),
            BusyAction::Remember
        );
    }

    #[test]
    fn second_toggle_press_during_processing_forgets_press() {
        assert_eq!(classify_busy_input(true, false, true), BusyAction::Forget);
    }

    #[test]
    fn toggle_release_during_processing_is_ignored() {
        assert_eq!(classify_busy_input(false, false, false), BusyAction::Ignore);
        assert_eq!(classify_busy_input(false, false, true), BusyAction::Ignore);
    }

    #[test]
    fn ptt_press_during_processing_remembers_start() {
        assert_eq!(classify_busy_input(true, true, false), BusyAction::Remember);
    }

    #[test]
    fn ptt_release_during_processing_forgets_remembered_press() {
        assert_eq!(classify_busy_input(false, true, true), BusyAction::Forget);
        assert_eq!(classify_busy_input(false, true, false), BusyAction::Ignore);
    }

    /// Toggle parity across a busy window: an odd number of presses remembers
    /// one start, each further press flips the remembered press off/on again.
    #[test]
    fn toggle_presses_alternate_remember_and_forget_while_busy() {
        let mut remembered = false;
        for expected in [
            BusyAction::Remember,
            BusyAction::Forget,
            BusyAction::Remember,
        ] {
            let action = classify_busy_input(true, false, remembered);
            assert_eq!(action, expected);
            remembered = action == BusyAction::Remember;
        }
        assert!(remembered);
    }

    /// A quick PTT tap that lands entirely inside the busy window must net to
    /// no-op: the press is remembered, the release forgets it, nothing starts.
    #[test]
    fn ptt_tap_inside_busy_window_nets_noop() {
        assert_eq!(classify_busy_input(true, true, false), BusyAction::Remember);
        assert_eq!(classify_busy_input(false, true, true), BusyAction::Forget);
    }

    // ---------------------------------------------------------------------
    // Sequence-level regression coverage for issue #1539.
    //
    // Under X11 key auto-repeat, holding a push-to-talk key does not emit one
    // long press. It emits the initial press followed by a stream of
    // synthesized release/press pairs, then a single genuine release on key-up.
    // Before the fix, every synthesized release passed straight through and
    // stopped recording, so holding the key "rapidly toggled" recording on and
    // off. The fix defers each release for a short grace window and cancels it
    // when the matching auto-repeat press arrives.
    //
    // The unit tests above assert the classifiers in isolation. The harness
    // below drives the real `CoordinatorState` through whole event sequences
    // — the same `on_input` / `on_grace_expired` handlers the coordinator
    // thread runs — so a burst can be exercised deterministically without a
    // Tauri AppHandle or real timers, and the tests can never drift from the
    // production transitions.
    // ---------------------------------------------------------------------

    const BINDING: &str = "transcribe";

    #[derive(Clone, Copy)]
    enum Ev {
        /// A key-down event (real initial press or a synthesized auto-repeat press).
        Press,
        /// A key-up event (synthesized auto-repeat release or the genuine key-up).
        Release,
        /// The `RELEASE_GRACE` window elapsed with no cancelling press arriving.
        Grace,
    }

    struct DriveResult {
        starts: u32,
        stops: u32,
        stage: Stage,
    }

    fn ptt_input(is_pressed: bool) -> InputEvent {
        InputEvent {
            binding_id: BINDING.to_string(),
            hotkey_string: BINDING.to_string(),
            is_pressed,
            push_to_talk: true,
            external: false,
        }
    }

    /// Feeds an event sequence to a real [`CoordinatorState`] the way the
    /// coordinator thread would; effects are counted instead of executed.
    fn drive(events: &[Ev]) -> DriveResult {
        let mut state = CoordinatorState::new();
        let mut clock = Instant::now();
        let mut starts = 0u32;
        let mut stops = 0u32;

        for ev in events {
            // Auto-repeat events arrive a few ms apart, well inside DEBOUNCE.
            clock += Duration::from_millis(5);

            let effect = match ev {
                Ev::Grace => state.on_grace_expired(),
                Ev::Press | Ev::Release => {
                    state.on_input(ptt_input(matches!(ev, Ev::Press)), clock)
                }
            };
            match effect {
                Some(Effect::Start { .. }) => starts += 1,
                Some(Effect::Stop { .. }) => stops += 1,
                None => {}
            }
        }

        DriveResult {
            starts,
            stops,
            stage: state.stage,
        }
    }

    /// Initial press plus several synthesized release/press pairs, as X11 emits
    /// while a push-to-talk key is held down.
    fn autorepeat_burst() -> Vec<Ev> {
        let mut events = vec![Ev::Press];
        for _ in 0..6 {
            events.push(Ev::Release);
            events.push(Ev::Press);
        }
        events
    }

    /// Regression for #1539: a burst of X11 auto-repeat release/press pairs must
    /// not stop recording. Before the fix the first synthesized release stopped
    /// recording immediately (stops == 1, stage left Recording), which produced
    /// the rapid on/off toggling. With the fix the releases are coalesced and
    /// recording stays continuously active for the whole burst.
    #[test]
    fn x11_autorepeat_burst_does_not_toggle_recording() {
        let result = drive(&autorepeat_burst());
        assert_eq!(result.starts, 1, "recording should start exactly once");
        assert_eq!(
            result.stops, 0,
            "synthesized auto-repeat releases must not stop recording mid-burst"
        );
        assert_eq!(
            result.stage,
            recording_stage(BINDING),
            "recording must remain active across the entire auto-repeat burst"
        );
    }

    /// Complements the burst test: once the key is genuinely released and the
    /// grace window elapses with no re-press, recording stops exactly once. This
    /// proves the debounce only coalesces synthesized releases and does not wedge
    /// the coordinator or swallow the real key-up.
    #[test]
    fn genuine_release_after_grace_stops_recording_once() {
        let mut events = autorepeat_burst();
        events.push(Ev::Release); // genuine key-up
        events.push(Ev::Grace); // grace window elapses, no cancelling press
        let result = drive(&events);
        assert_eq!(result.starts, 1, "recording should start exactly once");
        assert_eq!(
            result.stops, 1,
            "a genuine release should stop recording exactly once"
        );
        assert_eq!(result.stage, processing_stage(BINDING));
    }

    // ---------------------------------------------------------------------
    // Sequence-level coverage of the busy-pipeline and cancel paths, driven
    // through the real machine.
    // ---------------------------------------------------------------------

    /// PTT press while the pipeline is busy is remembered and starts recording
    /// once the pipeline drains.
    #[test]
    fn press_during_processing_starts_after_drain() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        let effect = state.on_input(ptt_input(true), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));

        let effect = state.on_input(ptt_input(false), now + Duration::from_millis(100));
        assert!(effect.is_none(), "release should be deferred, not fired");

        let effect = state.on_grace_expired();
        assert!(matches!(effect, Some(Effect::Stop { .. })));

        let effect = state.on_input(ptt_input(true), now + Duration::from_millis(200));
        assert!(effect.is_none(), "busy pipeline must remember, not start");

        let effect = state.on_processing_finished();
        assert!(
            matches!(effect, Some(Effect::Start { .. })),
            "remembered press should start once the pipeline drains"
        );
    }

    /// Two toggle presses inside one busy window net to no-op: nothing starts
    /// when the pipeline drains (toggle parity).
    #[test]
    fn toggle_presses_during_processing_net_noop_after_drain() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        let effect = state.on_input(ptt_input(true), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));
        let effect = state.on_input(ptt_input(false), now + Duration::from_millis(100));
        assert!(effect.is_none());
        let effect = state.on_grace_expired();
        assert!(matches!(effect, Some(Effect::Stop { .. })));

        let toggle = |state: &mut CoordinatorState, at: Instant| {
            state.on_input(
                InputEvent {
                    binding_id: BINDING.to_string(),
                    hotkey_string: BINDING.to_string(),
                    is_pressed: true,
                    push_to_talk: false,
                    external: true,
                },
                at,
            )
        };

        let effect = toggle(&mut state, now + Duration::from_millis(200));
        assert!(effect.is_none());
        let effect = toggle(&mut state, now + Duration::from_millis(300));
        assert!(effect.is_none());

        let effect = state.on_processing_finished();
        assert!(
            effect.is_none(),
            "even number of busy toggle presses must not start recording"
        );
        assert_eq!(state.stage, Stage::Idle);
    }

    /// Cancel while processing abandons a remembered press: the pipeline drains
    /// to idle and nothing starts.
    #[test]
    fn cancel_during_processing_drops_remembered_press() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        let effect = state.on_input(ptt_input(true), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));
        let effect = state.on_input(ptt_input(false), now + Duration::from_millis(100));
        assert!(effect.is_none());
        let effect = state.on_grace_expired();
        assert!(matches!(effect, Some(Effect::Stop { .. })));

        let effect = state.on_input(ptt_input(true), now + Duration::from_millis(200));
        assert!(effect.is_none());

        state.on_cancel(false);
        assert_eq!(
            state.stage,
            processing_stage(BINDING),
            "cancel must not reset mid-processing — the pipeline still finishes"
        );

        let effect = state.on_processing_finished();
        assert!(
            effect.is_none(),
            "cancelled session must not spawn a deferred recording"
        );
        assert_eq!(state.stage, Stage::Idle);
    }

    fn toggle_input(external: bool) -> InputEvent {
        toggle_input_for(BINDING, external)
    }

    fn toggle_input_for(binding_id: &str, external: bool) -> InputEvent {
        InputEvent {
            binding_id: binding_id.to_string(),
            hotkey_string: binding_id.to_string(),
            is_pressed: true,
            push_to_talk: false,
            external,
        }
    }

    /// Start and stop one toggle recording so the machine sits in `Processing`.
    fn drive_into_processing(state: &mut CoordinatorState, now: Instant) {
        let effect = state.on_input(toggle_input(true), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));
        let effect = state.on_input(toggle_input(true), now + Duration::from_millis(100));
        assert!(matches!(effect, Some(Effect::Stop { .. })));
        assert_eq!(state.stage, processing_stage(BINDING));
    }

    const OTHER_BINDING: &str = "transcribe_with_post_process";

    /// Only one press can be pending. Once a binding has claimed it, a toggle
    /// for a different binding is ignored (as it is while recording) instead of
    /// replacing the remembered press, so the pending binding's parity holds:
    /// two transcribe toggles still net to no-op.
    #[test]
    fn different_binding_does_not_replace_pending_press() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();
        drive_into_processing(&mut state, now);

        let at = |ms| now + Duration::from_millis(ms);
        assert!(state.on_input(toggle_input(true), at(200)).is_none());
        assert!(state
            .on_input(toggle_input_for(OTHER_BINDING, true), at(300))
            .is_none());
        assert!(state.on_input(toggle_input(true), at(400)).is_none());

        let effect = state.on_processing_finished();
        assert!(
            effect.is_none(),
            "two transcribe toggles net to no-op; the ignored post-process toggle must not start"
        );
        assert_eq!(state.stage, Stage::Idle);
    }

    /// The binding that claimed the pending press is the one that starts on
    /// drain, regardless of other bindings toggled in between.
    #[test]
    fn drain_starts_the_pending_binding_not_a_later_one() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();
        drive_into_processing(&mut state, now);

        let at = |ms| now + Duration::from_millis(ms);
        assert!(state.on_input(toggle_input(true), at(200)).is_none());
        assert!(state
            .on_input(toggle_input_for(OTHER_BINDING, true), at(300))
            .is_none());

        match state.on_processing_finished() {
            Some(Effect::Start { binding_id, .. }) => assert_eq!(binding_id, BINDING),
            other => panic!("expected Start for '{BINDING}', got {other:?}"),
        }
    }

    /// External triggers fire on every edge by design (e.g. SIGUSR2 sent on
    /// both key press and release). Two edges inside the debounce window must
    /// both be honoured, or the parity desyncs and recording wedges on.
    #[test]
    fn external_edges_inside_debounce_window_are_not_dropped() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        let effect = state.on_input(toggle_input(true), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));

        let effect = state.on_input(toggle_input(true), now + Duration::from_millis(5));
        assert!(
            matches!(effect, Some(Effect::Stop { .. })),
            "second external edge inside DEBOUNCE must stop the recording"
        );
        assert_eq!(state.stage, processing_stage(BINDING));
    }

    /// Physical keyboard presses keep the debounce: a repeat inside the window
    /// is still dropped and recording stays active.
    #[test]
    fn keyboard_press_inside_debounce_window_is_still_dropped() {
        let mut state = CoordinatorState::new();
        let now = Instant::now();

        let effect = state.on_input(toggle_input(false), now);
        assert!(matches!(effect, Some(Effect::Start { .. })));

        let effect = state.on_input(toggle_input(false), now + Duration::from_millis(5));
        assert!(
            effect.is_none(),
            "keyboard repeat inside DEBOUNCE must be debounced"
        );
        assert_eq!(state.stage, recording_stage(BINDING));
    }

    /// If the start effect fails to begin recording (e.g. microphone access
    /// denied), the optimistic transition rolls back to idle.
    #[test]
    fn failed_start_rolls_back_to_idle() {
        let mut state = CoordinatorState::new();

        let effect = state.on_input(ptt_input(true), Instant::now());
        assert!(matches!(effect, Some(Effect::Start { .. })));

        let orphaned = state.on_start_result(BINDING, false, None);
        assert_eq!(state.stage, Stage::Idle);
        assert_eq!(
            orphaned, None,
            "no session was ever allocated, so there is nothing to terminate"
        );
    }

    /// A concurrent cancel can stop the recorder and still let `hub.begin()`
    /// allocate a session for this same, now-dead start (`action.start()`
    /// starts the recorder before calling `hub.begin()`; see actions.rs).
    /// `on_start_result` must hand that session id back to its caller
    /// instead of silently discarding it on rollback -- `run_effect` is what
    /// actually terminates it (see its own doc comment), but this pins the
    /// pure decision that makes that possible: the id must survive the
    /// rollback path.
    #[test]
    fn start_result_with_no_recording_but_an_allocated_session_reports_it_for_termination() {
        let mut state = CoordinatorState::new();

        let effect = state.on_input(ptt_input(true), Instant::now());
        assert!(matches!(effect, Some(Effect::Start { .. })));

        let orphaned = state.on_start_result(BINDING, false, Some(9));
        assert_eq!(state.stage, Stage::Idle);
        assert_eq!(
            orphaned,
            Some(9),
            "a session hub.begin() allocated for a start that never actually \
             began recording must be reported so the caller can terminate it"
        );
    }

    /// A successful start folds the session `hub.begin()` allocated onto the
    /// already-`Recording` `Stage` -- it isn't known at `begin_recording`
    /// time, since `hub.begin()` runs inside the `Effect::Start` this call
    /// reports on, not before it.
    #[test]
    fn successful_start_result_records_its_publication_session_in_stage() {
        let mut state = CoordinatorState::new();

        let effect = state.on_input(ptt_input(true), Instant::now());
        assert!(matches!(effect, Some(Effect::Start { .. })));
        assert_eq!(
            state.stage,
            recording_stage(BINDING),
            "publication_session starts unknown -- hub.begin() hasn't run yet"
        );

        state.on_start_result(BINDING, true, Some(42));
        assert_eq!(
            state.stage,
            Stage::Recording {
                binding_id: BINDING.to_string(),
                mode: mode::mode_for_binding(BINDING),
                publication_session: Some(42),
            },
            "the session hub.begin() allocated must land on the active Stage"
        );
    }

    /// Pins the core Step-2 guarantee: the session `Stage::Recording` is
    /// carrying travels into the `Stop` effect (and `Stage::Processing`)
    /// explicitly, rather than the stop path having to read it from anywhere
    /// else -- there is no other source left to read it from.
    #[test]
    fn stopping_carries_the_recorded_publication_session_into_the_stop_effect() {
        let mut state = CoordinatorState::new();
        state.on_input(ptt_input(true), Instant::now());
        state.on_start_result(BINDING, true, Some(7));

        let effect = state.on_input(
            ptt_input(false),
            Instant::now() + Duration::from_millis(100),
        );
        assert!(effect.is_none(), "release should be deferred, not fired");
        let effect = state.on_grace_expired();

        match effect {
            Some(Effect::Stop {
                publication_session,
                ..
            }) => {
                assert_eq!(publication_session, Some(7));
            }
            other => panic!("expected Stop effect, got {other:?}"),
        }
        assert_eq!(
            state.stage,
            Stage::Processing {
                binding_id: BINDING.to_string(),
                mode: mode::mode_for_binding(BINDING),
                publication_session: Some(7),
            }
        );
    }

    // -------------------------------------------------------------------
    // Explicit `--start-assisted-notes` / `--stop-assisted-notes`.
    //
    // Regression coverage for the decide-then-toggle race: the old
    // `handle_explicit_assisted_notes_command` read capture state from
    // outside the coordinator thread, decided, and only then forwarded a
    // toggle press. Two retries could both observe `Idle` and the first
    // would start what the second then stopped. `decide_explicit_capture`
    // closes this by classifying `Stage` and mutating it in the same call,
    // so a second call always sees the first one's already-applied
    // transition -- exactly the atomicity a real two-command sequence gets
    // from being processed by one thread pulling off one channel.
    // -------------------------------------------------------------------

    const ASSISTED_NOTES: &str = "assisted_notes";
    const ASSISTED_NOTES_WITH_POST_PROCESS: &str = "assisted_notes_with_post_process";

    #[test]
    fn two_consecutive_explicit_starts_leave_exactly_one_capture_running() {
        let mut state = CoordinatorState::new();

        let first = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert!(
            matches!(first, ExplicitOutcome::Effect(Effect::Start { .. })),
            "first start must forward"
        );
        assert_eq!(state.stage, recording_stage(ASSISTED_NOTES));

        // The retry: unlike the removed external-snapshot path, this call
        // observes the `Stage` the first call already set, not a stale
        // "idle" read before it. It must NoOp, never forward a second Start
        // (which would toggle the capture back off) or a Stop.
        let second = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(
            second,
            ExplicitOutcome::NoOp,
            "retried start must be a no-op"
        );
        assert_eq!(
            state.stage,
            recording_stage(ASSISTED_NOTES),
            "the capture started by the first call must still be the one running"
        );
    }

    #[test]
    fn two_consecutive_explicit_stops_are_a_safe_noop() {
        let mut state = CoordinatorState::new();

        let first = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(first, ExplicitOutcome::NoOp, "nothing is capturing to stop");
        assert_eq!(state.stage, Stage::Idle);

        let second = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(second, ExplicitOutcome::NoOp);
        assert_eq!(state.stage, Stage::Idle);
    }

    /// A capture actually started (e.g. by the plain binding) and then
    /// stopped by an explicit stop must transition to `Processing`, and a
    /// second explicit stop right after must find nothing left to stop.
    #[test]
    fn explicit_stop_after_explicit_start_then_a_second_stop_is_a_noop() {
        let mut state = CoordinatorState::new();

        let started = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert!(matches!(
            started,
            ExplicitOutcome::Effect(Effect::Start { .. })
        ));

        let stopped = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        match stopped {
            ExplicitOutcome::Effect(Effect::Stop { binding_id, .. }) => {
                assert_eq!(binding_id, ASSISTED_NOTES);
            }
            other => panic!("expected Stop effect, got {other:?}"),
        }
        assert_eq!(state.stage, processing_stage(ASSISTED_NOTES));

        // decide() still sees `Capturing(AssistedNotes)` here (Processing
        // hasn't drained; see `capture_state_for_decision`), but there is no
        // live binding left, so this must resolve to NoOp rather than firing
        // a second Stop effect.
        let second_stop = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(second_stop, ExplicitOutcome::NoOp);
        assert_eq!(state.stage, processing_stage(ASSISTED_NOTES));
    }

    /// A stop must target whichever binding is actually recording, not the
    /// canonical one passed in -- the `_with_post_process` variant differs
    /// in whether the finished transcript gets post-processed, and stopping
    /// the wrong binding would silently drop that step.
    #[test]
    fn explicit_stop_targets_the_actually_running_post_process_binding() {
        let mut state = CoordinatorState::new();
        state.begin_recording(
            ASSISTED_NOTES_WITH_POST_PROCESS.to_string(),
            "keyboard".to_string(),
        );

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        match outcome {
            ExplicitOutcome::Effect(Effect::Stop { binding_id, .. }) => {
                assert_eq!(binding_id, ASSISTED_NOTES_WITH_POST_PROCESS);
            }
            other => panic!("expected Stop effect, got {other:?}"),
        }
    }

    /// An explicit start arriving while a *different* mode's pipeline is
    /// still draining (`Stage::Processing`) must be refused as busy, not
    /// forwarded -- forwarding here would land a second `Stage::Recording`
    /// that `on_processing_finished` would later stomp back to `Idle` when
    /// the first pipeline drains (see `capture_state_for_decision`).
    #[test]
    fn explicit_start_is_refused_while_a_different_mode_is_processing() {
        let mut state = CoordinatorState::new();
        state.begin_recording("transcribe".to_string(), "keyboard".to_string());
        state.begin_processing("transcribe".to_string(), "keyboard".to_string());
        assert_eq!(state.stage, processing_stage("transcribe"));

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            true,
            Mode::Meeting,
        );
        assert_eq!(
            outcome,
            ExplicitOutcome::Refuse(capture_command::RefusalReason::Busy)
        );
        assert_eq!(
            state.stage,
            processing_stage("transcribe"),
            "a refused start must not touch the in-flight pipeline's stage"
        );
    }

    /// Same window, but the mode already recorded is the one being
    /// re-started. This must refuse, not `NoOp`: the mic behind this
    /// `Processing` window is already closed (see `capture_state_for_decision`'s
    /// doc comment), so a `NoOp` here would report success while starting
    /// nothing — the pipeline then drains to `Idle` on its own and no
    /// capture is ever (re)started, with no observable record of that
    /// happening.
    #[test]
    fn explicit_start_while_the_same_mode_is_processing_is_refused_as_busy() {
        let mut state = CoordinatorState::new();
        state.begin_recording(ASSISTED_NOTES.to_string(), "keyboard".to_string());
        state.begin_processing(ASSISTED_NOTES.to_string(), "keyboard".to_string());
        assert_eq!(state.stage, processing_stage(ASSISTED_NOTES));

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(
            outcome,
            ExplicitOutcome::Refuse(capture_command::RefusalReason::Busy)
        );
        assert_eq!(
            state.stage,
            processing_stage(ASSISTED_NOTES),
            "a refused start must not touch the in-flight pipeline's stage"
        );
    }

    /// Stopping during `Processing` for the same mode, unlike starting, is
    /// still a safe `NoOp`: the capture already stopped (that is what
    /// `Processing` means), so there is nothing left for a second stop to
    /// do, and reporting that as success does not hide any missed effect.
    #[test]
    fn explicit_stop_while_the_same_mode_is_processing_is_a_noop() {
        let mut state = CoordinatorState::new();
        state.begin_recording(ASSISTED_NOTES.to_string(), "keyboard".to_string());
        state.begin_processing(ASSISTED_NOTES.to_string(), "keyboard".to_string());
        assert_eq!(state.stage, processing_stage(ASSISTED_NOTES));

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(outcome, ExplicitOutcome::NoOp);
        assert_eq!(state.stage, processing_stage(ASSISTED_NOTES));
    }

    /// Stopping during `Processing` for a *different* mode must likewise be
    /// a `NoOp` and must not touch the in-flight pipeline's stage.
    #[test]
    fn explicit_stop_while_a_different_mode_is_processing_is_a_noop() {
        let mut state = CoordinatorState::new();
        state.begin_recording("transcribe".to_string(), "keyboard".to_string());
        state.begin_processing("transcribe".to_string(), "keyboard".to_string());
        assert_eq!(state.stage, processing_stage("transcribe"));

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Stop,
            ASSISTED_NOTES,
            true,
            Mode::AssistedNotes,
        );
        assert_eq!(outcome, ExplicitOutcome::NoOp);
        assert_eq!(
            state.stage,
            processing_stage("transcribe"),
            "a stop for a mode that isn't running must not touch the in-flight pipeline's stage"
        );
    }

    #[test]
    fn explicit_start_is_refused_when_the_mode_is_disabled() {
        let mut state = CoordinatorState::new();

        let outcome = decide_explicit_capture(
            &mut state,
            capture_command::ExplicitOp::Start,
            ASSISTED_NOTES,
            false,
            Mode::AssistedNotes,
        );
        assert_eq!(
            outcome,
            ExplicitOutcome::Refuse(capture_command::RefusalReason::ModeDisabled)
        );
        assert_eq!(state.stage, Stage::Idle);
    }
}
