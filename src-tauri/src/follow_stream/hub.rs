use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use chrono::{DateTime, FixedOffset, Local};
use tokio::sync::Notify;

use crate::managers::transcription::StreamSource;
use crate::shorthand::capture_command::CaptureState as CoordinatorCaptureState;

use super::protocol::{
    CapturePhase, FollowEvent, FollowMode, RefusalReason, Speaker, Stamp, StartFailureCode,
    ERR_DISABLED, ERR_FOLLOWER_LIMIT, FOLLOW_PROTOCOL_VERSION,
};

/// Capabilities this binary advertises on `hello`. A control flag appears
/// here as the CLI flag minus its `--`; a new record type appears as its own
/// `t` value. One list, referenced from the one place `hello` is built, so
/// the advertised set can never drift from what `subscribe` actually sends.
/// See [`FollowEvent::Hello`]'s own doc comment for what each kind means.
///
/// `"refused"` says the `refused` record type exists at all; it says nothing
/// about which `reason` values it may carry. `"refused-publication-disabled"`
/// is the capability for the specific `reason:"publication-disabled"` value
/// (see `RefusalReason::PublicationDisabled` in protocol.rs): a follower that
/// wants to recognise that particular reason, rather than merely treat any
/// `refused` it doesn't understand as an unexplained refusal, gates on this
/// entry instead of assuming every installed binary that can send `refused`
/// at all can also send this specific reason. See FOLLOW_STREAM.md's
/// "Explicit start/stop commands" section for the full contract.
const CAPABILITIES: &[&str] = &[
    "toggle-assisted-notes",
    "start-assisted-notes",
    "stop-assisted-notes",
    "begin-mode",
    "capture-state",
    "refused",
    "refused-publication-disabled",
    "start-failed",
    "start-failed-code",
];

pub const MAX_FOLLOWERS: usize = 8;
pub const MAX_QUEUED_EVENTS: usize = 256;
/// Partials coalesce instead of accumulating, but a committed string grows for
/// the session. One MiB is roughly a full day of continuous speech, so this
/// only trips for a genuinely stuck follower, never a healthy mid-dictation one.
pub const MAX_BUFFERED_BYTES: usize = 1024 * 1024;

#[derive(Default)]
struct FollowerBuffer {
    queue: VecDeque<Arc<str>>,
    buffered_bytes: usize,
    partials: [Option<Arc<str>>; 2],
    overflowed: bool,
}

impl FollowerBuffer {
    fn push_partial(&mut self, speaker: Speaker, line: Arc<str>) {
        if self.overflowed {
            return;
        }

        let speaker_index = speaker.index();
        let old_len = self.partials[speaker_index]
            .as_ref()
            .map_or(0, |old| old.len());
        let new_total = self
            .buffered_bytes
            .checked_sub(old_len)
            .and_then(|without_old| without_old.checked_add(line.len()));
        let Some(new_total) = new_total.filter(|total| *total <= MAX_BUFFERED_BYTES) else {
            self.mark_overflowed();
            return;
        };

        self.partials[speaker_index] = Some(line);
        self.buffered_bytes = new_total;
    }

    fn push_event(&mut self, line: Arc<str>) {
        if self.overflowed {
            return;
        }

        let count_would_overflow = self.queue.len() >= MAX_QUEUED_EVENTS;
        let new_total = self
            .queue
            .iter()
            .map(|queued| queued.len())
            .try_fold(line.len(), usize::checked_add);
        let bytes_would_overflow = new_total
            .map(|total| total > MAX_BUFFERED_BYTES)
            .unwrap_or(true);
        if count_would_overflow || bytes_would_overflow {
            self.mark_overflowed();
            return;
        }

        self.buffered_bytes = new_total.expect("checked above");
        self.queue.push_back(line);
        self.partials = [None, None];
    }

    /// Queues a session-less control record (`refused`, `start_failed`) —
    /// unlike `push_event`, this does NOT clear unflushed partials. A `final`
    /// must never trail a stale `partial` of the text it just finalized, but
    /// a control record describes a declined *command*, not a session event:
    /// a `busy` refusal for one mode has nothing to do with another mode's
    /// active session, and clearing that session's latest undrained partial
    /// here would erase committed speech no later partial is guaranteed to
    /// repeat — if the session's own `final` arrives before another partial
    /// does, that text is gone from the wire for good. The bytes still count
    /// toward the shared budget: they are no longer being discarded to make
    /// room for this line, so the budget must still see them.
    fn push_control_event(&mut self, line: Arc<str>) {
        if self.overflowed {
            return;
        }

        let count_would_overflow = self.queue.len() >= MAX_QUEUED_EVENTS;
        let new_total = self.buffered_bytes.checked_add(line.len());
        let bytes_would_overflow = new_total
            .map(|total| total > MAX_BUFFERED_BYTES)
            .unwrap_or(true);
        if count_would_overflow || bytes_would_overflow {
            self.mark_overflowed();
            return;
        }

        self.buffered_bytes = new_total.expect("checked above");
        self.queue.push_back(line);
    }

    fn drain(&mut self) -> Vec<Arc<str>> {
        let mut lines = self.queue.drain(..).collect::<Vec<_>>();
        for partial in &mut self.partials {
            if let Some(line) = partial.take() {
                lines.push(line);
            }
        }
        self.buffered_bytes = 0;
        lines
    }

    fn overflowed(&self) -> bool {
        self.overflowed
    }

    fn mark_overflowed(&mut self) {
        self.overflowed = true;
        self.queue.clear();
        self.partials = [None, None];
        self.buffered_bytes = 0;
    }
}

pub struct Follower {
    id: u64,
    buffer: Mutex<FollowerBuffer>,
    notify: Notify,
    evicted: AtomicBool,
}

impl Follower {
    fn new(id: u64) -> Self {
        Self {
            id,
            buffer: Mutex::new(FollowerBuffer::default()),
            notify: Notify::new(),
            evicted: AtomicBool::new(false),
        }
    }

    fn mark_evicted(&self) {
        self.evicted.store(true, Ordering::Release);
        self.notify.notify_one();
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn drain(&self) -> Vec<Arc<str>> {
        self.buffer.lock().unwrap().drain()
    }

    pub fn is_evicted(&self) -> bool {
        self.evicted.load(Ordering::Acquire)
    }

    /// Waits until this follower has something to drain or has been evicted.
    /// Returns `false` once the follower is evicted; the consumer must stop.
    pub async fn wait(&self) -> bool {
        // The pre-check closes the drain-to-wait eviction race. If eviction or a
        // broadcast lands immediately after it, notify_one stores a permit until
        // notified().await consumes it, so neither kind of wake-up can be lost.
        if self.is_evicted() {
            return false;
        }
        self.notify.notified().await;
        !self.is_evicted()
    }
}

/// The two clocks every event is stamped with. Split behind a trait so tests can
/// assert byte-exact wire output; production always uses [`SystemClock`].
pub trait FollowClock: Send + Sync {
    /// Civil time, for `emitted_at`. Carries the machine's current UTC offset.
    fn wall(&self) -> DateTime<FixedOffset>;
    /// Monotonic time, for `session_elapsed_ms`.
    fn mono(&self) -> Instant;
}

pub struct SystemClock;

impl FollowClock for SystemClock {
    fn wall(&self) -> DateTime<FixedOffset> {
        Local::now().fixed_offset()
    }

    fn mono(&self) -> Instant {
        Instant::now()
    }
}

/// A clock the test can step by hand. Wall and monotonic time advance together,
/// so stamps are deterministic without being unrealistic.
#[cfg(test)]
pub(crate) struct TestClock {
    origin_wall: DateTime<FixedOffset>,
    origin_mono: Instant,
    offset_ms: std::sync::atomic::AtomicU64,
}

#[cfg(test)]
impl TestClock {
    /// The wall-clock origin every test clock starts at.
    pub(crate) const ORIGIN: &'static str = "2026-08-15T14:03:20.100-07:00";

    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            origin_wall: DateTime::parse_from_rfc3339(Self::ORIGIN).expect("valid origin"),
            origin_mono: Instant::now(),
            offset_ms: std::sync::atomic::AtomicU64::new(0),
        })
    }

    pub(crate) fn advance(&self, ms: u64) {
        self.offset_ms.fetch_add(ms, Ordering::AcqRel);
    }

    fn offset(&self) -> u64 {
        self.offset_ms.load(Ordering::Acquire)
    }
}

