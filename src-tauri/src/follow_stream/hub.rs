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
    FollowEvent, Speaker, Stamp, ERR_DISABLED, ERR_FOLLOWER_LIMIT, FOLLOW_PROTOCOL_VERSION,
};

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

    pub fn begin(&self, streaming: bool) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
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
            Self::broadcast(&mut state, line, None);
        }

        let session = state.next_session;
        state.next_session += 1;

        let started = self.clock.mono();
        let line = FollowEvent::Begin { session, streaming }.to_line(&self.stamp(Some(started)));
        state.active = Some(ActiveSession {
            id: session,
            started,
            begin_line: Arc::clone(&line),
            partial_lines: [None, None],
        });
        Self::broadcast(&mut state, line, None);
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
        Self::broadcast(&mut state, line, Some(speaker));
    }

    pub fn finish(&self, speaker: Option<Speaker>, text: &str) {
        self.finish_with(|session| FollowEvent::Final {
            session,
            speaker,
            text: text.to_string(),
        });
    }

    pub fn no_speech(&self) {
        self.finish_with(|session| FollowEvent::NoSpeech { session });
    }

    pub fn cancel(&self) {
        self.finish_with(|session| FollowEvent::Cancel { session });
    }

    pub fn error(&self, message: &str) {
        self.finish_with(|session| FollowEvent::Error {
            session: Some(session),
            code: None,
            message: message.to_string(),
        });
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
        }
        .to_line(&self.stamp(None))];
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

    fn finish_with(&self, make_event: impl FnOnce(u64) -> FollowEvent) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let mut state = self.inner.lock().unwrap();
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }

        let Some(active) = state.active.take() else {
            return;
        };
        let stamp = self.stamp(Some(active.started));
        let line = make_event(active.id).to_line(&stamp);
        Self::broadcast(&mut state, line, None);
    }

    fn broadcast(state: &mut HubState, line: Arc<str>, partial_speaker: Option<Speaker>) {
        state.followers.retain(|follower| {
            let overflowed = {
                let mut buffer = follower.buffer.lock().unwrap();
                if let Some(speaker) = partial_speaker {
                    buffer.push_partial(speaker, Arc::clone(&line));
                } else {
                    buffer.push_event(Arc::clone(&line));
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
    fn partial_without_an_active_session_emits_nothing() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.partial(StreamSource::Mic, "ignored", "");

        assert!(follower.drain().is_empty());
    }

    #[test]
    fn sessions_emit_at_most_one_terminal_event() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.finish(Some(Speaker::Me), "orphaned final");
        assert!(follower.drain().is_empty());

        hub.begin(false);
        hub.no_speech();
        hub.error("late error");
        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":false}\n",
                "{\"t\":\"no_speech\",\"session\":1}\n",
            ]
        );

        hub.begin(true);
        hub.cancel();
        hub.finish(Some(Speaker::Me), "late final");
        assert_eq!(
            events(follower.drain()),
            [
                "{\"t\":\"begin\",\"session\":2,\"streaming\":true}\n",
                "{\"t\":\"cancel\",\"session\":2}\n",
            ]
        );
    }

    #[test]
    fn follower_observes_begin_partial_and_final_in_order() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(initial),
            ["{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n"]
        );
        let mut observed = Vec::new();
        hub.begin(true);
        observed.extend(follower.drain());
        hub.partial(StreamSource::Mic, "hello ", "wor");
        observed.extend(follower.drain());
        hub.finish(Some(Speaker::Me), "Hello world.");
        observed.extend(follower.drain());

        assert_eq!(
            events(observed),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }

    #[test]
    fn late_attach_receives_active_session_and_both_partial_snapshots() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        hub.begin(true);
        hub.partial(StreamSource::System, "system", " audio");
        hub.partial(StreamSource::Mic, "hello", " there");

        let (_, initial) = hub.subscribe("0.9.5").unwrap();

        assert_eq!(
            events(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
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

        // `hello` describes the connection, not a session.
        assert_eq!(
            strings(initial),
            ["{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\"}\n"]
        );

        // Drain between events: an undrained partial is cleared by the next
        // lifecycle event, which is the buffer's normal coalescing.
        let mut observed = Vec::new();
        clock.advance(100);
        hub.begin(true);
        observed.extend(follower.drain());
        clock.advance(1112);
        hub.partial(StreamSource::Mic, "hello ", "wor");
        observed.extend(follower.drain());
        clock.advance(638);
        hub.finish(Some(Speaker::Me), "Hello world.");
        observed.extend(follower.drain());

        assert_eq!(
            strings(observed),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"emitted_at\":\"2026-08-15T14:03:20.200-07:00\",\"session_elapsed_ms\":0}\n",
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

        hub.begin(true);
        clock.advance(5_000);
        hub.finish(Some(Speaker::Me), "one");
        clock.advance(60_000);
        hub.begin(true);
        clock.advance(250);
        hub.finish(Some(Speaker::Me), "two");

        let elapsed = elapsed_values(follower.drain());
        assert_eq!(elapsed, [Some(0), Some(5_000), Some(0), Some(250)]);
    }

    #[test]
    fn an_orphaned_sessions_cancel_is_stamped_against_its_own_origin() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);
        let (follower, _) = hub.subscribe("0.9.5").unwrap();

        hub.begin(true);
        clock.advance(4_000);
        // A second begin cancels the orphan; that cancel closes session 1, so it
        // must be measured from session 1's start, not session 2's.
        hub.begin(true);

        let drained = follower.drain();
        assert_eq!(
            events(drained.clone()),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
                "{\"t\":\"cancel\",\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":true}\n",
            ]
        );
        assert_eq!(elapsed_values(drained), [Some(0), Some(4_000), Some(0)]);
    }

    #[test]
    fn late_attach_replays_the_timestamps_the_events_were_produced_with() {
        let clock = TestClock::new();
        let hub = enabled_hub(&clock);

        hub.begin(true);
        clock.advance(2_000);
        hub.partial(StreamSource::Mic, "hello", " there");

        // Attach long after the fact: the backlog must still describe when the
        // session began and when that partial landed, not when we arrived.
        clock.advance(30_000);
        let (_, initial) = hub.subscribe("0.9.5").unwrap();

        assert_eq!(
            strings(initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\",\"emitted_at\":\"2026-08-15T14:03:52.100-07:00\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"emitted_at\":\"2026-08-15T14:03:20.100-07:00\",\"session_elapsed_ms\":0}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello\",\"tentative\":\" there\",\"emitted_at\":\"2026-08-15T14:03:22.100-07:00\",\"session_elapsed_ms\":2000}\n",
            ]
        );
    }

    #[test]
    fn disabled_hub_ignores_every_publisher_method() {
        let hub = FollowStreamHub::default();
        hub.begin(true);
        hub.partial(StreamSource::Mic, "ignored", "ignored");
        hub.finish(Some(Speaker::Me), "ignored");
        hub.no_speech();
        hub.cancel();
        hub.error("ignored");

        assert!(!hub.is_enabled());

        hub.set_enabled(true);
        let (follower, initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(initial.len(), 1);
        hub.begin(false);
        assert_eq!(
            events(follower.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":false}\n"]
        );
    }

    #[test]
    fn session_ids_increment_and_new_begin_supersedes_the_snapshot() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (existing, _) = hub.subscribe("0.9.5").unwrap();

        hub.begin(true);
        assert_eq!(
            events(existing.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n"]
        );
        hub.partial(StreamSource::Mic, "old", " snapshot");
        hub.begin(false);

        assert_eq!(
            events(existing.drain()),
            [
                "{\"t\":\"cancel\",\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":false}\n",
            ]
        );
        let (_, late_initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            events(late_initial),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":false}\n",
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

        hub.begin(true);

        assert_eq!(hub.follower_count(), 0);
        assert!(follower.is_evicted());
    }

    #[test]
    fn disabling_and_unsubscribing_mark_followers_evicted() {
        let hub = FollowStreamHub::default();
        hub.set_enabled(true);
        let (disabled_follower, _) = hub.subscribe("0.9.5").unwrap();
        hub.begin(true);

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

        hub.begin(true);

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

        hub.begin(true);

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

        hub.begin(true);
        wait_for_line_count(&written, 2).await;
        hub.partial(StreamSource::Mic, "hello ", "wor");
        wait_for_line_count(&written, 3).await;
        hub.finish(Some(Speaker::Me), "Hello world.");
        wait_for_line_count(&written, 4).await;
        hub.set_enabled(false);

        tokio::time::timeout(std::time::Duration::from_secs(2), consumer)
            .await
            .expect("consumer loop did not terminate after eviction")
            .unwrap();
        assert_eq!(
            events(written.lock().unwrap().clone()),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }
}
