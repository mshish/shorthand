use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use tokio::sync::Notify;

use crate::managers::transcription::StreamSource;

use super::protocol::{
    FollowEvent, Speaker, ERR_DISABLED, ERR_FOLLOWER_LIMIT, FOLLOW_PROTOCOL_VERSION,
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

pub struct FollowStreamHub {
    enabled: AtomicBool,
    inner: Mutex<HubState>,
}

struct HubState {
    next_session: u64,
    next_follower_id: u64,
    active: Option<ActiveSession>,
    followers: Vec<Arc<Follower>>,
}

struct ActiveSession {
    id: u64,
    streaming: bool,
    /// Latest committed/tentative per speaker, for the late-attach snapshot.
    partials: [Option<(String, String)>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscribeError {
    Disabled,
    LimitReached,
}

impl SubscribeError {
    pub fn to_event(&self) -> FollowEvent {
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
        Self {
            enabled: AtomicBool::new(false),
            inner: Mutex::new(HubState {
                next_session: 1,
                next_follower_id: 1,
                active: None,
                followers: Vec::new(),
            }),
        }
    }
}

impl FollowStreamHub {
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
            Self::broadcast(
                &mut state,
                FollowEvent::Cancel {
                    session: orphaned.id,
                },
                None,
            );
        }

        let session = state.next_session;
        state.next_session += 1;
        state.active = Some(ActiveSession {
            id: session,
            streaming,
            partials: [None, None],
        });
        Self::broadcast(&mut state, FollowEvent::Begin { session, streaming }, None);
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
        let Some(active) = state.active.as_mut() else {
            return;
        };
        active.partials[speaker.index()] = Some((committed.to_string(), tentative.to_string()));
        let session = active.id;

        Self::broadcast(
            &mut state,
            FollowEvent::Partial {
                session,
                speaker,
                committed: committed.to_string(),
                tentative: tentative.to_string(),
            },
            Some(speaker),
        );
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

        let mut backlog = vec![FollowEvent::Hello {
            protocol: FOLLOW_PROTOCOL_VERSION,
            version: app_version.to_string(),
        }
        .to_line()];
        if let Some(active) = &state.active {
            backlog.push(
                FollowEvent::Begin {
                    session: active.id,
                    streaming: active.streaming,
                }
                .to_line(),
            );
            for (index, partial) in active.partials.iter().enumerate() {
                if let Some((committed, tentative)) = partial {
                    let speaker = if index == Speaker::Me.index() {
                        Speaker::Me
                    } else {
                        Speaker::Them
                    };
                    backlog.push(
                        FollowEvent::Partial {
                            session: active.id,
                            speaker,
                            committed: committed.clone(),
                            tentative: tentative.clone(),
                        }
                        .to_line(),
                    );
                }
            }
        }

        state.followers.push(Arc::clone(&follower));
        Ok((follower, backlog))
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
        Self::broadcast(&mut state, make_event(active.id), None);
    }

    fn broadcast(state: &mut HubState, event: FollowEvent, partial_speaker: Option<Speaker>) {
        let line = event.to_line();
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
            strings(follower.drain()),
            [
                "{\"t\":\"begin\",\"session\":1,\"streaming\":false}\n",
                "{\"t\":\"no_speech\",\"session\":1}\n",
            ]
        );

        hub.begin(true);
        hub.cancel();
        hub.finish(Some(Speaker::Me), "late final");
        assert_eq!(
            strings(follower.drain()),
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
            strings(initial),
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
            strings(observed),
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
            strings(initial),
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
        assert_eq!(
            &*SubscribeError::LimitReached.to_event().to_line(),
            "{\"t\":\"error\",\"code\":\"follower_limit\",\"message\":\"maximum number of follow-stream followers reached\"}\n"
        );
        assert_eq!(
            &*SubscribeError::Disabled.to_event().to_line(),
            "{\"t\":\"error\",\"code\":\"disabled\",\"message\":\"follow stream is disabled\"}\n"
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
            strings(follower.drain()),
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
            strings(existing.drain()),
            ["{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n"]
        );
        hub.partial(StreamSource::Mic, "old", " snapshot");
        hub.begin(false);

        assert_eq!(
            strings(existing.drain()),
            [
                "{\"t\":\"cancel\",\"session\":1}\n",
                "{\"t\":\"begin\",\"session\":2,\"streaming\":false}\n",
            ]
        );
        let (_, late_initial) = hub.subscribe("0.9.5").unwrap();
        assert_eq!(
            strings(late_initial),
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
            strings(written.lock().unwrap().clone()),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.9.5\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            ]
        );
    }
}