#[cfg(test)]
impl FollowClock for TestClock {
    fn wall(&self) -> DateTime<FixedOffset> {
        self.origin_wall + chrono::Duration::milliseconds(self.offset() as i64)
    }

    fn mono(&self) -> Instant {
        self.origin_mono + std::time::Duration::from_millis(self.offset())
    }
}

pub struct FollowStreamHub {
    enabled: AtomicBool,
    inner: Mutex<HubState>,
    clock: Arc<dyn FollowClock>,
}

struct HubState {
    next_session: u64,
    next_follower_id: u64,
    active: Option<ActiveSession>,
    /// The coordinator's authoritative lifecycle classification, mirrored
    /// here only so a newly accepted socket can receive it atomically with
    /// the hub's active `begin`. The hub never derives phase or mode from
    /// `active`: a non-publishing capture has no active session at all, which
    /// is exactly why the old hub-only `idle` inference was insufficient.
    capture_state: CaptureStateSnapshot,
    followers: Vec<Arc<Follower>>,
}

#[derive(Clone, Copy)]
struct CaptureStateSnapshot {
    phase: CapturePhase,
    mode: Option<FollowMode>,
    publishing: Option<bool>,
}

impl Default for CaptureStateSnapshot {
    fn default() -> Self {
        Self {
            phase: CapturePhase::Idle,
            mode: None,
            publishing: None,
        }
    }
}

struct ActiveSession {
    id: u64,
    /// Monotonic origin for this session's `session_elapsed_ms`.
    started: Instant,
    /// The session's own `begin` line, replayed verbatim to late attachers so
    /// they see when the session actually began, not when they arrived.
    begin_line: Arc<str>,
    /// Latest serialized partial line per speaker, for the late-attach snapshot.
    /// Stored already-stamped, for the same reason as `begin_line`.
    partial_lines: [Option<Arc<str>>; 2],
    /// The mode this session was opened for, so `suppress_if_active` can tell
    /// "the mode whose toggle just turned off" apart from "some other mode
    /// that happens to be capturing right now" without a caller having to
    /// pass a session id it may not have at hand (a settings setter knows
    /// only which mode's toggle changed).
    mode: FollowMode,
}

/// Which `FollowerBuffer` path a broadcast line takes.
///
/// `Event` is a real session lifecycle event (`begin`, and every terminal
/// `finish_with` produces) and clears any unflushed partial — a `final` must
/// never trail a stale `partial` of the text it just finalized. `Control` is
/// a session-less command record (`refused`, `start_failed`): it reports a
/// declined command rather than a session event, so it must leave whatever
/// session's undrained partial is currently buffered alone — see
/// `FollowerBuffer::push_control_event`'s own doc comment for the data loss
/// clearing it would cause.
#[derive(Clone, Copy)]
enum BroadcastKind {
    Partial(Speaker),
    Event,
    Control,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscribeError {
    Disabled,
    LimitReached,
}

impl SubscribeError {
    fn to_event(self) -> FollowEvent {
        let (code, message) = match self {
            Self::Disabled => (ERR_DISABLED, "follow stream is disabled"),
            Self::LimitReached => (
                ERR_FOLLOWER_LIMIT,
                "maximum number of follow-stream followers reached",
            ),
        };
        FollowEvent::Error {
            session: None,
            code: Some(code),
            message: message.to_string(),
        }
    }
}

impl Default for FollowStreamHub {
    fn default() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }
}

