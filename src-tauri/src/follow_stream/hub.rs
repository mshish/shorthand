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

use super::protocol::{
    FollowEvent, FollowMode, RefusalReason, Speaker, Stamp, ERR_DISABLED, ERR_FOLLOWER_LIMIT,
    FOLLOW_PROTOCOL_VERSION,
};

/// Capabilities this binary advertises on `hello`. A control flag appears
/// here as the CLI flag minus its `--`; a new record type appears as its own
/// `t` value. One list, referenced from the one place `hello` is built, so
/// the advertised set can never drift from what `subscribe` actually sends.
/// See [`FollowEvent::Hello`]'s own doc comment for what each kind means.
const CAPABILITIES: &[&str] = &[
    "toggle-assisted-notes",
    "start-assisted-notes",
    "stop-assisted-notes",
    "begin-mode",
    "idle",
    "refused",
    "start-failed",
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
    followers: Vec<Arc<Follower>>,
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
    /// second copy of the id resurrected to hold one. A session that gets
    /// superseded between reading its id here and `cancel` below is simply
    /// a no-op, the same tolerance the old cell-based path already had.
    pub fn cancel_active(&self) {
        let Some(session) = self.active_session_id() else {
            return;
        };
        self.cancel(session);
    }

    fn active_session_id(&self) -> Option<u64> {
        if !self.enabled.load(Ordering::Acquire) {
            return None;
        }
        self.inner.lock().unwrap().active.as_ref().map(|a| a.id)
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
    pub fn start_failed(&self, mode: FollowMode, message: &str) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let line = FollowEvent::StartFailed {
            mode,
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
        if let Some(active) = &state.active {
            backlog.push(Arc::clone(&active.begin_line));
            backlog.extend(active.partial_lines.iter().flatten().map(Arc::clone));
        } else {
            // Idle is otherwise only inferable from the absence of a `begin`,
            // which looks identical to "attached before the first capture
            // ever started". A follower that attaches first and reads this
            // record learns real state instead of arming a timer and hoping
            // silence means idle rather than a `begin` still in flight.
            backlog.push(FollowEvent::Idle.to_line(&self.stamp(None)));
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
    fn cancel_active_does_not_touch_a_session_it_did_not_read() {
        // Guards the race `cancel_active`'s own doc comment calls out: if the
        // active session changes between reading its id and cancelling it,
        // the stale id must be dropped by `cancel`'s own session check
        // rather than cancelling whatever replaced it.
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
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}\n",
                "{\"t\":\"idle\"}\n",
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
    fn hello_advertises_begin_mode_so_a_follower_need_not_guess_from_a_version() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (_follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}\n",
                "{\"t\":\"idle\"}\n",
            ]
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

        hub.start_failed(FollowMode::AssistedNotes, "no input device");

        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"start_failed\",\"mode\":\"assisted-notes\",\"message\":\"no input device\"}\n"]
        );
    }

    #[test]
    fn start_failed_is_ignored_while_the_hub_is_disabled() {
        let hub = FollowStreamHub::default();
        hub.start_failed(FollowMode::AssistedNotes, "ignored");

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
        hub.start_failed(FollowMode::AssistedNotes, "no input device");

        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"start_failed\",\"mode\":\"assisted-notes\",\"message\":\"no input device\"}\n",
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
        hub.begin(true, FollowMode::Meeting);
        hub.partial(StreamSource::System, "system", " audio");
        hub.partial(StreamSource::Mic, "hello", " there");

        let (_, initial) = hub.subscribe("0.9.5").unwrap();

        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}\n",
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

        // `hello` describes the connection, not a session; `idle` reports
        // that the connection found no active session, at that same instant.
        assert_eq!(
            strings(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"],\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n",
                "{\"t\":\"idle\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n",
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
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"],\"emitted_at\":\"2026-08-15T14:03:52.100-07:00\"}\n",
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
        hub.start_failed(FollowMode::AssistedNotes, "ignored");
        hub.refused(FollowMode::AssistedNotes, RefusalReason::Busy);

        assert!(!hub.is_enabled());

        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        // hello + idle: no active session survived the disabled window above.
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
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}\n",
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
        // initial already carries hello + idle, so each checkpoint below is
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
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}\n",
                "{\"t\":\"idle\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }
}
