//! Shorthand: two live transcription lanes, the microphone and system audio.
//!
//! A Meeting streams each lane through its own [`TranscriptionManager`] (and
//! so its own transcription worker), then merges their committed text into
//! labelled turns. Everything here is the fork's; upstream transcribes the
//! microphone only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use super::{post_process_transcription_text, TranscriptionManager};
use crate::audio_toolkit::OutputLanguageEvidence;
use crate::managers::model::ModelManager;
use crate::settings::AppSettings;

const STREAM_TURN_GAP: Duration = Duration::from_millis(1500);
const SUBSTANTIAL_TURN_CHARS: usize = 20;

/// Identifies the capture lane that produced a streaming event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum StreamSource {
    Mic,
    System,
}

#[derive(Clone, Debug)]
struct StreamCommit {
    source: StreamSource,
    arrived_at: Instant,
    text: String,
}

#[derive(Default)]
struct StreamTranscriptState {
    active: bool,
    mic_committed: String,
    system_committed: String,
    commits: Vec<StreamCommit>,
}

/// Session-scoped ordered log of append-only committed streaming text.
///
/// Decoder timing is intentionally not used here: VAD removes a different
/// amount of silence from each lane, so decoder counters are not comparable.
/// Capturing `Instant::now()` when each commit reaches this shared state keeps
/// the two sources ordered by wall-clock arrival instead.
#[derive(Default)]
pub struct StreamTranscriptMerger {
    state: Mutex<StreamTranscriptState>,
}

impl StreamTranscriptMerger {
    pub fn begin_session(&self, dual_lane: bool) {
        *self.state.lock().unwrap() = StreamTranscriptState {
            active: dual_lane,
            ..Default::default()
        };
    }

    pub fn cancel_session(&self) {
        *self.state.lock().unwrap() = StreamTranscriptState::default();
    }

    pub(super) fn observe_committed(&self, source: StreamSource, committed: &str) {
        self.observe_committed_at(source, Instant::now(), committed);
    }

    fn observe_committed_at(&self, source: StreamSource, arrived_at: Instant, committed: &str) {
        let mut state = self.state.lock().unwrap();
        if !state.active {
            return;
        }
        let previous = match source {
            StreamSource::Mic => &mut state.mic_committed,
            StreamSource::System => &mut state.system_committed,
        };

        let Some(delta) = committed.strip_prefix(previous.as_str()) else {
            warn!(
                "stream committed prefix rewrote for {:?}; dropping non-append-only update",
                source
            );
            *previous = committed.to_string();
            return;
        };
        if delta.is_empty() {
            return;
        }

        *previous = committed.to_string();
        state.commits.push(StreamCommit {
            source,
            arrived_at,
            text: delta.to_string(),
        });
    }