impl FollowStreamHub {
    pub fn with_clock(clock: Arc<dyn FollowClock>) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            inner: Mutex::new(HubState {
                next_session: 1,
                next_follower_id: 1,
                active: None,
                capture_state: CaptureStateSnapshot::default(),
                followers: Vec::new(),
            }),
            clock,
        }
    }

    /// Stamp for an event belonging to `started`'s session, or a session-less one
    /// (`hello`, connection-level `error`) when `started` is `None`.
    fn stamp(&self, started: Option<Instant>) -> Stamp {
        let elapsed = started.map(|started| {
            self.clock
                .mono()
                .saturating_duration_since(started)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        });
        Stamp::new(self.clock.wall(), elapsed)
    }

    /// `mode` is passed in rather than read here: the hub has no `AppHandle`,
    /// and the caller is `TranscribeAction::start`, which has already written
    /// the active-mode cell for this very capture.
    ///
    /// Returns the allocated session id, or `None` when the hub is disabled
    /// and no session was created. The caller (`TranscribeAction::start`)
    /// returns this onward so the coordinator's `Stage` can carry it (see
    /// `Stage`'s own doc comment in transcription_coordinator.rs) and hand it
    /// back explicitly to every terminal call made for this same capture,
    /// rather than each terminal call re-reading "whatever session is active
    /// now". The latter is what let a stale, already-queued terminal call
    /// finalize a session it did not belong to (a newer capture can begin
    /// before that queued call runs; see `finish_with`'s session check
    /// below).
    pub fn begin(&self, streaming: bool, mode: FollowMode) -> Option<u64> {
        if !self.enabled.load(Ordering::Acquire) {
            return None;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return None;
        }

        if let Some(orphaned) = state.active.take() {
            log::warn!(
                "Cancelling orphaned follow-stream session {} before starting a new one",
                orphaned.id
            );
            // Stamped against the orphan's own origin — it is that session's
            // terminal event, not the incoming one's.
            let stamp = self.stamp(Some(orphaned.started));
            let line = FollowEvent::Cancel {
                session: orphaned.id,
            }
            .to_line(&stamp);
            Self::broadcast(&mut state, line, BroadcastKind::Event);
        }

        let session = state.next_session;
        state.next_session += 1;

        let started = self.clock.mono();
        let line = FollowEvent::Begin {
            session,
            streaming,
            mode,
        }
        .to_line(&self.stamp(Some(started)));
        state.active = Some(ActiveSession {
            id: session,
            started,
            begin_line: Arc::clone(&line),
            partial_lines: [None, None],
            mode,
        });
        Self::broadcast(&mut state, line, BroadcastKind::Event);
        Some(session)
    }

    pub fn partial(&self, source: StreamSource, committed: &str, tentative: &str) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let speaker = Speaker::from(source);
        let Some(active) = state.active.as_ref() else {
            return;
        };
        let stamp = self.stamp(Some(active.started));
        let line = FollowEvent::Partial {
            session: active.id,
            speaker,
            committed: committed.to_string(),
            tentative: tentative.to_string(),
        }
        .to_line(&stamp);

        state
            .active
            .as_mut()
            .expect("active checked above")
            .partial_lines[speaker.index()] = Some(Arc::clone(&line));
        Self::broadcast(&mut state, line, BroadcastKind::Partial(speaker));
    }

    /// `session` must be the id `begin` returned for the capture this final
    /// text belongs to (see `begin`'s own doc comment for how that id
    /// reaches here). A call whose `session` does not match the hub's
    /// current active session is dropped — see `finish_with` — rather than
    /// finalizing whatever session happens to be active when this runs.
    pub fn finish(&self, session: u64, speaker: Option<Speaker>, text: &str) {
        self.finish_with(session, |session| FollowEvent::Final {
            session,
            speaker,
            text: text.to_string(),
        });
    }

    /// See `finish`'s doc comment: `session` gates this the same way.
    pub fn no_speech(&self, session: u64) {
        self.finish_with(session, |session| FollowEvent::NoSpeech { session });
    }

    /// See `finish`'s doc comment: `session` gates this the same way.
    pub fn cancel(&self, session: u64) {
        self.finish_with(session, |session| FollowEvent::Cancel { session });
    }

    /// Cancels whatever session is currently active, if any — for callers
    /// that need to end "the current operation" without an id in hand.
    /// `utils::cancel_current_operation` and the app-exit teardown in
    /// `lib.rs` both used to read one out of a process-wide cell
    /// (`follow_stream::last_begun_session`); now that `Stage` in
    /// `transcription_coordinator.rs` is the session id's one owner, neither
    /// caller can reach it synchronously from where they run —
    /// `cancel_current_operation` only notifies the coordinator
    /// asynchronously afterward, and exit teardown runs during shutdown,
    /// where depending on the coordinator's own thread still being
    /// responsive would be a new failure mode of its own. The hub already
    /// tracks its own active session (`HubState.active`) for the orphan
    /// check in `begin`, so asking it directly needs no id passed in and no
    /// second copy of the id resurrected to hold one.
    ///
    /// Also corrects the `capture_state` mirror's `publishing` bit in the
    /// same lock as ending the session, for the same reason
    /// `suppress_if_active` already does: `cancel_current_operation` calls
    /// this synchronously and only *afterward* notifies the coordinator
    /// asynchronously (`TranscriptionCoordinator::notify_cancel`), which is
    /// what eventually calls `set_capture_state` with the coordinator's own
    /// correction. Between those two points — which can be an arbitrarily
    /// long window, since the coordinator thread may be mid-`run_effect`
    /// doing model kickoff, tray or overlay work — a subscriber would
    /// otherwise see `capture_state` still claim `publishing:true` for a
    /// session this call has just ended: `session` is already correctly
    /// absent (derived from `active`, which is cleared below), but
    /// `publishing` was not, which is a positive claim that a live,
    /// publishing capture exists when it does not.
    ///
    /// This intentionally does *not* touch `phase`/`mode`, and does not reset
    /// to `Idle`: `CoordinatorState::on_cancel` deliberately leaves `Stage`
    /// alone while `Processing` (the pipeline can legitimately keep running
    /// after a cancel), and only the coordinator knows which is true.
    /// `publishing:false` is the one thing this call can assert honestly
    /// regardless of that — no session is left running here to publish
    /// anything — so it never over-reports a live publishing capture; at
    /// worst it briefly under-reports a phase the coordinator's own
    /// `set_capture_state` call corrects moments later. That later call is
    /// never blocked by this one: `set_capture_state` replaces the whole
    /// snapshot rather than merging into it, so nothing set here is sticky.
    ///
    /// Skipped when the mirror is not currently claiming any mode (i.e. it
    /// is already `Idle`): forcing `publishing:Some(false)` onto an idle
    /// snapshot would violate FOLLOW_STREAM.md's "while idle, `mode`,
    /// `publishing`, and `session` are omitted" and gain nothing, since idle
    /// already implies nothing is publishing.
    ///
    /// A session that gets superseded between this call reading `active` and
    /// clearing it is simply a no-op, the same tolerance the old cell-based
    /// path already had — there is only one lock acquisition now, so that
    /// can only happen across two separate calls to `cancel_active`, not
    /// within one.
    pub fn cancel_active(&self) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        if state.capture_state.mode.is_some() {
            state.capture_state.publishing = Some(false);
        }

        let Some(active) = state.active.take() else {
            return;
        };
        let stamp = self.stamp(Some(active.started));
        let line = FollowEvent::Cancel { session: active.id }.to_line(&stamp);
        Self::broadcast(&mut state, line, BroadcastKind::Event);
    }

    /// See `finish`'s doc comment: `session` gates this the same way.
    pub fn error(&self, session: u64, message: &str) {
        self.finish_with(session, |session| FollowEvent::Error {
            session: Some(session),
            code: None,
            message: message.to_string(),
        });
    }

    /// Broadcasts that a capture request never produced a `begin`. Unlike
    /// `error`, `no_speech` etc. this closes no active session — there isn't
    /// one, by construction (`begin` only fires after `try_start_recording`
    /// succeeds) — so it is stamped session-less like `hello` rather than
    /// against a session origin.
    pub fn start_failed(&self, mode: FollowMode, code: StartFailureCode, message: &str) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let line = FollowEvent::StartFailed {
            mode,
            code,
            message: message.to_string(),
        }
        .to_line(&self.stamp(None));
        Self::broadcast(&mut state, line, BroadcastKind::Control);
    }

    /// Broadcasts that the app declined an explicit start/stop command. Also
    /// session-less: a refusal opens and closes nothing.
    pub fn refused(&self, mode: FollowMode, reason: RefusalReason) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let line = FollowEvent::Refused { mode, reason }.to_line(&self.stamp(None));
        Self::broadcast(&mut state, line, BroadcastKind::Control);
    }

    /// Ends the active session if, and only if, it belongs to `mode` —
    /// called when `mode`'s own publication toggle is switched off while a
    /// capture for it is in flight.
    ///
    /// Without this, turning that toggle off mid-capture did nothing: the
    /// listener is unconditional (see FOLLOW_STREAM.md's note on publication
    /// vs. listener lifetime) and `begin`/`partial`/`finish_with` only check
    /// the toggle at the moment each of *their own* calls runs, not whether
    /// it is still on by the time a later call for the same session arrives.
    /// A follower already attached kept receiving `partial`/`final` for a
    /// mode the user had just told the app to stop streaming.
    ///
    /// This reuses `Cancel` rather than adding a new wire record: a follower
    /// already reads `Cancel` as "this session produced no committed final
    /// text", which is exactly true here, and a new record type is a
    /// consumer-visible protocol addition this task has no reason to make.
    /// Taking `state.active` (rather than leaving it open and merely
    /// dropping future broadcasts) is deliberate too: it makes the capture's
    /// real, eventual `hub.finish`/`hub.no_speech`/etc. for this same session
    /// id fall through `finish_with`'s existing stale-session check and
    /// no-op on its own, instead of this method needing its own "publication
    /// suppressed" flag that every other publisher would then have to
    /// consult. Either way the follower must end up with a `begin` and
    /// exactly one terminal, never a `begin` left dangling — the same
    /// invariant `a_start_cancelled_before_begin_still_gets_a_terminal`
    /// pins for the unrelated race it was written for.
    pub fn suppress_if_active(&self, mode: FollowMode) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        // The settings command supplies `mode`; the hub still does not read
        // settings itself. Update the mirrored publication bit even when no
        // session exists (a non-publishing capture has none) so a late
        // subscriber sees why this in-flight capture cannot produce a begin.
        if state.capture_state.mode == Some(mode) {
            state.capture_state.publishing = Some(false);
        }

        match state.active.as_ref() {
            Some(active) if active.mode == mode => {}
            _ => return,
        }

        let active = state.active.take().expect("checked above");
        let stamp = self.stamp(Some(active.started));
        let line = FollowEvent::Cancel { session: active.id }.to_line(&stamp);
        Self::broadcast(&mut state, line, BroadcastKind::Event);
    }

    pub fn set_enabled(&self, enabled: bool) {
        let was_enabled = self.enabled.swap(enabled, Ordering::AcqRel);
        if !was_enabled || enabled {
            return;
        }

        let followers = {
            let mut state = self.inner.lock().unwrap();
            state.active = None;
            std::mem::take(&mut state.followers)
        };
        for follower in followers {
            follower.mark_evicted();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Mirrors the coordinator's authoritative capture classification for
    /// the next subscriber snapshot. `publishing` is already resolved by the
    /// coordinator where `AppHandle` and per-mode settings are available;
    /// this settings-free hub merely stores the supplied value.
    ///
    /// This update intentionally happens even while the listener is disabled.
    /// Listener lifetime and capture lifetime are independent, and a listener
    /// restarted during a capture must not regress to reporting idle.
    pub fn set_capture_state(&self, capture: CoordinatorCaptureState, publishing: bool) {
        let capture_state = match capture {
            CoordinatorCaptureState::Idle => CaptureStateSnapshot::default(),
            CoordinatorCaptureState::Recording(mode) => CaptureStateSnapshot {
                phase: CapturePhase::Recording,
                mode: Some(mode.into()),
                publishing: Some(publishing),
            },
            CoordinatorCaptureState::Processing(mode) => CaptureStateSnapshot {
                phase: CapturePhase::Processing,
                mode: Some(mode.into()),
                publishing: Some(publishing),
            },
        };
        self.inner.lock().unwrap().capture_state = capture_state;
    }

    pub fn subscribe(
        &self,
        app_version: &str,
    ) -> Result<(Arc<Follower>, Vec<Arc<str>>), SubscribeError> {
        if !self.enabled.load(Ordering::Acquire) {
            return Err(SubscribeError::Disabled);
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return Err(SubscribeError::Disabled);
        }
        if state.followers.len() >= MAX_FOLLOWERS {
            return Err(SubscribeError::LimitReached);
        }

        let follower = Arc::new(Follower::new(state.next_follower_id));
        state.next_follower_id += 1;

        // `hello` describes this connection, so it is stamped now and carries no
        // `session_elapsed_ms`. Everything after it is replayed verbatim from the
        // active session, keeping the timestamps the events were produced with.
        let mut backlog = vec![FollowEvent::Hello {
            protocol: FOLLOW_PROTOCOL_VERSION,
            version: app_version.to_string(),
            capabilities: CAPABILITIES.to_vec(),
        }
        .to_line(&self.stamp(None))];
        let capture = state.capture_state;
        // Derived from `state.active` alone, on purpose -- the same condition
        // the `begin` replay below tests -- rather than cross-checked against
        // `capture.mode`/`capture.publishing`. An earlier version gated
        // `session` on that pair matching `active.mode`, but the `begin`
        // replay a few lines down was never gated on it: if the mirror ever
        // disagreed with the hub's own `active` (the mirror is the
        // coordinator's copy, kept in step by `publish_capture_state`, not
        // this hub's own source of truth), `session` could come out absent
        // while `begin` was replayed anyway, contradicting the invariant this
        // module already promises in FOLLOW_STREAM.md: "the hub contributes
        // only the active session ID it owns, which is what guarantees the ID
        // matches the replayed `begin`". The hub owns both `active` and its
        // `begin_line`, so deriving `session` from `active` directly makes
        // that guarantee hold by construction instead of by two independent
        // conditions happening to agree. No production caller has been found
        // that can make the mirror and `active` disagree (see
        // `suppress_if_active` and `publish_capture_state`, which keep them
        // in step), so this is hardening the stated invariant, not fixing an
        // observed wire bug.
        let session = state.active.as_ref().map(|active| active.id);
        backlog.push(
            FollowEvent::CaptureState {
                phase: capture.phase,
                mode: capture.mode,
                publishing: capture.publishing,
                session,
            }
            .to_line(&self.stamp(None)),
        );
        if let Some(active) = &state.active {
            backlog.push(Arc::clone(&active.begin_line));
            backlog.extend(active.partial_lines.iter().flatten().map(Arc::clone));
        }

        state.followers.push(Arc::clone(&follower));
        Ok((follower, backlog))
    }

    /// Renders a rejection for a connection that never became a follower. It has
    /// no session, so it carries only `emitted_at`.
    pub fn rejection_line(&self, error: SubscribeError) -> Arc<str> {
        error.to_event().to_line(&self.stamp(None))
    }

    pub fn unsubscribe(&self, id: u64) {
        let follower = {
            let mut state = self.inner.lock().unwrap();
            state
                .followers
                .iter()
                .position(|follower| follower.id() == id)
                .map(|index| state.followers.remove(index))
        };
        if let Some(follower) = follower {
            follower.mark_evicted();
        }
    }

    pub fn follower_count(&self) -> usize {
        self.inner.lock().unwrap().followers.len()
    }

    /// Applies a terminal event to the active session, but only if `session`
    /// is that session's own id.
    ///
    /// Without this check, a terminal call queued for one capture can still
    /// run after a *newer* capture has begun — `TranscribeAction::stop`
    /// queues its `hub.finish` call onto the main thread, and `FinishGuard`
    /// tells the coordinator the pipeline is free (permitting a new capture
    /// to start) as soon as that queueing returns, not once the queued call
    /// has actually executed. An unscoped `finish_with` would then finalize
    /// the NEW session with the OLD capture's text. Comparing `session`
    /// against `state.active`'s own id closes that window regardless of
    /// which future code path manages to start a capture quickly, rather
    /// than relying on some timing margin between "coordinator goes idle"
    /// and "queued closure runs" staying wide enough forever.
    fn finish_with(&self, session: u64, make_event: impl FnOnce(u64) -> FollowEvent) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        match state.active.as_ref() {
            Some(active) if active.id == session => {}
            Some(active) => {
                // Diagnosable rather than silent: this is the exact shape a
                // stale-session bug takes on the wire (nothing happens), so a
                // log line naming both ids is the only way to tell "working
                // as designed" apart from "a terminal call went missing".
                log::debug!(
                    "Dropping stale follow-stream terminal call for session {session}; \
                     the active session is {}",
                    active.id
                );
                return;
            }
            None => return,
        }

        let active = state.active.take().expect("checked above");
        let stamp = self.stamp(Some(active.started));
        let line = make_event(active.id).to_line(&stamp);
        Self::broadcast(&mut state, line, BroadcastKind::Event);
    }

    fn broadcast(state: &mut HubState, line: Arc<str>, kind: BroadcastKind) {
        state.followers.retain(|follower| {
            let overflowed = {
                let mut buffer = follower.buffer.lock().unwrap();
                match kind {
                    BroadcastKind::Partial(speaker) => {
                        buffer.push_partial(speaker, Arc::clone(&line))
                    }
                    BroadcastKind::Event => buffer.push_event(Arc::clone(&line)),
                    BroadcastKind::Control => buffer.push_control_event(Arc::clone(&line)),
                }
                buffer.overflowed()
            };

            if overflowed {
                log::warn!(
                    "Evicting follow-stream follower {} after its buffer overflowed",
                    follower.id()
                );
                follower.mark_evicted();
                false
            } else {
                follower.notify.notify_one();
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shorthand::mode::Mode;

    fn strings(lines: Vec<Arc<str>>) -> Vec<String> {
        lines.into_iter().map(|line| line.to_string()).collect()
    }

    /// Lifecycle assertions care about which events arrived, not when. The stamp
    /// is always the trailing pair of fields, so drop it to keep those
    /// expectations readable; the stamped bytes are asserted on their own below.
    fn events(lines: Vec<Arc<str>>) -> Vec<String> {
        lines
            .into_iter()
            .map(|line| {
                let start = line
                    .find(",\"emitted_at\":")
                    .unwrap_or_else(|| panic!("every event is stamped, got {line}"));
                format!("{}}}\n", &line[..start])
            })
            .collect()
    }

    /// The `session_elapsed_ms` of each line, or `None` where the field is
    /// absent because the event belongs to no session.
    fn elapsed_values(lines: Vec<Arc<str>>) -> Vec<Option<u64>> {
        lines
            .into_iter()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(&line)
                    .expect("event line is JSON")
                    .get("session_elapsed_ms")
                    .and_then(serde_json::Value::as_u64)
            })
            .collect()
    }

    fn enabled_hub(clock: &Arc<TestClock>) -> FollowStreamHub {
        let hub = FollowStreamHub::with_clock(Arc::clone(clock) as Arc<dyn FollowClock>);
        hub.set_enabled(true);
        hub
    }

    async fn wait_for_line_count(lines: &Arc<Mutex<Vec<Arc<str>>>>, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let current = lines.lock().unwrap().len();
                if current >= expected {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("consumer did not write expected lines in time");
    }

    #[test]
    fn partials_coalesce_to_the_newest_line_for_one_speaker() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Me, Arc::from("first\n"));
        buffer.push_partial(Speaker::Me, Arc::from("second\n"));
        buffer.push_partial(Speaker::Me, Arc::from("third\n"));

        assert_eq!(strings(buffer.drain()), ["third\n"]);
    }

    #[test]
    fn both_speaker_partial_slots_drain_in_speaker_order() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Them, Arc::from("them\n"));
        buffer.push_partial(Speaker::Me, Arc::from("me\n"));

        assert_eq!(strings(buffer.drain()), ["me\n", "them\n"]);
    }

    #[test]
    fn lifecycle_event_clears_unflushed_partials() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Me, Arc::from("partial\n"));
        buffer.push_event(Arc::from("final\n"));

        assert_eq!(strings(buffer.drain()), ["final\n"]);
    }

    #[test]
    fn control_event_preserves_unflushed_partials_and_still_drains_in_order() {
        // FIX 2: unlike a real lifecycle event, a control record (`refused`,
        // `start_failed`) must not erase an undrained partial -- it
        // describes a declined command, not this session's own terminal.
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Me, Arc::from("partial\n"));
        buffer.push_control_event(Arc::from("refused\n"));

        assert_eq!(strings(buffer.drain()), ["refused\n", "partial\n"]);
    }

    #[test]
    fn lifecycle_events_are_fifo_and_precede_partial_slots() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_event(Arc::from("begin\n"));
        buffer.push_event(Arc::from("other lifecycle\n"));
        buffer.push_partial(Speaker::Them, Arc::from("them\n"));
        buffer.push_partial(Speaker::Me, Arc::from("me\n"));

        assert_eq!(
            strings(buffer.drain()),
            ["begin\n", "other lifecycle\n", "me\n", "them\n"]
        );
    }

    #[test]
    fn lifecycle_event_count_overflow_clears_the_buffer() {
        let mut buffer = FollowerBuffer::default();
        for _ in 0..=MAX_QUEUED_EVENTS {
            buffer.push_event(Arc::from("event\n"));
        }

        assert!(buffer.overflowed());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn lifecycle_byte_overflow_trips_well_before_count_limit() {
        let mut buffer = FollowerBuffer::default();
        let large: Arc<str> = Arc::from("x".repeat(MAX_BUFFERED_BYTES / 2 + 1));
        buffer.push_event(Arc::clone(&large));
        assert_eq!(buffer.queue.len(), 1);
        assert!(!buffer.overflowed());
        buffer.push_event(large);

        assert!(buffer.overflowed());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn oversized_partial_overflows_and_clears_the_buffer() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Me, Arc::from("x".repeat(MAX_BUFFERED_BYTES + 1)));

        assert!(buffer.overflowed());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn replacing_a_partial_charges_only_the_new_slot_size() {
        let mut buffer = FollowerBuffer::default();
        let partial: Arc<str> = Arc::from("x".repeat(MAX_BUFFERED_BYTES / 2));
        for _ in 0..10 {
            buffer.push_partial(Speaker::Me, Arc::clone(&partial));
        }

        assert!(!buffer.overflowed());
        assert_eq!(buffer.buffered_bytes, partial.len());
        assert_eq!(strings(buffer.drain()).len(), 1);
    }

    #[test]
    fn queue_and_partial_bytes_share_one_budget() {
        let mut buffer = FollowerBuffer::default();
        buffer.push_event(Arc::from("q".repeat(MAX_BUFFERED_BYTES - 32)));
        assert!(!buffer.overflowed());
        buffer.push_partial(Speaker::Them, Arc::from("p".repeat(33)));

        assert!(buffer.overflowed());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn control_event_that_cannot_fit_alongside_preserved_partials_overflows() {
        // A control record's bytes still count toward the shared budget --
        // it is no longer discarding the partials it preserves to make room
        // for itself, so it must still overflow like anything else that
        // doesn't fit.
        let mut buffer = FollowerBuffer::default();
        buffer.push_partial(Speaker::Me, Arc::from("p".repeat(MAX_BUFFERED_BYTES - 32)));
        assert!(!buffer.overflowed());

        buffer.push_control_event(Arc::from("c".repeat(33)));

        assert!(buffer.overflowed());
        assert!(buffer.drain().is_empty());
    }

    #[test]
    fn partial_without_an_active_session_emits_nothing() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.partial(StreamSource::Mic, "ignored", "");

        assert!(follower.drain().is_empty());
    }

    #[test]
    fn finish_no_speech_and_partial_without_begin_broadcast_nothing() {
        // Dictation must never reach the follow-stream hub: `TranscribeAction::start`
        // skips `hub.begin()` for a dictation capture (see the actions.rs change in
        // this task), and this test pins the consequence that makes that single skip
        // sufficient — every other hub call is already a silent no-op without a
        // preceding `begin`. If a later refactor to `finish_with` or `partial` ever
        // breaks that, this test catches it even though nothing here calls `begin`.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        // Session 1 has never been allocated (no `begin` ran), so any id
        // presented here is necessarily stale; 1 is as good as any other.
        hub.finish(1, Some(Speaker::Me), "orphaned final");
        hub.no_speech(1);
        hub.partial(StreamSource::Mic, "ignored", "");

        assert!(follower.drain().is_empty());
    }

    #[test]
    fn sessions_emit_at_most_one_terminal_event() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.finish(1, Some(Speaker::Me), "orphaned final");
        assert!(follower.drain().is_empty());

        let first = hub.begin(false, FollowMode::Meeting).unwrap();
        hub.no_speech(first);
        hub.error(first, "late error");
        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":false,\"mode\":\"meeting\"}\n",
                "{\"t\":\"no_speech\",\"session\":1}\n",
            ]
        );

        let second = hub.begin(true, FollowMode::Meeting).unwrap();
        hub.cancel(second);
        hub.finish(second, Some(Speaker::Me), "late final");
        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"begin\",\"session\":2,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"cancel\",\"session\":2}\n",
            ]
        );
    }

    #[test]
    fn cancel_active_ends_whatever_session_is_currently_open() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        hub.cancel_active();

        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"cancel\",\"session\":1}\n"]
        );
    }

    #[test]
    fn cancel_active_is_a_noop_with_nothing_open() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        follower.drain();

        hub.cancel_active();

        assert!(follower.drain().is_empty());
    }

    #[test]
    fn cancel_active_marks_the_mirror_not_publishing_so_a_subscriber_sees_no_live_capture() {
        // FIX 1 (regression): `utils::cancel_current_operation` calls
        // `hub.cancel_active()` synchronously, then only *afterward* notifies
        // the coordinator asynchronously, which is what eventually corrects
        // `capture_state` via `set_capture_state`. Before this fix, a
        // subscriber attaching in that window saw `capture_state` still claim
        // `publishing:true` for a session `cancel_active` had just ended --
        // `session` was already correctly absent, but `publishing` was not, a
        // positive claim that a live, publishing capture exists when it does
        // not. This pins that `cancel_active` corrects `publishing` itself,
        // atomically with ending the session, without waiting for the
        // coordinator to catch up. `phase`/`mode` are deliberately left
        // alone -- they are the coordinator's to correct (see the doc
        // comment on `cancel_active` for why forcing `idle` would
        // over-correct the `Processing` case).
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Meeting), true);
        hub.begin(true, FollowMode::Meeting).unwrap();

        hub.cancel_active();

        let (_, initial) = hub.subscribe("0.9.5").unwrap();
        let initial = events(initial);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(initial[1].trim()).unwrap(),
            serde_json::json!({
                "t": "capture_state",
                "phase": "recording",
                "mode": "meeting",
                "publishing": false
            }),
            "a subscriber must not see publishing:true for a session cancel_active just ended"
        );
    }

    #[test]
    fn cancel_active_leaves_the_mirror_idle_alone() {
        // Companion to the test above: if the mirror is already `Idle` (no
        // mode claimed), `cancel_active` must not introduce a `publishing`
        // value into it. FOLLOW_STREAM.md is explicit that an idle
        // `capture_state` omits `mode`, `publishing`, and `session` --
        // forcing `publishing:Some(false)` in here would violate that for no
        // benefit, since idle already implies nothing is publishing.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);

        hub.cancel_active();

        let (_, initial) = hub.subscribe("0.9.5").unwrap();
        let initial = events(initial);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(initial[1].trim()).unwrap(),
            serde_json::json!({"t": "capture_state", "phase": "idle"})
        );
    }

    #[test]
    fn set_capture_state_still_overwrites_what_cancel_active_set() {
        // FIX 1's other half: `cancel_active`'s correction must not be
        // sticky. Once the coordinator's own `Command::Cancel` handling
        // dequeues and calls `set_capture_state` (see
        // `transcription_coordinator.rs`), that call must win outright --
        // `set_capture_state` replaces the whole snapshot rather than merging
        // into it, so there is nothing here for `cancel_active` to have left
        // behind that could resist being overwritten.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Meeting), true);
        hub.begin(true, FollowMode::Meeting).unwrap();

        hub.cancel_active();

        // The coordinator's own correction, arriving later: `on_cancel`
        // leaves `Stage::Processing` alone, so it can report `processing`
        // rather than `idle` even after this cancel.
        hub.set_capture_state(CoordinatorCaptureState::Processing(Mode::Meeting), true);

        let (_, initial) = hub.subscribe("0.9.5").unwrap();
        let initial = events(initial);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(initial[1].trim()).unwrap(),
            serde_json::json!({
                "t": "capture_state",
                "phase": "processing",
                "mode": "meeting",
                "publishing": true
            }),
            "the coordinator's later set_capture_state must fully overwrite cancel_active's \
             correction, proving nothing sticky was introduced"
        );
    }

    #[test]
    fn subscribe_session_always_matches_the_replayed_begin_even_if_the_mirror_disagrees() {
        // FIX 2 (hardening): `session` must be derived from the same
        // condition that decides whether `begin` is replayed
        // (`state.active`), not cross-checked against the coordinator's
        // `capture_state` mirror's `mode`/`publishing`. Forcing a mismatch
        // here -- a shape the reviewer could not reach through any
        // production caller, only through the hub's own public API, see
        // FOLLOW_STREAM.md's "Connection state" section -- pins that even
        // when the mirror disagrees with the hub's own active session, a
        // subscriber still gets a `session` that matches the `begin` it
        // replays, never one without the other.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);

        // Mirror claims a different, non-publishing mode is recording --
        // deliberately at odds with the hub's own active session below.
        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Dictation), false);
        let session = hub.begin(true, FollowMode::Meeting).unwrap();

        let (_, initial) = hub.subscribe("0.9.5").unwrap();
        let initial = events(initial);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(initial[1].trim()).unwrap(),
            serde_json::json!({
                "t": "capture_state",
                "phase": "recording",
                "mode": "dictation",
                "publishing": false,
                "session": session
            }),
            "session must be present and match the active session even though the mirror's \
             own mode/publishing disagree with it"
        );
        assert_eq!(
            initial[2], "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
            "begin must still be replayed for the same session id capture_state just reported"
        );
    }

    #[test]
    fn cancel_active_does_not_touch_a_session_it_did_not_read() {
        // A stale session id presented through some other path (e.g. a
        // terminal call queued before session 1 ended) must not cancel
        // whatever replaced it, and `cancel_active` itself -- which now reads
        // and clears `state.active` under one lock acquisition rather than
        // two (see its own doc comment) -- must still end whichever session
        // is actually current when it runs.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let first = hub.begin(true, FollowMode::Meeting).unwrap();
        hub.no_speech(first); // session 1 ends normally
        hub.begin(true, FollowMode::Meeting).unwrap(); // session 2 is now active
        follower.drain();

        // Simulates the stale id `cancel_active` could have captured just
        // before session 1 ended and session 2 began: presenting it now must
        // not touch session 2.
        hub.cancel(first);
        assert!(
            follower.drain().is_empty(),
            "a stale id must not cancel the session that replaced it"
        );

        hub.cancel_active();
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"cancel\",\"session\":2}\n"]
        );
    }

    #[test]
    fn a_start_cancelled_before_begin_still_gets_a_terminal() {
        // Pins the actions.rs/transcription_coordinator.rs fix: `action.start()`
        // starts the recorder before calling `hub.begin()`, so a concurrent
        // cancel can stop the recorder and find no active hub session to end
        // -- `hub.begin()` then runs anyway and publishes a session for a
        // capture that is already dead. `on_start_result` reports that
        // session's id back to `run_effect` on rollback (`started == false`
        // but a `publication_session` was allocated), which is what calls
        // `hub.cancel` with it below. Without that call the session would sit
        // open until some unrelated later `begin` happened to notice it as
        // orphaned (see `begin`'s own orphan handling) -- an arbitrarily long
        // wait during which a follower has a `begin` with no terminal at all.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let session = hub.begin(true, FollowMode::AssistedNotes).unwrap();
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"assisted-notes\"}\n"]
        );

        // What `run_effect` does when `on_start_result` reports this session
        // as orphaned by the rollback.
        hub.cancel(session);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"cancel\",\"session\":1}\n"],
            "the follower must see a terminal for the cancelled start, not be left on begin alone"
        );

        // The session is fully closed -- not merely appearing to a single
        // follower -- so a later terminal call for the same id is dropped as
        // stale rather than reopening or double-closing it.
        hub.no_speech(session);
        assert!(
            follower.drain().is_empty(),
            "a session that already received its terminal must not emit a second one"
        );
    }

    #[test]
    fn a_terminal_call_for_a_superseded_session_does_not_close_the_new_one() {
        // Pins the fix for the stale-terminal bug: `TranscribeAction::stop`
        // queues its `hub.finish` call onto the main thread, and the
        // coordinator can admit a brand new capture before that queued call
        // actually runs (see `FinishGuard`'s doc comment in actions.rs). If
        // `finish_with` finalized whatever session happens to be active
        // instead of checking the id it was given, this stale call would
        // steal session 1's text onto session 2's `final` record.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let first = hub.begin(true, FollowMode::Meeting).unwrap();
        hub.no_speech(first); // session 1 ends normally, with no orphan involved
        let second = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        // The stale call: presents session 1's id, long after session 2 began.
        hub.finish(first, Some(Speaker::Me), "stale text from session 1");
        assert!(
            follower.drain().is_empty(),
            "a stale terminal call must not broadcast anything"
        );

        // Session 2 is still open — the stale call above must not have
        // consumed it — and its own matching terminal still closes it.
        hub.no_speech(second);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"no_speech\",\"session\":2}\n"]
        );
    }

    #[test]
    fn a_finished_session_is_not_orphan_cancelled_by_the_next_begin() {
        // Pins the actions.rs fix: `TranscribeAction::stop` now calls
        // `hub.finish` synchronously on the worker, before `FinishGuard`
        // drops and lets the coordinator admit a new capture — rather than
        // queuing `hub.finish` onto the main thread as part of the paste
        // closure. Under the old ordering, a rapid next capture's
        // `hub.begin` could run before that queued closure did, find this
        // session's `ActiveSession` still open, and force-cancel it as
        // orphaned (see `begin`'s own orphan handling below); the delayed
        // `hub.finish` then found the session already replaced and silently
        // dropped as stale (see `finish_with`), so a transcript that
        // actually completed reached the wire as `cancel` instead of
        // `final`. This test exercises the two calls in the order the fix
        // now guarantees — `finish` completes before the next `begin` is
        // ever issued — and pins that no orphan `cancel` appears between
        // them.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let first = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        hub.finish(first, Some(Speaker::Me), "completed transcript");
        let second = hub.begin(true, FollowMode::Meeting).unwrap();
        assert_eq!(second, 2, "the next capture still gets a fresh session");

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"completed transcript\"}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":true,\"mode\":\"meeting\"}\n",
            ],
            "a cleanly finished session must not be orphan-cancelled by the next begin"
        );
    }

    #[test]
    fn a_matching_terminal_call_still_closes_its_own_session() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let session = hub.begin(true, FollowMode::AssistedNotes).unwrap();
        follower.drain();
        hub.finish(session, Some(Speaker::Me), "correct session");

        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"correct session\"}\n"]
        );
    }

    #[test]
    fn follower_observes_begin_partial_and_final_in_order() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"]}\n",
                "{\"t\":\"capture_state\",\"phase\":\"idle\"}\n",
            ]
        );
        let mut observed = Vec::new();
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        observed.extend(follower.drain());
        hub.partial(StreamSource::Mic, "hello ", "wor");
        observed.extend(follower.drain());
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        observed.extend(follower.drain());

        assert_eq!(
            events(observed),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }

    #[test]
    fn hello_advertises_capture_state_and_start_failed_code_capabilities() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (_follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"]}\n",
                "{\"t\":\"capture_state\",\"phase\":\"idle\"}\n",
            ]
        );
    }

    #[test]
    fn subscription_emits_capture_state_for_every_phase_and_matches_the_following_begin_session() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);

        let (_, idle) = hub.subscribe("0.9.5").unwrap();
        let idle = events(idle);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(idle[1].trim()).unwrap(),
            serde_json::json!({"t": "capture_state", "phase": "idle"})
        );

        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Dictation), false);
        let (_, recording) = hub.subscribe("0.9.5").unwrap();
        let recording = events(recording);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(recording[1].trim()).unwrap(),
            serde_json::json!({
                "t": "capture_state",
                "phase": "recording",
                "mode": "dictation",
                "publishing": false
            })
        );
        assert_eq!(recording.len(), 2, "a non-publishing capture has no begin");

        hub.set_capture_state(
            CoordinatorCaptureState::Processing(Mode::AssistedNotes),
            true,
        );
        let session = hub.begin(true, FollowMode::AssistedNotes).unwrap();
        let (_, processing) = hub.subscribe("0.9.5").unwrap();
        let processing = events(processing);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(processing[1].trim()).unwrap(),
            serde_json::json!({
                "t": "capture_state",
                "phase": "processing",
                "mode": "assisted-notes",
                "publishing": true,
                "session": session
            })
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(processing[2].trim()).unwrap()["session"],
            session,
            "capture_state.session must identify the immediately following begin"
        );
    }

    #[test]
    fn begin_carries_the_mode_it_was_given() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.begin(true, FollowMode::AssistedNotes);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"assisted-notes\"}\n"]
        );
    }

    #[test]
    fn start_failed_broadcasts_without_an_active_session() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.start_failed(
            FollowMode::AssistedNotes,
            StartFailureCode::NoInputDevice,
            "no input device",
        );

        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"start_failed\",\"mode\":\"assisted-notes\",\"code\":\"no-input-device\",\"message\":\"no input device\"}\n"]
        );
    }

    #[test]
    fn start_failed_is_ignored_while_the_hub_is_disabled() {
        let hub = FollowStreamHub::default();
        hub.start_failed(
            FollowMode::AssistedNotes,
            StartFailureCode::AudioCaptureFailed,
            "ignored",
        );

        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        assert!(follower.drain().is_empty());
    }

    #[test]
    fn refused_names_the_mode_and_reason() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);
        hub.refused(FollowMode::AssistedNotes, RefusalReason::ModeDisabled);

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"refused\",\"mode\":\"assisted-notes\",\"reason\":\"busy\"}\n",
                "{\"t\":\"refused\",\"mode\":\"assisted-notes\",\"reason\":\"mode-disabled\"}\n",
            ]
        );
    }

    #[test]
    fn refused_is_ignored_while_the_hub_is_disabled() {
        let hub = FollowStreamHub::default();
        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);

        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        assert!(follower.drain().is_empty());
    }

    #[test]
    fn a_refusal_does_not_close_an_active_session() {
        // A refusal is orthogonal to whatever session is or isn't active — it
        // reports a declined command, not a session lifecycle event, so it
        // must never consume `state.active` the way a terminal event does.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);
        hub.finish(session, Some(Speaker::Me), "still open");

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"refused\",\"mode\":\"assisted-notes\",\"reason\":\"busy\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"still open\"}\n",
            ]
        );
    }

    #[test]
    fn refused_preserves_an_active_sessions_undrained_partial_and_it_still_drains_after() {
        // FIX 2 (data loss): a `busy` refusal for one mode used to broadcast
        // through the same path as a real lifecycle event, which cleared
        // BOTH partial slots -- including an unrelated (or the same) active
        // session's latest undrained partial. If no later partial arrived
        // before that session's own `final`, the committed text in the
        // cleared partial was gone from the wire for good. This pins that the
        // partial survives the refusal and is still delivered, in order,
        // ahead of the eventual `final`.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        // Left undrained on purpose: this is the exact state a slow consumer
        // is in when a `refused` for an unrelated command lands.
        hub.partial(StreamSource::Mic, "hello ", "wor");
        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"refused\",\"mode\":\"assisted-notes\",\"reason\":\"busy\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
            ],
            "the refusal must not have discarded the undrained partial, and the partial \
             must still drain, in order, after it"
        );

        // The session's own later final is unaffected: it drains cleanly on
        // its own, proving the preserved partial was actually consumed above
        // rather than lingering to double-count here.
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n"]
        );
    }

    #[test]
    fn start_failed_also_preserves_an_active_sessions_undrained_partial() {
        // Same shape as `refused`'s test above; `start_failed` takes the same
        // control-record path and must have the same property.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        hub.partial(StreamSource::Mic, "hello ", "wor");
        hub.start_failed(
            FollowMode::AssistedNotes,
            StartFailureCode::NoInputDevice,
            "no input device",
        );

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"start_failed\",\"mode\":\"assisted-notes\",\"code\":\"no-input-device\",\"message\":\"no input device\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
            ]
        );
    }

    #[test]
    fn a_real_session_event_still_clears_an_undrained_partial() {
        // Contrast with the two tests above: this property is deliberately
        // *not* extended to real session events. A `final` must never trail
        // a stale `partial` of the text it just finalized, so `no_speech`
        // here must still discard the earlier partial rather than replaying
        // it after its own session has already ended.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        hub.partial(StreamSource::Mic, "hello ", "wor");
        follower.drain();
        hub.no_speech(session);

        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"no_speech\",\"session\":1}\n"],
            "a real terminal event must still clear the undrained partial"
        );
    }

    #[test]
    fn suppress_if_active_ends_the_matching_session_and_silences_its_later_real_events() {
        // Pins the FIX 1 regression: turning a mode's own publication toggle
        // off mid-capture must stop the follower seeing anything further for
        // that session, and must not leave it with a `begin` and no
        // terminal.
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let session = hub.begin(true, FollowMode::AssistedNotes).unwrap();
        follower.drain();

        hub.suppress_if_active(FollowMode::AssistedNotes);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"cancel\",\"session\":1}\n"],
            "the follower must see a terminal, not be left on begin alone"
        );

        // The real capture keeps running server-side and still reports its
        // own partial/finish for this session id -- both must now be silent,
        // exactly like any other stale terminal call (see finish_with).
        hub.partial(StreamSource::Mic, "still recording", "");
        hub.finish(session, Some(Speaker::Me), "late real transcript");
        assert!(
            follower.drain().is_empty(),
            "publication must not resume for a session already ended by its own toggle"
        );
    }

    #[test]
    fn suppress_if_active_ignores_a_session_from_a_different_mode() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        follower.drain();

        // Assisted Notes' own toggle turning off must not touch Meeting's
        // active session.
        hub.suppress_if_active(FollowMode::AssistedNotes);
        assert!(follower.drain().is_empty());

        hub.finish(session, Some(Speaker::Me), "still open");
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"still open\"}\n"]
        );
    }

    #[test]
    fn suppress_if_active_is_a_noop_with_nothing_open() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.suppress_if_active(FollowMode::Meeting);

        assert!(follower.drain().is_empty());
    }

    #[test]
    fn late_attach_receives_active_session_and_both_partial_snapshots() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Meeting), true);
        hub.begin(true, FollowMode::Meeting);
        hub.partial(StreamSource::System, "system", " audio");
        hub.partial(StreamSource::Mic, "hello", " there");

        let (_, initial) = hub.subscribe("0.9.5").unwrap();

        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"]}\n",
                "{\"t\":\"capture_state\",\"phase\":\"recording\",\"mode\":\"meeting\",\"publishing\":true,\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello\",\"tentative\":\" there\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"them\",\"committed\":\"system\",\"tentative\":\" audio\"}\n",
            ]
        );
    }

    #[test]
    fn ninth_subscriber_hits_the_follower_limit() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        for _ in 0..MAX_FOLLOWERS {
            assert!(hub.subscribe("0.9.5").is_ok());
        }

        assert!(matches!(
            hub.subscribe("0.9.5"),
            Err(SubscribeError::LimitReached)
        ));
        assert_eq!(hub.follower_count(), MAX_FOLLOWERS);
    }

    #[test]
    fn subscribing_while_disabled_returns_disabled() {
        let hub = FollowStreamHub::default();

        assert!(matches!(
            hub.subscribe("0.9.5"),
            Err(SubscribeError::Disabled)
        ));
        assert_eq!(hub.follower_count(), 0);
    }

    #[test]
    fn subscribe_errors_have_exact_wire_events() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);

        // Connection-level rejections belong to no session, so they carry
        // `emitted_at` and never `session_elapsed_ms`.
        assert_eq!(
            &*hub.rejection_line(SubscribeError::LimitReached),
            "{\"t\":\"error\",\"code\":\"follower_limit\",\"message\":\"maximum number of follow-stream followers reached\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n"
        );
        assert_eq!(
            &*hub.rejection_line(SubscribeError::Disabled),
            "{\"t\":\"error\",\"code\":\"disabled\",\"message\":\"follow stream is disabled\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n"
        );
    }

    #[test]
    fn every_broadcast_event_carries_the_wall_clock_and_the_session_elapsed_stamp() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();

        // `hello` describes the connection, not a session; `capture_state`
        // reports authoritative idle state at that same instant.
        assert_eq!(
            strings(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"],\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n",
                "{\"t\":\"capture_state\",\"phase\":\"idle\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n",
            ]
        );

        // Drain between events: an undrained partial is cleared by the next
        // lifecycle event, which is the buffer's normal coalescing.
        let mut observed = Vec::new();
        clock.advance(100);
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        observed.extend(follower.drain());
        clock.advance(1112);
        hub.partial(StreamSource::Mic, "hello ", "wor");
        observed.extend(follower.drain());
        clock.advance(638);
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        observed.extend(follower.drain());

        assert_eq!(
            strings(observed),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\",\"emitted_at\":\"2026-08-15T14:03:20.200-07:00\",\"session_elapsed_ms\":0}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\",\"emitted_at\":\"2026-08-15T14:03:21.312-07:00\",\"session_elapsed_ms\":1112}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\",\"emitted_at\":\"2026-08-15T14:03:21.950-07:00\",\"session_elapsed_ms\":1750}\n",
            ]
        );
    }

    #[test]
    fn session_elapsed_restarts_at_zero_for_each_new_session() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        let first = hub.begin(true, FollowMode::Meeting).unwrap();
        clock.advance(5_000);
        hub.finish(first, Some(Speaker::Me), "one");
        clock.advance(60_000);
        let second = hub.begin(true, FollowMode::Meeting).unwrap();
        clock.advance(250);
        hub.finish(second, Some(Speaker::Me), "two");

        let elapsed = elapsed_values(follower.drain());
        assert_eq!(elapsed, [Some(0), Some(5_000), Some(0), Some(250)]);
    }

    #[test]
    fn an_orphaned_sessions_cancel_is_stamped_against_its_own_origin() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.begin(true, FollowMode::Meeting);
        clock.advance(4_000);
        // A second begin cancels the orphan; that cancel closes session 1, so it
        // must be measured from session 1's start, not session 2's.
        hub.begin(true, FollowMode::Meeting);

        let drained = follower.drain();
        assert_eq!(
            events(drained.clone()),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"cancel\",\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":true,\"mode\":\"meeting\"}\n",
            ]
        );
        assert_eq!(elapsed_values(drained), [Some(0), Some(4_000), Some(0)]);
    }

    #[test]
    fn late_attach_replays_the_timestamps_the_events_were_produced_with() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);

        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Meeting), true);
        hub.begin(true, FollowMode::Meeting);
        clock.advance(2_000);
        hub.partial(StreamSource::Mic, "hello", " there");

        // Attach long after the fact: the backlog must still describe when the
        // session began and when that partial landed, not when we arrived.
        clock.advance(30_000);
        let (_, initial) = hub.subscribe("0.9.5").unwrap();

        assert_eq!(
            strings(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"],\"emitted_at\":\"2026-08-15T14:03:52.100-07:00\"}\n",
                "{\"t\":\"capture_state\",\"phase\":\"recording\",\"mode\":\"meeting\",\"publishing\":true,\"session\":1,\"emitted_at\":\"2026-08-15T14:03:52.100-07:00\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\",\"session_elapsed_ms\":0}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello\",\"tentative\":\" there\",\"emitted_at\":\"2026-08-15T14:03:22.100-07:00\",\"session_elapsed_ms\":2000}\n",
            ]
        );
    }

    #[test]
    fn disabled_hub_ignores_every_publisher_method() {
        let hub = FollowStreamHub::default();
        assert_eq!(hub.begin(true, FollowMode::Meeting), None);
        hub.partial(StreamSource::Mic, "ignored", "ignored");
        // The hub is disabled, so no session was ever allocated; any id is
        // equally stale here.
        hub.finish(1, Some(Speaker::Me), "ignored");
        hub.no_speech(1);
        hub.cancel(1);
        hub.error(1, "ignored");
        hub.start_failed(
            FollowMode::AssistedNotes,
            StartFailureCode::AudioCaptureFailed,
            "ignored",
        );
        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);

        assert!(!hub.is_enabled());

        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        // hello + idle-phase capture_state: no active session survived the disabled window above.
        assert_eq!(initial.len(), 2);
        hub.begin(false, FollowMode::Meeting);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":false,\"mode\":\"meeting\"}\n"]
        );
    }

    #[test]
    fn session_ids_increment_and_new_begin_supersedes_the_snapshot() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (existing, _) = hub.subscribe("0.9.5").unwrap();

        hub.set_capture_state(CoordinatorCaptureState::Recording(Mode::Meeting), true);
        hub.begin(true, FollowMode::Meeting);
        assert_eq!(
            events(existing.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n"]
        );
        hub.partial(StreamSource::Mic, "old", " snapshot");
        hub.begin(false, FollowMode::Meeting);

        assert_eq!(
            events(existing.drain()),
            [
                "{\"t\":\"cancel\",\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":false,\"mode\":\"meeting\"}\n",
            ]
        );
        let (_, late_initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(late_initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"]}\n",
                "{\"t\":\"capture_state\",\"phase\":\"recording\",\"mode\":\"meeting\",\"publishing\":true,\"session\":2}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":false,\"mode\":\"meeting\"}\n",
            ]
        );
    }

    #[test]
    fn overflowed_follower_is_evicted_on_the_next_broadcast() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        {
            let mut buffer = follower.buffer.lock().unwrap();
            for _ in 0..=MAX_QUEUED_EVENTS {
                buffer.push_event(Arc::from("event\n"));
            }
        }
        assert!(follower.buffer.lock().unwrap().overflowed());

        hub.begin(true, FollowMode::Meeting);

        assert_eq!(hub.follower_count(), 0);
        assert!(follower.is_evicted());
    }

    #[test]
    fn disabling_and_unsubscribing_mark_followers_evicted() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (disabled_follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.begin(true, FollowMode::Meeting);

        hub.set_enabled(false);

        assert!(disabled_follower.is_evicted());
        assert_eq!(hub.follower_count(), 0);

        hub.set_enabled(true);
        let (unsubscribed_follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.unsubscribe(unsubscribed_follower.id());
        assert!(unsubscribed_follower.is_evicted());
        assert_eq!(hub.follower_count(), 0);
    }

    #[tokio::test]
    async fn wait_wakes_true_when_hub_broadcasts() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let wait_task = tokio::spawn(async move { follower.wait().await });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!wait_task.is_finished());

        hub.begin(true, FollowMode::Meeting);

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), wait_task)
            .await
            .expect("broadcast did not wake follower")
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn wait_wakes_false_when_hub_is_disabled() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        let wait_task = tokio::spawn(async move { follower.wait().await });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!wait_task.is_finished());

        hub.set_enabled(false);

        let result = tokio::time::timeout(std::time::Duration::from_secs(1), wait_task)
            .await
            .expect("disabling the hub did not wake follower")
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn wait_on_already_evicted_follower_returns_false_immediately() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.unsubscribe(follower.id());

        for _ in 0..2 {
            let result =
                tokio::time::timeout(std::time::Duration::from_millis(100), follower.wait())
                    .await
                    .expect("an already-evicted follower parked in wait");
            assert!(!result);
        }
    }

    #[tokio::test]
    async fn notification_between_drain_and_wait_is_not_lost() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();
        assert!(follower.drain().is_empty());

        hub.begin(true, FollowMode::Meeting);

        let result = tokio::time::timeout(std::time::Duration::from_millis(100), follower.wait())
            .await
            .expect("stored notification permit was lost");
        assert!(result);
    }

    #[tokio::test]
    async fn consumer_loop_drains_all_lines_and_terminates_after_eviction() {
        let hub = Arc::new(FollowStreamHub::default());
        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        let written = Arc::new(Mutex::new(initial));

        let consumer_hub = Arc::clone(&hub);
        let consumer_follower = Arc::clone(&follower);
        let consumer_written = Arc::clone(&written);
        let consumer = tokio::spawn(async move {
            loop {
                for line in consumer_follower.drain() {
                    consumer_written.lock().unwrap().push(line);
                }
                if !consumer_follower.wait().await {
                    break;
                }
            }
            for line in consumer_follower.drain() {
                consumer_written.lock().unwrap().push(line);
            }
            consumer_hub.unsubscribe(consumer_follower.id());
        });

        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        // initial already carries hello + capture_state, so each checkpoint below is
        // offset by those two.
        wait_for_line_count(&written, 3).await;
        hub.partial(StreamSource::Mic, "hello ", "wor");
        wait_for_line_count(&written, 4).await;
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        wait_for_line_count(&written, 5).await;
        hub.set_enabled(false);

        tokio::time::timeout(std::time::Duration::from_secs(2), consumer)
            .await
            .expect("consumer loop did not terminate after eviction")
            .unwrap();
        assert_eq!(
            events(written.lock().unwrap().clone()),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"capture-state\",\"refused\",\"refused-publication-disabled\",\"start-failed\",\"start-failed-code\"]}\n",
                "{\"t\":\"capture_state\",\"phase\":\"idle\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }
}