    /// Render the ordered commit log. Mic-only runs deliberately return the
    /// primary manager's final text unchanged, preserving existing output bytes.
    pub fn render(
        &self,
        mic_final: &str,
        mic_stream: Option<&FinalizedStreamResult>,
        system_stream: Option<&FinalizedStreamResult>,
        settings: &AppSettings,
        mic_label: &str,
        system_label: &str,
    ) -> String {
        let state = std::mem::take(&mut *self.state.lock().unwrap());
        let Some(system_stream) = filter_dual_speaker(system_stream) else {
            return mic_final.to_string();
        };

        let mut commits = state.commits;
        if let Some(mic_stream) = mic_stream {
            append_missing_final_tail(&mut commits, StreamSource::Mic, &mic_stream.raw);
        }
        append_missing_final_tail(&mut commits, StreamSource::System, &system_stream.raw);
        let mut turns = coalesce_stream_commits(&commits);
        if !mic_final.trim().is_empty()
            && !turns.iter().any(|turn| turn.source == StreamSource::Mic)
        {
            turns.insert(0, StreamTurn::fallback(StreamSource::Mic, mic_final));
        }
        if !turns.iter().any(|turn| turn.source == StreamSource::System) {
            turns.push(StreamTurn::fallback(
                StreamSource::System,
                &system_stream.filtered,
            ));
        }

        let mut used_batch_mic = false;
        turns
            .into_iter()
            .filter_map(|turn| {
                let text = match turn.source {
                    StreamSource::Mic => match mic_stream {
                        Some(context) => context.post_process_turn(turn.text, settings),
                        None if !used_batch_mic => {
                            used_batch_mic = true;
                            mic_final.to_string()
                        }
                        None => return None,
                    },
                    StreamSource::System => system_stream.post_process_turn(turn.text, settings),
                };
                let text = text.trim();
                if text.is_empty() {
                    None
                } else {
                    let label = match turn.source {
                        StreamSource::Mic => mic_label,
                        StreamSource::System => system_label,
                    };
                    Some(format!("{label}: {text}"))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug)]
struct StreamTurn {
    source: StreamSource,
    last_arrival: Instant,
    text: String,
}

impl StreamTurn {
    fn fallback(source: StreamSource, text: &str) -> Self {
        let now = Instant::now();
        Self {
            source,
            last_arrival: now,
            text: text.to_string(),
        }
    }
}

fn is_substantial_commit(text: &str) -> bool {
    text.trim().chars().count() >= SUBSTANTIAL_TURN_CHARS
}

fn append_missing_final_tail(
    commits: &mut Vec<StreamCommit>,
    source: StreamSource,
    final_raw: &str,
) {
    let observed = commits
        .iter()
        .filter(|commit| commit.source == source)
        .map(|commit| commit.text.as_str())
        .collect::<String>();
    if let Some(tail) = final_raw.strip_prefix(&observed) {
        if !tail.is_empty() {
            commits.push(StreamCommit {
                source,
                arrived_at: Instant::now(),
                text: tail.to_string(),
            });
        }
    } else if observed != final_raw {
        warn!(
            "final stream text did not extend observed {:?} commits",
            source
        );
    }
}

fn coalesce_stream_commits(commits: &[StreamCommit]) -> Vec<StreamTurn> {
    let mut turns: Vec<StreamTurn> = Vec::new();
    for commit in commits {
        if let Some(last) = turns.last_mut() {
            if last.source == commit.source {
                last.text.push_str(&commit.text);
                last.last_arrival = commit.arrived_at;
                continue;
            }
        }

        let previous_quiet = turns.last().is_none_or(|turn| {
            commit.arrived_at.duration_since(turn.last_arrival) >= STREAM_TURN_GAP
        });
        if !previous_quiet && !is_substantial_commit(&commit.text) {
            if let Some(existing) = turns.iter_mut().rev().find(|turn| {
                turn.source == commit.source
                    && commit.arrived_at.duration_since(turn.last_arrival) < STREAM_TURN_GAP
            }) {
                existing.text.push_str(&commit.text);
                existing.last_arrival = commit.arrived_at;
                continue;
            }
        }

        turns.push(StreamTurn {
            source: commit.source,
            last_arrival: commit.arrived_at,
            text: commit.text.clone(),
        });
    }
    turns
}

pub struct FinalizedStreamResult {
    pub(super) raw: String,
    pub filtered: String,
    pub(super) output_language: OutputLanguageEvidence,
    pub(super) supported_languages: Vec<String>,
    /// Custom words went to the model as vocabulary, so fuzzy correction is
    /// skipped for this lane's turns as it was for its whole text.
    pub(super) custom_words_prompted: bool,
}

impl FinalizedStreamResult {
    fn post_process_turn(&self, raw: String, settings: &AppSettings) -> String {
        post_process_transcription_text(
            raw,
            settings,
            self.custom_words_prompted,
            &self.output_language,
            &self.supported_languages,
        )
    }
}

fn filter_dual_speaker(
    system_stream: Option<&FinalizedStreamResult>,
) -> Option<&FinalizedStreamResult> {
    system_stream.filter(|result| !result.filtered.trim().is_empty())
}

pub fn has_dual_speaker_output(system_stream: Option<&FinalizedStreamResult>) -> bool {
    filter_dual_speaker(system_stream).is_some()
}

/// Type-keyed Tauri state for the system-audio transcription lane.
///
/// The manager exists ahead of a capture so either active mode can opt into
/// the lane without racing a hotkey. It does not load a model until streaming
/// actually starts.
pub struct SystemAudioTranscription(pub Mutex<Option<Arc<TranscriptionManager>>>);

struct CapturedHandles<T>(Mutex<Vec<Arc<T>>>);

impl<T> Default for CapturedHandles<T> {
    fn default() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}

impl<T> CapturedHandles<T> {
    fn capture(&self, handles: Vec<Arc<T>>) {
        *self.0.lock().unwrap() = handles;
    }

    fn take(&self) -> Vec<Arc<T>> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

#[derive(Default)]
pub struct ActiveStreamManagers(CapturedHandles<TranscriptionManager>);

impl ActiveStreamManagers {
    pub fn capture(&self, managers: Vec<Arc<TranscriptionManager>>) {
        self.0.capture(managers);
    }

    pub fn take(&self) -> Vec<Arc<TranscriptionManager>> {
        self.0.take()
    }
}

fn dual_lane_enabled(system_audio_enabled: bool, model_supports_streaming: bool) -> bool {
    system_audio_enabled && model_supports_streaming
}

pub fn system_audio_streaming_enabled(app: &AppHandle) -> bool {
    let settings = crate::shorthand::dictation::resolve_settings(app);
    let supports_streaming = app
        .try_state::<Arc<ModelManager>>()
        .and_then(|manager| manager.get_model_info(&settings.selected_model))
        .is_some_and(|model| model.supports_streaming);
    dual_lane_enabled(settings.system_audio_enabled, supports_streaming)
}

pub fn cancel_active_streams(app: &AppHandle) {
    let managers = app
        .try_state::<ActiveStreamManagers>()
        .map(|active| active.take())
        .unwrap_or_default();
    for manager in managers {
        manager.cancel_stream();
    }
    if let Some(merger) = app.try_state::<StreamTranscriptMerger>() {
        merger.cancel_session();
    }
}

/// Return the transcription managers actually in service. Headless mode and
/// the disabled system-audio path deliberately expose only the primary lane.
pub fn transcription_managers(app: &AppHandle) -> Vec<Arc<TranscriptionManager>> {
    let mut managers = Vec::new();
    if let Some(primary) = app.try_state::<Arc<TranscriptionManager>>() {
        managers.push(Arc::clone(&primary));
    }
    if system_audio_streaming_enabled(app) {
        if let Some(system) = app.try_state::<SystemAudioTranscription>() {
            if let Some(manager) = system.0.lock().unwrap().as_ref() {
                managers.push(Arc::clone(manager));
            }
        }
    }
    managers
}

/// Stops the idle watcher of every transcription manager that exists, whether
/// or not its lane is in service. Called from the `RunEvent::Exit` handler.
///
/// Tauri clears the resource table right after that handler returns (see
/// `App::run`), which frees the settings store. A watcher tick that lands
/// after that point panics in `get_settings`'s `expect("Failed to
/// initialize store")`, and `Drop` never stops the watcher at exit because
/// the watcher holds its own clone of the manager. Joining here, while the
/// store still exists, guarantees no tick can outlive it.
pub fn stop_idle_watchers(app: &AppHandle) {
    if let Some(primary) = app.try_state::<Arc<TranscriptionManager>>() {
        primary.stop_idle_watcher();
    }
    if let Some(system) = app.try_state::<SystemAudioTranscription>() {
        let manager = system.0.lock().ok().and_then(|slot| slot.clone());
        if let Some(manager) = manager {
            manager.stop_idle_watcher();
        }
    }
}

/// Signals a parked idle watcher to stop, wakes it, and waits for it to exit.
/// A no-op once the watcher is gone, so it is safe to call more than once.
pub(super) fn stop_watcher_thread(
    shutdown_signal: &AtomicBool,
    watcher_handle: &Mutex<Option<thread::JoinHandle<()>>>,
) {
    shutdown_signal.store(true, Ordering::Relaxed);
    let handle = match watcher_handle.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    if let Some(handle) = handle {
        handle.thread().unpark();
        if let Err(e) = handle.join() {
            warn!("Failed to join idle watcher thread: {:?}", e);
        }
    }
}

fn model_ids_match<I>(model_ids: I) -> bool
where
    I: IntoIterator<Item = Option<String>>,
{
    let mut model_ids = model_ids.into_iter();
    let Some(expected) = model_ids.next() else {
        return true;
    };
    model_ids.all(|model_id| model_id == expected)
}

pub(super) fn claim_model_load(
    is_loading: &Mutex<bool>,
    current_model_id: &Mutex<Option<String>>,
    reload_model_on_next_use: &AtomicBool,
    selected_model: &str,
) -> Option<bool> {
    let mut loading = is_loading.lock().unwrap();
    if *loading {
        return None;
    }
    let reload_pending = reload_model_on_next_use.load(Ordering::Acquire);
    if !reload_pending && current_model_id.lock().unwrap().as_deref() == Some(selected_model) {
        return None;
    }
    *loading = true;
    Some(reload_pending)
}

pub fn transcription_models_are_coherent(app: &AppHandle) -> bool {
    model_ids_match(
        transcription_managers(app)
            .into_iter()
            .map(|manager| manager.get_current_model()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    /// A thread shaped like the idle watcher's loop: parks for 10 s between
    /// ticks and exits once the signal is set.
    fn spawn_parked_watcher(
        shutdown_signal: Arc<AtomicBool>,
        ticks: Arc<AtomicU64>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            while !shutdown_signal.load(Ordering::Relaxed) {
                thread::park_timeout(Duration::from_secs(10));
                if shutdown_signal.load(Ordering::Relaxed) {
                    break;
                }
                ticks.fetch_add(1, Ordering::Relaxed);
            }
        })
    }

    #[test]
    fn stopping_the_idle_watcher_wakes_it_instead_of_waiting_out_its_sleep() {
        let shutdown_signal = Arc::new(AtomicBool::new(false));
        let ticks = Arc::new(AtomicU64::new(0));
        let watcher_handle = Mutex::new(Some(spawn_parked_watcher(
            Arc::clone(&shutdown_signal),
            Arc::clone(&ticks),
        )));
        // Let it reach the park.
        thread::sleep(Duration::from_millis(50));

        let started = Instant::now();
        stop_watcher_thread(&shutdown_signal, &watcher_handle);

        assert!(
            started.elapsed() < Duration::from_secs(2),
            "stop must not wait out the 10 s park (took {:?})",
            started.elapsed()
        );
        assert!(watcher_handle.lock().unwrap().is_none());
        // Joined: it can never tick (and so never read settings) again.
        assert_eq!(ticks.load(Ordering::Relaxed), 0);

        // Exit calls it, and Drop may follow: a second stop is a no-op.
        stop_watcher_thread(&shutdown_signal, &watcher_handle);
    }

    fn languages(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_string()).collect()
    }

    fn finalized(raw: &str, settings: &AppSettings) -> FinalizedStreamResult {
        let output_language = OutputLanguageEvidence::UserSelected("en".to_string());
        let supported_languages = languages(&["en"]);
        FinalizedStreamResult {
            raw: raw.to_string(),
            filtered: post_process_transcription_text(
                raw.to_string(),
                settings,
                false,
                &output_language,
                &supported_languages,
            ),
            output_language,
            supported_languages,
            custom_words_prompted: false,
        }
    }

    #[test]
    fn dual_speaker_predicate_matches_render_system_branch() {
        let settings = AppSettings::default();
        let whitespace_only = finalized(" \t\n", &settings);
        let contributing = finalized("system speech", &settings);

        assert!(!has_dual_speaker_output(None));
        assert!(!has_dual_speaker_output(Some(&whitespace_only)));
        assert!(has_dual_speaker_output(Some(&contributing)));

        for stream in [None, Some(&whitespace_only), Some(&contributing)] {
            assert_eq!(
                filter_dual_speaker(stream).is_some(),
                has_dual_speaker_output(stream)
            );
        }
    }

    #[test]
    fn merge_appends_only_new_committed_delta() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        merger.observe_committed(StreamSource::Mic, "hello");
        merger.observe_committed(StreamSource::Mic, "hello world");

        let state = merger.state.lock().unwrap();
        assert_eq!(state.commits.len(), 2);
        assert_eq!(state.commits[0].text, "hello");
        assert_eq!(state.commits[1].text, " world");
    }

    #[test]
    fn non_append_only_committed_rewrite_is_dropped_instead_of_duplicated() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        merger.observe_committed(StreamSource::Mic, "original prefix");
        merger.observe_committed(StreamSource::Mic, "rewritten prefix");

        let state = merger.state.lock().unwrap();
        assert_eq!(state.commits.len(), 1);
        assert_eq!(state.commits[0].text, "original prefix");
        assert_eq!(state.mic_committed, "rewritten prefix");
    }

    #[test]
    fn cancel_deactivates_merge_until_a_new_dual_session_begins() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        merger.observe_committed(StreamSource::Mic, "first session");
        merger.cancel_session();
        merger.observe_committed(StreamSource::Mic, "ignored after cancel");
        assert!(merger.state.lock().unwrap().commits.is_empty());

        merger.begin_session(true);
        merger.observe_committed(StreamSource::Mic, "second session");

        let state = merger.state.lock().unwrap();
        assert_eq!(state.mic_committed, "second session");
        assert_eq!(state.commits.len(), 1);
        assert_eq!(state.commits[0].text, "second session");
    }

    #[test]
    fn mic_only_merge_is_byte_identical_to_primary_text() {
        let merger = StreamTranscriptMerger::default();
        let primary = "  Existing mic output. \n";

        assert_eq!(
            merger
                .render(
                    primary,
                    None,
                    None,
                    &AppSettings::default(),
                    "You",
                    "System",
                )
                .as_bytes(),
            primary.as_bytes()
        );
    }

    #[test]
    fn dual_mode_post_processes_each_raw_turn_before_labelling() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        let settings = AppSettings {
            custom_words: vec!["Handy".to_string()],
            custom_filler_words: Some(vec!["um".to_string()]),
            filler_word_removal_enabled: true,
            ..AppSettings::default()
        };
        merger.observe_committed(StreamSource::Mic, "um handy works");
        merger.observe_committed(StreamSource::System, "yes it does");
        let mic = finalized("um handy works", &settings);
        let system = finalized("yes it does", &settings);

        let rendered = merger.render(
            &mic.filtered,
            Some(&mic),
            Some(&system),
            &settings,
            "You",
            "System",
        );
        println!("dual_mode_rendered={rendered:?}");
        assert_eq!(rendered, "You: Handy works\nSystem: yes it does");
    }

    #[test]
    fn finalized_context_supplies_a_tail_not_seen_in_incremental_events() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        merger.observe_committed(StreamSource::Mic, "hello");
        merger.observe_committed(StreamSource::System, "sending");
        let settings = AppSettings::default();
        let mic = finalized("hello", &settings);
        let system = finalized("sending now", &settings);

        let rendered = merger.render(
            &mic.filtered,
            Some(&mic),
            Some(&system),
            &settings,
            "You",
            "System",
        );
        assert!(rendered.contains("System: sending now"));
    }

    #[test]
    fn rapidly_alternating_small_commits_coalesce_into_readable_lane_turns() {
        let merger = StreamTranscriptMerger::default();
        merger.begin_session(true);
        let start = Instant::now();
        let mut mic = String::new();
        let mut system = String::new();
        for (index, (source, delta)) in [
            (StreamSource::Mic, "So "),
            (StreamSource::System, "I "),
            (StreamSource::Mic, "that "),
            (StreamSource::System, "think "),
            (StreamSource::Mic, "was "),
            (StreamSource::System, "the "),
            (StreamSource::Mic, "the point."),
            (StreamSource::System, "same."),
        ]
        .into_iter()
        .enumerate()
        {
            let committed = match source {
                StreamSource::Mic => &mut mic,
                StreamSource::System => &mut system,
            };
            committed.push_str(delta);
            merger.observe_committed_at(
                source,
                start + Duration::from_millis(index as u64 * 100),
                committed,
            );
        }
        let settings = AppSettings::default();
        let mic_final = finalized(&mic, &settings);
        let system_final = finalized(&system, &settings);
        let rendered = merger.render(
            &mic_final.filtered,
            Some(&mic_final),
            Some(&system_final),
            &settings,
            "You",
            "System",
        );
        assert_eq!(
            rendered,
            "You: So that was the point.\nSystem: I think the same."
        );
    }

    #[test]
    fn stale_system_audio_setting_does_not_enable_dual_lane_for_batch_model() {
        assert!(!dual_lane_enabled(true, false));
        assert!(dual_lane_enabled(true, true));
        assert!(!dual_lane_enabled(false, true));
    }

    #[test]
    fn captured_session_handle_survives_optional_manager_slot_being_cleared() {
        let captured = CapturedHandles::default();
        let manager = Arc::new(7_u8);
        let mut optional_slot = Some(Arc::clone(&manager));
        captured.capture(vec![Arc::clone(&manager)]);

        optional_slot.take();
        drop(manager);

        let handles = captured.take();
        assert_eq!(*handles[0], 7);
        assert_eq!(Arc::strong_count(&handles[0]), 1);
    }

    #[test]
    fn both_manager_load_gates_claim_a_changed_selected_model() {
        let primary_loading = Mutex::new(false);
        let primary_model = Mutex::new(Some("old-model".to_string()));
        let primary_reload = AtomicBool::new(false);
        let system_loading = Mutex::new(false);
        let system_model = Mutex::new(Some("old-model".to_string()));
        let system_reload = AtomicBool::new(false);

        let claims = [
            claim_model_load(
                &primary_loading,
                &primary_model,
                &primary_reload,
                "selected-model",
            ),
            claim_model_load(
                &system_loading,
                &system_model,
                &system_reload,
                "selected-model",
            ),
        ];
        assert_eq!(claims, [Some(false), Some(false)]);
        assert!(*primary_loading.lock().unwrap());
        assert!(*system_loading.lock().unwrap());
    }
}
