use std::{
    io::Error,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio_toolkit::{
    audio::{device_display_name, AudioVisualiser, FrameResampler},
    constants,
    vad::{self, VadFrame},
    VoiceActivityDetector,
};

enum Cmd {
    /// Begin capturing. Carries the send timestamp so the consumer can log how
    /// long the command sat in the channel, plus a one-shot first-sample acknowledgement.
    Start(VadPolicy, Instant, mpsc::Sender<()>),
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

// Two seconds of ring capacity absorbs consumer stalls without adding latency
// during normal 10 ms drains.
const AUDIO_RING_SECONDS: usize = 2;
const CONSUMER_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_DRAIN_CHUNK: Duration = Duration::from_millis(50);
const PAUSE_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// Atomics shared by the callback and consumer; audio uses a wait-free SPSC ring.
/// The callback must remain allocation-, lock-, logging-, and blocking-free.
#[derive(Default)]
struct CaptureTransportState {
    pause_requested: AtomicBool,
    /// Set after forwarding a pause's boundary block; subsequent callbacks
    /// remain silent until the consumer clears the request.
    pause_acknowledged: AtomicBool,
    overrun_samples: AtomicU64,
}

// ---- Shorthand: system-audio (loopback) lane ------------------------------ //
// The microphone uses upstream's real-time-safe ring above. The loopback lane
// keeps its own channel transport: a pump thread paces it and supplies the
// end-of-stream sentinel its consumer (`run_system_consumer`) drains to at stop.

enum LoopbackPumpCmd {
    /// Begin pacing the session `start()` just opened, identified by the
    /// generation loopback blocks are tagged with.
    StartSession(u64),
    /// Forward the session's remaining audio, then exactly one `EndOfStream`.
    /// Sent once per `Cmd::Stop` the system consumer receives, including
    /// outside a session, so the consumer can pair every marker with a stop.
    EndSession,
    /// Exit, even though the recorder still holds a sender. The capture worker
    /// sends this when the microphone consumer has returned.
    Shutdown,
}

enum AudioChunk {
    Samples(Vec<f32>),
    EndOfStream,
}

/// How long the loopback pump waits for real system audio before emitting an
/// equivalent run of silence.
///
/// This exists because `run_system_consumer` is driven entirely by its sample channel:
/// it only polls `cmd_rx` after a chunk arrives. A microphone satisfies that
/// implicitly — an open capture endpoint keeps delivering near-zero buffers every
/// device period — but system-audio loopback can go completely silent on an idle render
/// endpoint. Without a pump, `Cmd::Stop` would never be observed and `stop()`
/// would block forever.
const LOOPBACK_PUMP_INTERVAL_MS: usize = 10;

const CONSUMER_STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the system consumer waits for the pump's `EndOfStream` at stop.
const SYSTEM_STOP_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the pump waits, at the end of a session, for a loopback callback
/// that was already running when `stop()` was called. A callback never
/// blocks, so this bounds only a preempted audio thread.
const LOOPBACK_CALLBACK_SETTLE_TIMEOUT: Duration = Duration::from_millis(100);

struct LoopbackChunk {
    samples: Vec<f32>,
    sample_rate: u32,
    session_generation: u64,
}

/// A device cpal can open as a system-audio loopback input stream.
#[derive(Clone)]
pub struct SystemAudioCapture {
    pub device: Device,
}

// ---- end Shorthand -------------------------------------------------------- //

/// How 16 kHz mono frames should be filtered for one recording session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadPolicy {
    /// Bypass VAD and forward every frame.
    Disabled,
    /// Current offline-tuned VAD profile.
    Offline,
    /// VAD profile with a longer post-speech tail for streaming-capable models.
    Streaming,
}

/// A single VAD engine plus the two hangover-tail lengths its smoothing wrapper
/// should use. The offline and streaming policies are never active
/// concurrently, so one detector is reconfigured per session (see `Cmd::Start`)
/// rather than kept as two resident engines.
#[derive(Clone)]
struct VadConfig {
    detector: Arc<Mutex<Box<dyn vad::VoiceActivityDetector>>>,
    frame_samples: usize,
    offline_hangover_frames: usize,
    streaming_hangover_frames: usize,
}

impl VadConfig {
    /// Post-speech hangover tail (in backend-sized frames) for the given policy.
    /// `Disabled` never reaches the detector, so it maps to the offline value.
    fn hangover_for(&self, policy: VadPolicy) -> usize {
        match policy {
            VadPolicy::Streaming => self.streaming_hangover_frames,
            VadPolicy::Offline | VadPolicy::Disabled => self.offline_hangover_frames,
        }
    }
}

/// Callback invoked with each 16 kHz mono frame that passes the active capture
/// policy while recording. Used to feed a live streaming transcription as audio arrives.
pub type AudioFrameCallback = Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;
pub type LevelCallback = Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>;

/// The independently processed microphone and system-audio lanes returned by a
/// recording stop. Persistence continues to use `microphone` only.
pub struct RecordedAudio {
    pub microphone: Vec<f32>,
    pub system: Vec<f32>,
}

/// The system-audio portion of the most recent recorder open.
///
/// `Unavailable` is deliberately a normal degraded state: microphone capture
/// continues when loopback cannot open. Keeping CPAL's error text here lets a
/// platform-specific caller distinguish a denied permission from a missing
/// endpoint without making that choice in the realtime capture path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoopbackOpenOutcome {
    NotRequested,
    Active,
    Unavailable { error: String },
}

impl LoopbackOpenOutcome {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    system_cmd_tx: Option<mpsc::Sender<Cmd>>,
    loopback_pump_tx: Option<mpsc::Sender<LoopbackPumpCmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<VadConfig>,
    system_vad: Option<VadConfig>,
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
    system_audio_cb: Option<AudioFrameCallback>,
    /// Whether the most recent `open()` brought up the loopback stream.
    system_audio_active: bool,
    loopback_open_outcome: LoopbackOpenOutcome,
    /// Which input channel to use. None = average all (original behavior).
    selected_channel: Option<usize>,
    /// Preferred stream config cached per device name. The two HAL property
    /// queries in `get_preferred_config` cost ~40-85ms per open (worse on
    /// USB/Bluetooth), which lands on the keypress->capture path in on-demand
    /// mode. Keyed by name so a system-default change misses naturally;
    /// cleared whenever an open fails so a stale rate/format self-heals on the
    /// caller's retry.
    config_cache: Arc<Mutex<Option<(String, cpal::SupportedStreamConfig)>>>,
    system_audio_session: Arc<SystemAudioSession>,
    /// Set by cpal when the active input stream can no longer capture.
    stream_error: Arc<AtomicBool>,
    /// Set when the system-audio lane can no longer capture: its loopback
    /// stream reported a fatal error, or one of its threads stopped taking
    /// commands. Like `stream_error`, it makes `needs_reopen()` rebuild the
    /// recorder before the next recording.
    system_audio_error: Arc<AtomicBool>,
}

#[derive(Default)]
struct SystemAudioSession {
    active: AtomicBool,
    generation: AtomicU64,
    stale_samples: AtomicUsize,
    /// True while a loopback callback is between reading `active` and handing
    /// its block to the pump. The pump waits for it to clear before draining
    /// a stopped session, so the block in flight at `stop()` is kept.
    callback_busy: AtomicBool,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            system_cmd_tx: None,
            loopback_pump_tx: None,
            worker_handle: None,
            vad: None,
            system_vad: None,
            level_cb: None,
            audio_cb: None,
            system_audio_cb: None,
            system_audio_active: false,
            loopback_open_outcome: LoopbackOpenOutcome::NotRequested,
            selected_channel: None,
            config_cache: Arc::new(Mutex::new(None)),
            system_audio_session: Arc::new(SystemAudioSession::default()),
            stream_error: Arc::new(AtomicBool::new(false)),
            system_audio_error: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Attach a single VAD engine, reconfigured per session for the offline vs
    /// streaming hangover tail. The two policies are mutually exclusive within a
    /// recording, so one engine covers both instead of two resident instances.
    pub fn with_vad(
        mut self,
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        let frame_samples = detector.frame_samples();
        assert!(frame_samples > 0, "VAD frame size must be non-zero");
        self.vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
            frame_samples,
            offline_hangover_frames,
            streaming_hangover_frames,
        });
        self
    }

    /// Attach an independent detector for the system-audio consumer. It must not
    /// share Silero recurrent state or smoothing counters with the microphone.
    pub fn with_system_vad(
        mut self,
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        let frame_samples = detector.frame_samples();
        assert!(frame_samples > 0, "VAD frame size must be non-zero");
        self.system_vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
            frame_samples,
            offline_hangover_frames,
            streaming_hangover_frames,
        });
        self
    }

    pub fn with_level_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    /// Register a callback that receives real-time 16 kHz frames after the active
    /// VAD policy has been applied. Frames arrive in real time, in order, on the
    /// recorder's consumer thread — keep the callback cheap (e.g. forward to a
    /// channel) so it never stalls capture.
    pub fn with_audio_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        self.audio_cb = Some(Arc::new(cb));
        self
    }

    pub fn with_system_audio_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(&[f32]) + Send + Sync + 'static,
    {
        self.system_audio_cb = Some(Arc::new(cb));
        self
    }

    pub fn with_selected_channel(mut self, channel: Option<u16>) -> Self {
        self.set_selected_channel(channel);
        self
    }

    pub fn set_selected_channel(&mut self, channel: Option<u16>) {
        self.selected_channel = channel.map(usize::from);
    }

    pub fn open(
        &mut self,
        device: Option<Device>,
        system_audio: Option<SystemAudioCapture>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            if !self.needs_reopen() {
                return Ok(self.system_audio_active); // already open
            }
            log::warn!("Capture stream failed; rebuilding microphone stream");
            self.close()?;
        }

        self.stream_error.store(false, Ordering::Relaxed);
        self.system_audio_error.store(false, Ordering::Relaxed);

        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (system_sample_tx, system_sample_rx) = mpsc::channel::<AudioChunk>();
        let (system_cmd_tx, system_cmd_rx) = mpsc::channel::<Cmd>();
        let system_cmd_tx_for_worker = system_cmd_tx.clone();
        let (loopback_pump_tx, loopback_pump_rx) = mpsc::channel::<LoopbackPumpCmd>();
        let loopback_pump_tx_for_worker = loopback_pump_tx.clone();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<LoopbackOpenOutcome, String>>(1);

        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        let system_vad = self.system_vad.clone();
        // Move the optional level callback into the worker thread
        let level_cb = self.level_cb.clone();
        // Move the optional real-time audio frame callback into the worker thread
        let audio_cb = self.audio_cb.clone();
        let system_audio_cb = self.system_audio_cb.clone();
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        let system_audio_session = Arc::clone(&self.system_audio_session);
        let stream_error = Arc::clone(&self.stream_error);
        let system_audio_error = Arc::clone(&self.system_audio_error);

        let worker = std::thread::spawn(move || {
            let transport = Arc::new(CaptureTransportState::default());
            let (loopback_tx, loopback_rx) = mpsc::sync_channel::<LoopbackChunk>(16);
            let (loopback_buffer_tx, loopback_buffer_rx) = mpsc::sync_channel::<Vec<f32>>(16);
            for _ in 0..16 {
                let _ = loopback_buffer_tx.try_send(Vec::with_capacity(4096));
            }
            let dropped_loopback_samples = Arc::new(AtomicUsize::new(0));
            let loopback_sample_rate = Arc::new(AtomicU32::new(0));
            let init_result = (|| -> Result<_, String> {
                let config_started = Instant::now();
                let device_name = device_display_name(&thread_device).unwrap_or_default();
                let cached_config = config_cache
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|(name, _)| !device_name.is_empty() && *name == device_name)
                    .map(|(_, cfg)| *cfg);
                let config_was_cached = cached_config.is_some();
                let config = match cached_config {
                    Some(cfg) => cfg,
                    None => AudioRecorder::get_preferred_config(&thread_device)
                        .map_err(|e| format!("Failed to fetch preferred config: {e}"))?,
                };
                let config_elapsed = config_started.elapsed();

                let sample_rate = config.sample_rate();
                let channels = config.channels() as usize;

                log::info!(
                    "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                    thread_device,
                    sample_rate,
                    channels,
                    config.sample_format()
                );

                if let Some(channel) = selected_channel {
                    if channel < channels {
                        log::info!("Using selected input channel: {}", channel + 1);
                    } else {
                        log::warn!(
                            "Selected input channel {} is out of range for a {}-channel device; averaging all channels instead",
                            channel + 1,
                            channels
                        );
                    }
                } else {
                    log::info!("Averaging all {} input channels", channels);
                }

                let build_started = Instant::now();
                let (stream, sample_consumer) = match config.sample_format() {
                    cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                        &thread_device,
                        &config,
                        channels,
                        selected_channel,
                        Arc::clone(&transport),
                        Arc::clone(&stream_error),
                    ),
                    cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                        &thread_device,
                        &config,
                        channels,
                        selected_channel,
                        Arc::clone(&transport),
                        Arc::clone(&stream_error),
                    ),
                    cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                        &thread_device,
                        &config,
                        channels,
                        selected_channel,
                        Arc::clone(&transport),
                        Arc::clone(&stream_error),
                    ),
                    cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                        &thread_device,
                        &config,
                        channels,
                        selected_channel,
                        Arc::clone(&transport),
                        Arc::clone(&stream_error),
                    ),
                    cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                        &thread_device,
                        &config,
                        channels,
                        selected_channel,
                        Arc::clone(&transport),
                        Arc::clone(&stream_error),
                    ),
                    sample_format => {
                        return Err(format!("Unsupported sample format: {sample_format:?}"));
                    }
                }
                .map_err(|e| format!("Failed to build input stream: {e}"))?;
                let build_elapsed = build_started.elapsed();

                let play_started = Instant::now();
                stream
                    .play()
                    .map_err(|e| format!("Failed to start microphone stream: {e}"))?;

                let (loopback_stream, loopback_open_outcome) = match system_audio {
                    Some(capture) => match AudioRecorder::build_loopback_stream(
                        &capture.device,
                        loopback_tx,
                        loopback_buffer_rx,
                        Arc::clone(&system_audio_session),
                        Arc::clone(&system_audio_error),
                        Arc::clone(&loopback_sample_rate),
                        Arc::clone(&dropped_loopback_samples),
                    ) {
                        Ok(stream) => (Some(stream), LoopbackOpenOutcome::Active),
                        Err(error) => {
                            log::warn!(
                                "System audio capture unavailable; continuing microphone-only: {error}"
                            );
                            (
                                None,
                                LoopbackOpenOutcome::Unavailable {
                                    error: error.to_string(),
                                },
                            )
                        }
                    },
                    None => (None, LoopbackOpenOutcome::NotRequested),
                };
                log::debug!(
                    "mic worker init: fetch_config={:?} (cached={}) build_stream={:?} play={:?}",
                    config_elapsed,
                    config_was_cached,
                    build_elapsed,
                    play_started.elapsed()
                );

                // The device accepted this config; remember it so the next
                // open skips the HAL property queries entirely.
                if !config_was_cached && !device_name.is_empty() {
                    *config_cache.lock().unwrap() = Some((device_name, config));
                }

                Ok((
                    stream,
                    sample_rate,
                    sample_consumer,
                    loopback_stream,
                    loopback_open_outcome,
                ))
            })();

            match init_result {
                Ok((
                    stream,
                    sample_rate,
                    sample_consumer,
                    loopback_stream,
                    loopback_open_outcome,
                )) => {
                    let system_lane = if loopback_stream.is_some() {
                        let system_sample_rate = match loopback_sample_rate.load(Ordering::Acquire)
                        {
                            0 => constants::WHISPER_SAMPLE_RATE,
                            sample_rate => sample_rate,
                        };
                        match spawn_system_audio_lane(
                            system_sample_rate,
                            system_vad,
                            system_audio_cb,
                            system_sample_tx,
                            system_sample_rx,
                            system_cmd_rx,
                            loopback_rx,
                            loopback_buffer_tx,
                            loopback_pump_rx,
                            system_audio_session,
                        ) {
                            Ok(threads) => Some(threads),
                            Err(error_message) => {
                                let _ = init_tx.send(Err(error_message));
                                return;
                            }
                        }
                    } else {
                        None
                    };
                    let _ = init_tx.send(Ok(loopback_open_outcome));
                    // Timestamp for the play()-returned -> first-samples gap the
                    // init handshake can't see (hardware dependent).
                    let stream_running_at = Instant::now();
                    let processor = CaptureProcessor::new(
                        sample_rate,
                        vad,
                        level_cb,
                        audio_cb,
                        stream_running_at,
                    );
                    run_consumer(
                        processor,
                        sample_consumer,
                        cmd_rx,
                        transport,
                        Arc::clone(&stream_error),
                    );
                    drop(loopback_stream);
                    drop(stream);
                    if let Some(system_lane) = system_lane {
                        system_lane
                            .shut_down(&system_cmd_tx_for_worker, &loopback_pump_tx_for_worker);
                    }
                }
                Err(error_message) => {
                    // A failed open may mean the cached config went stale
                    // (device re-plugged, rate/format changed in the OS).
                    // Drop it so the next attempt re-queries the device.
                    *config_cache.lock().unwrap() = None;
                    log::error!("{error_message}");
                    let _ = init_tx.send(Err(error_message));
                }
            }
        });

        match init_rx.recv() {
            Ok(Ok(loopback_open_outcome)) => {
                let system_audio_active = loopback_open_outcome.is_active();
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                self.system_cmd_tx = system_audio_active.then_some(system_cmd_tx);
                self.loopback_pump_tx = system_audio_active.then_some(loopback_pump_tx);
                self.system_audio_active = system_audio_active;
                self.loopback_open_outcome = loopback_open_outcome;
                self.worker_handle = Some(worker);
                Ok(system_audio_active)
            }
            Ok(Err(error_message)) => {
                let _ = worker.join();
                let kind = if is_microphone_access_denied(&error_message) {
                    std::io::ErrorKind::PermissionDenied
                } else {
                    std::io::ErrorKind::Other
                };
                Err(Box::new(Error::new(kind, error_message)))
            }
            Err(recv_error) => {
                let _ = worker.join();
                Err(Box::new(Error::other(format!(
                    "Failed to initialize microphone worker: {recv_error}"
                ))))
            }
        }
    }

    /// Queue a recording start and return a one-shot receiver that resolves only
    /// after the first real microphone sample chunk has entered the capture path.
    /// `Stream::play()` returning is not sufficient: some Bluetooth and USB
    /// devices take much longer to begin delivering callbacks.
    pub fn start(
        &self,
        vad_policy: VadPolicy,
    ) -> Result<mpsc::Receiver<()>, Box<dyn std::error::Error>> {
        let tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let session_generation = {
            let generation = self
                .system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel)
                + 1;
            self.system_audio_session
                .active
                .store(true, Ordering::SeqCst);
            generation
        };
        // A dead system-audio lane must not cost the microphone its recording:
        // record this one microphone-only and rebuild the lane before the next.
        if let Some(pump_tx) = &self.loopback_pump_tx {
            if let Err(error) = pump_tx.send(LoopbackPumpCmd::StartSession(session_generation)) {
                log::warn!("Failed to start loopback pump; continuing microphone-only: {error}");
                self.system_audio_error.store(true, Ordering::Release);
            }
        }
        if let Some(system_tx) = &self.system_cmd_tx {
            let (system_ready_tx, _system_ready_rx) = mpsc::channel();
            if let Err(error) =
                system_tx.send(Cmd::Start(vad_policy, Instant::now(), system_ready_tx))
            {
                log::warn!(
                    "Failed to start system-audio consumer; continuing microphone-only: {error}"
                );
                self.system_audio_error.store(true, Ordering::Release);
            }
        }
        if let Err(error) = tx.send(Cmd::Start(vad_policy, Instant::now(), ready_tx)) {
            {
                self.system_audio_session
                    .active
                    .store(false, Ordering::Release);
                self.system_audio_session
                    .generation
                    .fetch_add(1, Ordering::AcqRel);
            }
            return Err(Box::new(error));
        }
        Ok(ready_rx)
    }

    pub fn stop(&self) -> Result<RecordedAudio, Box<dyn std::error::Error>> {
        let (mic_resp_tx, mic_resp_rx) = mpsc::channel();
        let mic_tx = self
            .cmd_tx
            .as_ref()
            .ok_or_else(|| Error::other("Recorder is not open"))?;
        {
            // SeqCst pairs with the loopback callback's `callback_busy` store:
            // once the pump sees no callback in flight, every later callback
            // sees this session as stopped.
            self.system_audio_session
                .active
                .store(false, Ordering::SeqCst);
            self.system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        let system_response = if let Some(system_tx) = &self.system_cmd_tx {
            let (system_resp_tx, system_resp_rx) = mpsc::channel();
            if let Err(error) = system_tx.send(Cmd::Stop(system_resp_tx)) {
                log::warn!("Failed to stop system-audio consumer: {error}");
                self.system_audio_error.store(true, Ordering::Release);
                None
            } else {
                if let Some(pump_tx) = &self.loopback_pump_tx {
                    if let Err(error) = pump_tx.send(LoopbackPumpCmd::EndSession) {
                        log::warn!("Failed to end loopback pump session: {error}");
                        self.system_audio_error.store(true, Ordering::Release);
                    }
                }
                Some(system_resp_rx)
            }
        } else {
            None
        };

        mic_tx.send(Cmd::Stop(mic_resp_tx))?;

        let stop_deadline = Instant::now() + CONSUMER_STOP_TIMEOUT;
        let microphone = mic_resp_rx
            .recv_timeout(stop_deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| {
                Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("Timed out waiting for microphone consumer stop: {error}"),
                )
            })?;
        let system = match system_response {
            Some(rx) => {
                match rx.recv_timeout(stop_deadline.saturating_duration_since(Instant::now())) {
                    Ok(samples) => samples,
                    Err(error) => {
                        log::warn!("Timed out waiting for system-audio consumer stop: {error}");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        };

        // The loopback error callback only sets the flag; report it here.
        if self.system_audio_error.load(Ordering::Acquire) {
            log::warn!("System audio capture failed; it will be rebuilt before the next recording");
        }

        Ok(RecordedAudio { microphone, system })
    }

    /// True when the active capture stream must be rebuilt.
    ///
    /// cpal may report a device disconnect asynchronously without closing its
    /// callback channel, so also honor the error callback's explicit flag.
    pub fn needs_reopen(&self) -> bool {
        self.stream_error.load(Ordering::Relaxed)
            || self.system_audio_error.load(Ordering::Relaxed)
            || self
                .worker_handle
                .as_ref()
                .is_some_and(|handle| handle.is_finished())
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        {
            self.system_audio_session
                .active
                .store(false, Ordering::SeqCst);
            self.system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        if let Some(tx) = self.system_cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        {
            self.loopback_pump_tx.take();
        }
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
        self.device = None;
        self.system_audio_active = false;
        self.loopback_open_outcome = LoopbackOpenOutcome::NotRequested;
        Ok(())
    }

    pub fn loopback_open_outcome(&self) -> &LoopbackOpenOutcome {
        &self.loopback_open_outcome
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        channels: usize,
        selected_channel: Option<usize>,
        transport: Arc<CaptureTransportState>,
        stream_error: Arc<AtomicBool>,
    ) -> Result<(cpal::Stream, Consumer<f32>), cpal::Error>
    where
        T: Sample + SizedSample + Copy + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let ring_capacity = config.sample_rate() as usize * AUDIO_RING_SECONDS;
        let (mut sample_producer, mut sample_consumer) = RingBuffer::new(ring_capacity);

        // Touch rtrb's uninitialized pages before the stream starts to reduce
        // callback page faults. This does not pin them.
        {
            let chunk = sample_producer
                .write_chunk(ring_capacity)
                .expect("new audio ring has its full capacity available");
            chunk.commit_all();
        }
        {
            let chunk = sample_consumer
                .read_chunk(ring_capacity)
                .expect("pre-filled audio ring is readable");
            chunk.commit_all();
        }

        // Resolve the effective channel to use. If the selected channel is
        // out of range for this device, fall back to averaging all channels.
        let use_channel = selected_channel.filter(|&channel| channel < channels);
        let callback_transport = Arc::clone(&transport);
        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            Self::write_input_to_ring(
                data,
                channels,
                use_channel,
                &mut sample_producer,
                &callback_transport,
            );
        };

        let stream = device.build_input_stream(
            (*config).into(),
            stream_cb,
            move |_err| {
                // Error callbacks may share the platform audio thread. Defer
                // logging and recovery to the consumer/manager path.
                stream_error.store(true, Ordering::Release);
            },
            None,
        )?;
        Ok((stream, sample_consumer))
    }

    /// Real-time callback body. Keep this allocation-free, wait-free, and free
    /// of locks, logging, clocks, and system calls.
    fn write_input_to_ring<T>(
        data: &[T],
        channels: usize,
        use_channel: Option<usize>,
        producer: &mut Producer<f32>,
        transport: &CaptureTransportState,
    ) where
        T: Sample + SizedSample + Copy,
        f32: cpal::FromSample<T>,
    {
        // Forward the first block that observes a pause; once acknowledged,
        // remain silent until the consumer resumes capture.
        if transport.pause_requested.load(Ordering::Acquire)
            && transport.pause_acknowledged.load(Ordering::Acquire)
        {
            return;
        }

        let frame_count = data.len() / channels;
        let writable_frames = producer.slots().min(frame_count);
        let written = if writable_frames == 0 {
            0
        } else {
            let chunk = producer
                .write_chunk_uninit(writable_frames)
                .expect("the producer just reported this many writable slots");
            if channels == 1 {
                chunk.fill_from_iter(
                    data.iter()
                        .take(writable_frames)
                        .map(|&sample| sample.to_sample::<f32>()),
                )
            } else if let Some(channel) = use_channel {
                chunk.fill_from_iter(
                    data.chunks_exact(channels)
                        .take(writable_frames)
                        .map(|frame| frame[channel].to_sample::<f32>()),
                )
            } else {
                chunk.fill_from_iter(data.chunks_exact(channels).take(writable_frames).map(
                    |frame| {
                        frame
                            .iter()
                            .map(|&sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / channels as f32
                    },
                ))
            }
        };
        debug_assert_eq!(written, writable_frames);

        let dropped = frame_count - written;
        if dropped > 0 {
            transport
                .overrun_samples
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }

        // Publish the boundary write before acknowledging, including when the
        // pause request arrives during the write.
        acknowledge_pause_after_write(transport);
    }

    fn build_loopback_stream(
        device: &cpal::Device,
        loopback_tx: mpsc::SyncSender<LoopbackChunk>,
        loopback_buffer_rx: mpsc::Receiver<Vec<f32>>,
        session: Arc<SystemAudioSession>,
        system_audio_error: Arc<AtomicBool>,
        shared_sample_rate: Arc<AtomicU32>,
        dropped_samples: Arc<AtomicUsize>,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
        let config = Self::get_preferred_loopback_config(device)?;
        let channels = usize::from(config.channels());
        let sample_rate = config.sample_rate();
        shared_sample_rate.store(sample_rate, Ordering::Release);
        log::info!(
            "Using system audio device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
            device,
            sample_rate,
            channels,
            config.sample_format()
        );

        let stream = match config.sample_format() {
            cpal::SampleFormat::U8 => Self::build_loopback_stream_typed::<u8>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&system_audio_error),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::I8 => Self::build_loopback_stream_typed::<i8>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&system_audio_error),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::I16 => Self::build_loopback_stream_typed::<i16>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&system_audio_error),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::I32 => Self::build_loopback_stream_typed::<i32>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&system_audio_error),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::F32 => Self::build_loopback_stream_typed::<f32>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&system_audio_error),
                Arc::clone(&dropped_samples),
            )?,
            sample_format => {
                return Err(Box::new(Error::other(format!(
                    "Unsupported loopback sample format: {sample_format:?}"
                ))));
            }
        };

        stream.play()?;
        Ok(stream)
    }

    fn build_loopback_stream_typed<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        loopback_tx: mpsc::SyncSender<LoopbackChunk>,
        loopback_buffer_rx: mpsc::Receiver<Vec<f32>>,
        session: Arc<SystemAudioSession>,
        system_audio_error: Arc<AtomicBool>,
        dropped_samples: Arc<AtomicUsize>,
    ) -> Result<cpal::Stream, cpal::Error>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut callback = LoopbackCallback {
            channels: usize::from(config.channels()),
            sample_rate: config.sample_rate(),
            session,
            emergency_buffer: Some(Vec::with_capacity(4096)),
            buffer_rx: loopback_buffer_rx,
            loopback_tx,
            dropped_samples,
        };

        device.build_input_stream(
            (*config).into(),
            move |data: &[T], _: &cpal::InputCallbackInfo| callback.process(data),
            loopback_error_callback(system_audio_error),
            None,
        )
    }

    fn get_preferred_loopback_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // WASAPI render endpoints reject input-config enumeration even though
        // cpal can open them for shared-mode loopback, so their output default
        // is authoritative there. Other backends may expose loopback as an
        // ordinary input device instead; fall back to that config shape.
        match device.default_output_config() {
            Ok(config) => {
                Self::select_supported_config(config, || device.supported_output_configs())
            }
            Err(output_error) => match device.default_input_config() {
                Ok(config) => {
                    Self::select_supported_config(config, || device.supported_input_configs())
                }
                Err(input_error) => Err(format!(
                    "no loopback config: output query failed ({output_error}), input query failed ({input_error})"
                )
                .into()),
            },
        }
    }

    pub fn preferred_input_channel_count(
        device: &cpal::Device,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        Ok(Self::get_preferred_config(device)?.channels())
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        let default_config = device.default_input_config()?;
        Self::select_supported_config(default_config, || device.supported_input_configs())
    }

    /// Formats `build_stream` and `build_loopback_stream_typed` can convert
    /// from. CPAL 0.18 changed default-format ranking, so a device default is
    /// a preference rather than a guarantee we can decode.
    const SUPPORTED_FORMATS: &[cpal::SampleFormat] = &[
        cpal::SampleFormat::F32,
        cpal::SampleFormat::I32,
        cpal::SampleFormat::I16,
        cpal::SampleFormat::I8,
        cpal::SampleFormat::U8,
    ];

    /// `supported_configs` is a closure, not an already-evaluated iterator, so
    /// enumeration only happens when the device default is a format we cannot
    /// decode. Some virtual and exotic capture devices answer
    /// `default_*_config()` fine but fail config enumeration; asking eagerly
    /// turned that into a hard `open()` failure where it had always been a
    /// warning and a fall back to the default.
    fn select_supported_config<I>(
        default_config: cpal::SupportedStreamConfig,
        supported_configs: impl FnOnce() -> Result<I, cpal::Error>,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>>
    where
        I: Iterator<Item = cpal::SupportedStreamConfigRange>,
    {
        if Self::SUPPORTED_FORMATS.contains(&default_config.sample_format()) {
            return Ok(default_config);
        }

        let supported_configs: Vec<_> = match supported_configs() {
            Ok(configs) => configs.collect(),
            Err(error) => {
                log::warn!("Could not enumerate stream configs ({error}), using device default");
                return Ok(default_config);
            }
        };
        for sample_format in Self::SUPPORTED_FORMATS {
            if let Some(config) = supported_configs
                .iter()
                .find(|config| config.sample_format() == *sample_format)
            {
                return Ok((*config).with_max_sample_rate());
            }
        }

        Err(Error::other("No recorder-supported audio sample format").into())
    }
}

fn acknowledge_pause_after_write(transport: &CaptureTransportState) {
    if transport.pause_requested.load(Ordering::Acquire) {
        transport.pause_acknowledged.store(true, Ordering::Release);
    }
}

pub fn is_microphone_access_denied(error_message: &str) -> bool {
    let normalized = error_message.to_lowercase();
    normalized.contains("access is denied")
        || normalized.contains("permission denied")
        || normalized.contains("0x80070005")
}

pub fn is_no_input_device_error(error_message: &str) -> bool {
    let normalized = error_message.to_lowercase();
    normalized.contains("no input device found")
        || (normalized.contains("failed to fetch preferred config")
            && normalized.contains("coreaudio"))
}

/// Route one 16 kHz frame through VAD to recording and live outputs.
/// Kept free-standing to permit disjoint borrows around resampler callbacks.
fn handle_frame(
    samples: &[f32],
    vad_policy: VadPolicy,
    vad: &Option<VadConfig>,
    audio_cb: &Option<AudioFrameCallback>,
    out_buf: &mut Vec<f32>,
) {
    let mut emit = |buf: &[f32]| {
        out_buf.extend_from_slice(buf);
        if let Some(cb) = audio_cb {
            cb(buf);
        }
    };

    if vad_policy == VadPolicy::Disabled {
        emit(samples);
        return;
    }

    if let Some(cfg) = vad {
        let mut detector = cfg.detector.lock().unwrap();
        match detector
            .push_frame(samples)
            .unwrap_or(VadFrame::Speech(samples))
        {
            VadFrame::Speech(buf) => emit(buf),
            VadFrame::Noise => {}
        }
    } else {
        emit(samples);
    }
}

fn drain_available_samples(
    consumer: &mut Consumer<f32>,
    max_samples: usize,
    mut process: impl FnMut(&[f32]),
) -> usize {
    let available = consumer.slots().min(max_samples);
    if available == 0 {
        return 0;
    }

    let chunk = consumer
        .read_chunk(available)
        .expect("reported audio ring slots must be readable");
    let (first, second) = chunk.as_slices();
    if !first.is_empty() {
        process(first);
    }
    if !second.is_empty() {
        process(second);
    }
    chunk.commit_all();
    available
}

/// What to do with a chunk drained from the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkDisposition {
    /// Process as active recording audio, including during the final stop drain.
    Capture,
    /// Consume idle audio without processing it.
    Discard,
}

/// Converts raw ring samples into 16 kHz frames across recording sessions.
/// Ring transport stays outside to avoid conflicting borrows during drains.
struct CaptureProcessor {
    // ---- stream-scoped: fixed for the life of the input stream ---------- //
    in_sample_rate: u32,
    vad: Option<VadConfig>,
    level_cb: Option<LevelCallback>,
    audio_cb: Option<AudioFrameCallback>,
    stream_running_at: Instant,
    visualizer: AudioVisualiser,
    frame_resampler: FrameResampler,
    max_drain_samples: usize,
    first_chunk_logged: bool,

    // ---- recording-scoped: reset by `begin_recording` ------------------- //
    vad_policy: VadPolicy,
    processed_samples: Vec<f32>,
    awaiting_first_captured_chunk: Option<Instant>,
    capture_ready_tx: Option<mpsc::Sender<()>>,
    total_dropped_samples: u64,
    overrun_warning_logged: bool,
}

impl CaptureProcessor {
    fn new(
        in_sample_rate: u32,
        vad: Option<VadConfig>,
        level_cb: Option<LevelCallback>,
        audio_cb: Option<AudioFrameCallback>,
        stream_running_at: Instant,
    ) -> Self {
        // Resample into frames sized for the active VAD backend (30 ms when
        // no detector is attached) so the detector never sees a partial frame.
        let frame_samples = vad.as_ref().map_or(
            (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize,
            |config| config.frame_samples,
        );
        let frame_duration =
            Duration::from_secs_f64(frame_samples as f64 / constants::WHISPER_SAMPLE_RATE as f64);
        let frame_resampler = FrameResampler::new(
            in_sample_rate as usize,
            constants::WHISPER_SAMPLE_RATE as usize,
            frame_duration,
        );

        const BUCKETS: usize = 16;
        let target_window = (f64::from(in_sample_rate) / 30.0).round() as usize;
        let window_size = [256usize, 512, 1024, 2048]
            .into_iter()
            .min_by_key(|w| w.abs_diff(target_window))
            .unwrap();
        let visualizer = AudioVisualiser::new(in_sample_rate, window_size, BUCKETS, 400.0, 4000.0);

        let max_drain_samples =
            ((in_sample_rate as u128 * MAX_DRAIN_CHUNK.as_millis()) / 1_000).max(1) as usize;

        Self {
            in_sample_rate,
            vad,
            level_cb,
            audio_cb,
            stream_running_at,
            visualizer,
            frame_resampler,
            max_drain_samples,
            first_chunk_logged: false,
            vad_policy: VadPolicy::Offline,
            processed_samples: Vec::new(),
            awaiting_first_captured_chunk: None,
            capture_ready_tx: None,
            total_dropped_samples: 0,
            overrun_warning_logged: false,
        }
    }

    /// Reset per-recording state and arm the first-sample acknowledgement.
    fn begin_recording(&mut self, policy: VadPolicy, ready_tx: mpsc::Sender<()>) {
        self.awaiting_first_captured_chunk = Some(Instant::now());
        self.capture_ready_tx = Some(ready_tx);
        self.total_dropped_samples = 0;
        self.overrun_warning_logged = false;
        self.vad_policy = policy;
        self.processed_samples.clear();
        self.visualizer.reset();
        self.frame_resampler.reset();
        if policy != VadPolicy::Disabled {
            if let Some(cfg) = &self.vad {
                let mut detector = cfg.detector.lock().unwrap();
                detector.set_hangover_frames(cfg.hangover_for(policy));
                detector.reset();
            }
        }
    }

    /// Drop a pending first-sample acknowledgement. If Stop was queued before
    /// the first chunk, this prevents a stale ready UI event or start chime.
    fn cancel_ready_signal(&mut self) {
        self.capture_ready_tx = None;
        self.awaiting_first_captured_chunk = None;
    }

    /// Drain up to one bounded chunk from the ring. Returns the number of
    /// samples consumed so callers can tell an empty ring from a busy one.
    fn drain(&mut self, consumer: &mut Consumer<f32>, disposition: ChunkDisposition) -> usize {
        let max_samples = self.max_drain_samples;
        drain_available_samples(consumer, max_samples, |raw| {
            self.process_raw_chunk(raw, disposition)
        })
    }

    fn process_raw_chunk(&mut self, raw: &[f32], disposition: ChunkDisposition) {
        let chunk_ms = raw.len() as f64 * 1000.0 / self.in_sample_rate as f64;
        if !self.first_chunk_logged {
            self.first_chunk_logged = true;
            log::debug!(
                "first audio samples arrived {:?} after stream start ({:.1}ms drained)",
                self.stream_running_at.elapsed(),
                chunk_ms
            );
        }

        if disposition == ChunkDisposition::Discard {
            return;
        }

        if let Some(buckets) = self.visualizer.feed(raw) {
            if let Some(callback) = &self.level_cb {
                callback(buckets);
            }
        }

        let vad_policy = self.vad_policy;
        self.frame_resampler.push(raw, |frame: &[f32]| {
            handle_frame(
                frame,
                vad_policy,
                &self.vad,
                &self.audio_cb,
                &mut self.processed_samples,
            )
        });

        if let Some(started) = self.awaiting_first_captured_chunk.take() {
            log::debug!(
                "first captured samples ({:.1}ms) processed {:?} after Cmd::Start",
                chunk_ms,
                started.elapsed()
            );
        }
        if let Some(ready_tx) = self.capture_ready_tx.take() {
            // Silence still counts: readiness means the host is delivering samples,
            // not that VAD has detected speech.
            let _ = ready_tx.send(());
        }
    }

    /// Account for samples the callback could not fit into the ring during
    /// the active recording. Warns once per recording.
    fn observe_overrun(&mut self, samples: u64) {
        if samples == 0 {
            return;
        }

        self.total_dropped_samples = self.total_dropped_samples.saturating_add(samples);
        if !self.overrun_warning_logged {
            self.overrun_warning_logged = true;
            log::warn!(
                "Microphone capture ring dropped {samples} samples; continuing the active recording"
            );
        }
    }

    /// Flush the resampler tail and hand back the finished recording.
    fn finish_recording(&mut self) -> Vec<f32> {
        let vad_policy = self.vad_policy;
        self.frame_resampler.finish(|frame: &[f32]| {
            handle_frame(
                frame,
                vad_policy,
                &self.vad,
                &self.audio_cb,
                &mut self.processed_samples,
            )
        });

        // Diagnostic for VAD audio still withheld when capture stopped; it is
        // not conclusive in either direction.
        if vad_policy != VadPolicy::Disabled {
            if let Some(cfg) = &self.vad {
                let report = cfg.detector.lock().unwrap().tail_report();
                if let Some(report) = report {
                    log::debug!(
                        "VAD at stop: withheld tail {} frames (~{}ms, {} voiced), in_speech={}, onset_counter={}, hangover_counter={}",
                        report.withheld_frames,
                        report.withheld_frames * cfg.frame_samples * 1000
                            / constants::WHISPER_SAMPLE_RATE as usize,
                        report.withheld_voiced_frames,
                        report.in_speech,
                        report.onset_counter,
                        report.hangover_counter
                    );
                }
            }
        }

        if self.total_dropped_samples > 0 {
            log::warn!(
                "Active recording completed after dropping {} microphone samples",
                self.total_dropped_samples
            );
        }
        std::mem::take(&mut self.processed_samples)
    }
}

fn run_consumer(
    mut processor: CaptureProcessor,
    mut sample_consumer: Consumer<f32>,
    cmd_rx: mpsc::Receiver<Cmd>,
    transport: Arc<CaptureTransportState>,
    stream_error: Arc<AtomicBool>,
) {
    let mut recording = false;
    let mut stream_error_logged = false;

    loop {
        // Avoid sleeping with queued audio; check commands before each bounded
        // drain so Stop cannot sit behind a multi-second backlog.
        let mut command = if sample_consumer.slots() > 0 {
            match cmd_rx.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        } else {
            match cmd_rx.recv_timeout(CONSUMER_POLL_INTERVAL) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        };

        loop {
            if let Some(cmd) = command.take() {
                match cmd {
                    Cmd::Start(policy, sent_at, ready_tx) => {
                        log::debug!(
                            "Cmd::Start processed {:?} after send; capture begins with {} samples",
                            sent_at.elapsed(),
                            if sample_consumer.slots() > 0 {
                                "the in-flight"
                            } else {
                                "the next available"
                            }
                        );
                        // Ignore overruns accumulated while the always-on stream
                        // was idle; only active-capture loss is relevant.
                        transport.overrun_samples.store(0, Ordering::Release);
                        processor.begin_recording(policy, ready_tx);
                        recording = true;
                    }
                    Cmd::Stop(reply_tx) => {
                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        recording = false;
                        processor.cancel_ready_signal();

                        // Request a pause that forwards one boundary block, then drain
                        // all audio committed before the acknowledgement.
                        transport.pause_acknowledged.store(false, Ordering::Relaxed);
                        transport.pause_requested.store(true, Ordering::Release);
                        let pause_started = Instant::now();
                        while !transport.pause_acknowledged.load(Ordering::Acquire)
                            && pause_started.elapsed() < PAUSE_ACK_TIMEOUT
                        {
                            let drained =
                                processor.drain(&mut sample_consumer, ChunkDisposition::Capture);
                            if drained == 0 {
                                std::thread::sleep(Duration::from_millis(1));
                            }
                        }

                        let pause_timed_out = !transport.pause_acknowledged.load(Ordering::Acquire);
                        if pause_timed_out {
                            log::warn!("Timed out waiting for the microphone callback to pause");
                            // Preserve the existing recovery model: finish this
                            // stop, then rebuild the stream on the next start.
                            stream_error.store(true, Ordering::Release);
                        }

                        // Everything still in the ring, including the boundary
                        // block, belongs to this recording.
                        while processor.drain(&mut sample_consumer, ChunkDisposition::Capture) > 0 {
                        }

                        // Include drops that raced with the pause request.
                        processor
                            .observe_overrun(transport.overrun_samples.swap(0, Ordering::AcqRel));
                        let samples = processor.finish_recording();
                        if !pause_timed_out {
                            // Resume before stop() returns so an immediate recording
                            // cannot lose its first callback to this pause request.
                            transport.pause_acknowledged.store(false, Ordering::Relaxed);
                            transport.pause_requested.store(false, Ordering::Release);
                        }
                        let _ = reply_tx.send(samples);

                        if pause_timed_out {
                            return;
                        }
                    }
                    Cmd::Shutdown => {
                        transport.pause_requested.store(true, Ordering::Release);
                        return;
                    }
                }
            }

            command = match cmd_rx.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            };
        }

        let disposition = if recording {
            ChunkDisposition::Capture
        } else {
            ChunkDisposition::Discard
        };
        processor.drain(&mut sample_consumer, disposition);

        let overrun_samples = transport.overrun_samples.swap(0, Ordering::AcqRel);
        if recording {
            processor.observe_overrun(overrun_samples);
        }

        // The CPAL error callback only sets an atomic; log here and rebuild the
        // stream on the next start.
        if stream_error.load(Ordering::Acquire) && !stream_error_logged {
            log::error!("Microphone backend reported a stream error; it will be rebuilt");
            stream_error_logged = true;
        }
    }
}

// ---- Shorthand: system-audio (loopback) lane ------------------------------ //

/// The system-audio consumer and loopback pump threads of one recorder open.
struct SystemAudioLane {
    consumer: std::thread::JoinHandle<()>,
    pump: std::thread::JoinHandle<()>,
}

impl SystemAudioLane {
    /// Tell both threads to exit, then wait for them. The recorder keeps its
    /// own senders until `close()`, so dropped senders alone would not stop
    /// them, and a capture worker whose microphone consumer had already
    /// returned would never finish.
    fn shut_down(self, cmd_tx: &mpsc::Sender<Cmd>, pump_tx: &mpsc::Sender<LoopbackPumpCmd>) {
        let _ = cmd_tx.send(Cmd::Shutdown);
        let _ = pump_tx.send(LoopbackPumpCmd::Shutdown);
        let _ = self.consumer.join();
        let _ = self.pump.join();
    }
}

/// Start the system-audio consumer and the loopback pump that feeds it.
/// Returns their join handles, or the init error to report from `open()`.
#[allow(clippy::too_many_arguments)]
fn spawn_system_audio_lane(
    sample_rate: u32,
    vad: Option<VadConfig>,
    audio_cb: Option<AudioFrameCallback>,
    sample_tx: mpsc::Sender<AudioChunk>,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    loopback_rx: mpsc::Receiver<LoopbackChunk>,
    loopback_buffer_tx: mpsc::SyncSender<Vec<f32>>,
    pump_rx: mpsc::Receiver<LoopbackPumpCmd>,
    session: Arc<SystemAudioSession>,
) -> Result<SystemAudioLane, String> {
    let stream_running_at = Instant::now();
    let consumer = std::thread::Builder::new()
        .name("audio-loopback-consumer".to_string())
        .spawn(move || {
            // The system lane has no level meter; the processor is otherwise
            // the microphone's, so both lanes resample and filter identically.
            let processor =
                CaptureProcessor::new(sample_rate, vad, None, audio_cb, stream_running_at);
            run_system_consumer(processor, sample_rx, cmd_rx);
        })
        .map_err(|error| format!("Failed to start loopback consumer thread: {error}"))?;
    let pump = std::thread::Builder::new()
        .name("audio-loopback-pump".to_string())
        .spawn(move || {
            run_loopback_pump(
                loopback_rx,
                loopback_buffer_tx,
                sample_tx,
                pump_rx,
                session,
                sample_rate,
            );
        });
    match pump {
        Ok(pump) => Ok(SystemAudioLane { consumer, pump }),
        Err(error) => {
            // The failed spawn dropped `sample_tx`, which disconnects the
            // consumer's sample channel and lets it exit.
            let _ = consumer.join();
            Err(format!("Failed to start loopback pump thread: {error}"))
        }
    }
}

/// State owned by the loopback stream's data callback.
struct LoopbackCallback {
    channels: usize,
    sample_rate: u32,
    session: Arc<SystemAudioSession>,
    /// Held back for when the pool is empty or a send fails, so a full channel
    /// costs one block rather than an allocation.
    emergency_buffer: Option<Vec<f32>>,
    buffer_rx: mpsc::Receiver<Vec<f32>>,
    loopback_tx: mpsc::SyncSender<LoopbackChunk>,
    dropped_samples: Arc<AtomicUsize>,
}

impl LoopbackCallback {
    /// Real-time callback body. Like `write_input_to_ring`, keep this free of
    /// allocation, locks, logging and blocking: buffers come from the pump's
    /// pool, which returns them with their capacity.
    fn process<T>(&mut self, data: &[T])
    where
        T: Sample + Copy,
        f32: cpal::FromSample<T>,
    {
        self.session.callback_busy.store(true, Ordering::SeqCst);
        // A block belongs to the session that was active when it arrived, even
        // if `stop()` runs before it is handed over: the pump drains it.
        let session_generation = self.session.generation.load(Ordering::SeqCst);
        if self.session.active.load(Ordering::SeqCst) {
            self.forward(data, session_generation);
        }
        self.session.callback_busy.store(false, Ordering::SeqCst);
    }

    fn forward<T>(&mut self, data: &[T], session_generation: u64)
    where
        T: Sample + Copy,
        f32: cpal::FromSample<T>,
    {
        let Some(mut mono_buffer) = self
            .emergency_buffer
            .take()
            .or_else(|| self.buffer_rx.try_recv().ok())
        else {
            self.dropped_samples
                .fetch_add(data.len() / self.channels, Ordering::Relaxed);
            return;
        };
        downmix_loopback(data, self.channels, &mut mono_buffer);
        let sample_count = mono_buffer.len();
        let chunk = LoopbackChunk {
            samples: mono_buffer,
            sample_rate: self.sample_rate,
            session_generation,
        };
        match self.loopback_tx.try_send(chunk) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(chunk)) => {
                self.dropped_samples
                    .fetch_add(sample_count, Ordering::Relaxed);
                self.emergency_buffer = Some(chunk.samples);
            }
            Err(mpsc::TrySendError::Disconnected(chunk)) => {
                self.emergency_buffer = Some(chunk.samples);
            }
        }
    }
}

/// Error callback for the loopback stream. Like the microphone's, it may run
/// on the platform audio thread, so it only sets a flag: `stop()` reports it
/// and `needs_reopen()` rebuilds the lane before the next recording.
fn loopback_error_callback(
    system_audio_error: Arc<AtomicBool>,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| {
        if loopback_error_requires_rebuild(error.kind()) {
            system_audio_error.store(true, Ordering::Release);
        }
    }
}

/// cpal 0.18 also reports conditions that leave the stream capturing: an
/// overrun, a refused real-time promotion, or an automatic reroute to a new
/// default device. Rebuilding for those would restart capture for nothing.
fn loopback_error_requires_rebuild(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied | cpal::ErrorKind::DeviceChanged
    )
}

fn downmix_loopback<T>(data: &[T], channels: usize, output: &mut Vec<f32>)
where
    T: Sample + Copy,
    f32: cpal::FromSample<T>,
{
    output.clear();
    if channels == 0 {
        return;
    }
    output.reserve(data.len() / channels);

    if channels == 1 {
        output.extend(data.iter().map(|sample| (*sample).to_sample::<f32>()));
        return;
    }
    if channels == 2 {
        output.extend(
            data.chunks_exact(2)
                .map(|frame| (frame[0].to_sample::<f32>() + frame[1].to_sample::<f32>()) * 0.5),
        );
        return;
    }

    // Windows uses FL, FR, FC, LFE, then surround/back channels for its common
    // 5.1/7.1 layouts. Preserve centre-channel speech, ignore LFE, and spread
    // the remaining weight across the surrounds. cpal exposes only a channel
    // count, not the endpoint's channel mask, so exotic layouts may differ.
    for frame in data.chunks_exact(channels) {
        let front = frame[0].to_sample::<f32>() * 0.2
            + frame[1].to_sample::<f32>() * 0.2
            + frame[2].to_sample::<f32>() * 0.4;
        let surround_count = channels.saturating_sub(4);
        let surround = if surround_count == 0 {
            0.0
        } else {
            frame[4..]
                .iter()
                .map(|sample| (*sample).to_sample::<f32>())
                .sum::<f32>()
                * (0.2 / surround_count as f32)
        };
        output.push(front + surround);
    }
}

fn run_loopback_pump(
    loopback_rx: mpsc::Receiver<LoopbackChunk>,
    loopback_buffer_tx: mpsc::SyncSender<Vec<f32>>,
    sample_tx: mpsc::Sender<AudioChunk>,
    control_rx: mpsc::Receiver<LoopbackPumpCmd>,
    session: Arc<SystemAudioSession>,
    sample_rate: u32,
) {
    let pump_interval = Duration::from_millis(LOOPBACK_PUMP_INTERVAL_MS as u64);
    let silence_len =
        ((u64::from(sample_rate) * LOOPBACK_PUMP_INTERVAL_MS as u64) / 1000).max(1) as usize;
    let mut loopback_connected = true;

    loop {
        // No wakeups or allocations between recordings. The pump becomes active
        // only after the recorder explicitly starts a system-audio session.
        let mut session_generation = match control_rx.recv() {
            Ok(LoopbackPumpCmd::StartSession(generation)) => generation,
            Ok(LoopbackPumpCmd::EndSession) => {
                // A stop outside a session still owes its consumer a marker.
                if send_end_of_stream(&sample_tx, silence_len).is_err() {
                    return;
                }
                continue;
            }
            Ok(LoopbackPumpCmd::Shutdown) | Err(_) => return,
        };
        let mut next_tick = Instant::now() + pump_interval;

        'session: loop {
            while let Ok(command) = control_rx.try_recv() {
                match command {
                    LoopbackPumpCmd::StartSession(generation) => {
                        session_generation = generation;
                        next_tick = Instant::now() + pump_interval;
                    }
                    LoopbackPumpCmd::EndSession => {
                        if end_loopback_session(
                            &loopback_rx,
                            &loopback_buffer_tx,
                            &sample_tx,
                            &session,
                            session_generation,
                            sample_rate,
                            silence_len,
                        )
                        .is_err()
                        {
                            return;
                        }
                        break 'session;
                    }
                    LoopbackPumpCmd::Shutdown => return,
                }
            }

            let now = Instant::now();
            let wait = next_tick.saturating_duration_since(now);
            let receive_result = if loopback_connected {
                loopback_rx.recv_timeout(wait)
            } else {
                match control_rx.recv_timeout(wait) {
                    Ok(LoopbackPumpCmd::EndSession) => {
                        if end_loopback_session(
                            &loopback_rx,
                            &loopback_buffer_tx,
                            &sample_tx,
                            &session,
                            session_generation,
                            sample_rate,
                            silence_len,
                        )
                        .is_err()
                        {
                            return;
                        }
                        break 'session;
                    }
                    Ok(LoopbackPumpCmd::StartSession(generation)) => {
                        session_generation = generation;
                        next_tick = Instant::now() + pump_interval;
                        continue;
                    }
                    Ok(LoopbackPumpCmd::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => Err(mpsc::RecvTimeoutError::Timeout),
                }
            };

            match receive_result {
                Ok(chunk) => {
                    let Ok(forwarded) = forward_loopback_chunk(
                        chunk,
                        session_generation,
                        sample_rate,
                        &sample_tx,
                        &loopback_buffer_tx,
                        &session,
                    ) else {
                        return;
                    };
                    next_tick += Duration::from_secs_f64(forwarded as f64 / sample_rate as f64);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let now = Instant::now();
                    while now >= next_tick {
                        if sample_tx
                            .send(AudioChunk::Samples(vec![0.0; silence_len]))
                            .is_err()
                        {
                            return;
                        }
                        next_tick += pump_interval;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // A dead render endpoint must degrade to synthetic silence, not
                    // take command polling (and therefore stop/shutdown) down with it.
                    loopback_connected = false;
                }
            }
        }
    }
}

/// Forward one loopback block if it belongs to the pump's session, and return
/// its buffer to the callback's pool either way. The samples are copied here,
/// off the audio thread, so the pooled buffer keeps its capacity and the
/// callback never allocates. Returns how many samples were forwarded.
fn forward_loopback_chunk(
    mut chunk: LoopbackChunk,
    session_generation: u64,
    sample_rate: u32,
    sample_tx: &mpsc::Sender<AudioChunk>,
    loopback_buffer_tx: &mpsc::SyncSender<Vec<f32>>,
    session: &SystemAudioSession,
) -> Result<usize, mpsc::SendError<AudioChunk>> {
    let mut forwarded = 0;
    if chunk.session_generation != session_generation {
        session
            .stale_samples
            .fetch_add(chunk.samples.len(), Ordering::Relaxed);
    } else if chunk.sample_rate != sample_rate {
        log::warn!(
            "Loopback sample rate changed from {sample_rate} to {}; dropping packet",
            chunk.sample_rate
        );
    } else {
        forwarded = chunk.samples.len();
        sample_tx.send(AudioChunk::Samples(chunk.samples.clone()))?;
    }
    chunk.samples.clear();
    let _ = loopback_buffer_tx.try_send(chunk.samples);
    Ok(forwarded)
}

/// Close the pump's session. First wait out a callback that was mid-block
/// when `stop()` cleared `active`, then forward everything already queued for
/// the session, so the recording keeps its tail. Finally wake
/// `run_system_consumer` (its Cmd::Stop is queued before EndSession) and place
/// exactly one sentinel behind that wake-up chunk for its drain.
fn end_loopback_session(
    loopback_rx: &mpsc::Receiver<LoopbackChunk>,
    loopback_buffer_tx: &mpsc::SyncSender<Vec<f32>>,
    sample_tx: &mpsc::Sender<AudioChunk>,
    session: &SystemAudioSession,
    session_generation: u64,
    sample_rate: u32,
    silence_len: usize,
) -> Result<(), mpsc::SendError<AudioChunk>> {
    let settle_started = Instant::now();
    while session.callback_busy.load(Ordering::SeqCst) {
        if settle_started.elapsed() >= LOOPBACK_CALLBACK_SETTLE_TIMEOUT {
            log::warn!("Loopback callback still running at stop; its block may be lost");
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    while let Ok(chunk) = loopback_rx.try_recv() {
        forward_loopback_chunk(
            chunk,
            session_generation,
            sample_rate,
            sample_tx,
            loopback_buffer_tx,
            session,
        )?;
    }
    send_end_of_stream(sample_tx, silence_len)
}

fn send_end_of_stream(
    sample_tx: &mpsc::Sender<AudioChunk>,
    silence_len: usize,
) -> Result<(), mpsc::SendError<AudioChunk>> {
    sample_tx.send(AudioChunk::Samples(vec![0.0; silence_len]))?;
    sample_tx.send(AudioChunk::EndOfStream)
}

/// Consumer for the system-audio lane. Processing is the microphone's
/// `CaptureProcessor`; only the transport differs. Loopback arrives as
/// `AudioChunk`s from `run_loopback_pump`, and a stop drains up to the pump's
/// `EndOfStream` sentinel rather than to a ring pause acknowledgement.
fn run_system_consumer(
    mut processor: CaptureProcessor,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
) {
    let mut recording = false;
    // Markers owed to stops that timed out waiting for them. The pump sends
    // one marker per stop, so everything up to an owed marker belongs to that
    // stop's session: it is discarded rather than credited to a later one,
    // and the marker cannot end a later stop's drain early.
    let mut owed_end_markers = 0usize;

    // Poll commands even when the pump stops producing samples.
    loop {
        let mut pending = match sample_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => Some(chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if owed_end_markers > 0 && matches!(pending.take(), Some(AudioChunk::EndOfStream)) {
            owed_end_markers -= 1;
        }

        // Handle pending commands BEFORE the in-flight chunk so a Start
        // captures it.
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start(policy, sent_at, ready_tx) => {
                    log::debug!(
                        "Cmd::Start processed {:?} after send; capture begins with {} chunk",
                        sent_at.elapsed(),
                        if pending.is_some() {
                            "the in-flight"
                        } else {
                            "the next available"
                        }
                    );
                    processor.begin_recording(policy, ready_tx);
                    recording = true;
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    processor.cancel_ready_signal();

                    // The chunk in hand arrived before the stop; it belongs to
                    // the recording, so feed it ahead of the drain below.
                    if let Some(AudioChunk::Samples(raw)) = pending.take() {
                        processor.process_raw_chunk(&raw, ChunkDisposition::Capture);
                    }

                    // Drain all remaining audio until the pump confirms
                    // end-of-stream. `LoopbackPumpCmd::EndSession` queues a
                    // wake-up chunk and then the sentinel behind every chunk
                    // already sent for this session.
                    loop {
                        match sample_rx.recv_timeout(SYSTEM_STOP_DRAIN_TIMEOUT) {
                            Ok(AudioChunk::Samples(remaining)) => {
                                if owed_end_markers == 0 {
                                    processor
                                        .process_raw_chunk(&remaining, ChunkDisposition::Capture);
                                }
                            }
                            Ok(AudioChunk::EndOfStream) if owed_end_markers > 0 => {
                                owed_end_markers -= 1;
                            }
                            Ok(AudioChunk::EndOfStream) => break,
                            Err(_) => {
                                log::warn!("Timed out waiting for EndOfStream from loopback pump");
                                owed_end_markers += 1;
                                break;
                            }
                        }
                    }

                    let _ = reply_tx.send(processor.finish_recording());
                }
                Cmd::Shutdown => return,
            }
        }

        let raw = match pending.take() {
            Some(AudioChunk::Samples(s)) => s,
            // EndOfStream, or the chunk was consumed by a Stop above.
            _ => continue,
        };

        let disposition = if recording {
            ChunkDisposition::Capture
        } else {
            ChunkDisposition::Discard
        };
        processor.process_raw_chunk(&raw, disposition);
    }
}

// ---- end Shorthand -------------------------------------------------------- //

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shorthand_tests;

/// Opens the real system-audio loopback endpoint on developer machines. CI is
/// intentionally skipped because its runners have no audio hardware; these
/// tests catch CPAL format negotiation or stream-opening regressions that
/// synthetic loopback-pump tests cannot exercise.
#[cfg(test)]
mod hardware_tests {
    use super::AudioRecorder;
    use cpal::traits::{DeviceTrait, StreamTrait};
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Duration;

    fn system_audio_device() -> Option<cpal::Device> {
        #[cfg(target_os = "linux")]
        {
            crate::audio_toolkit::resolve_linux_system_audio_device(None)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut devices = crate::audio_toolkit::list_system_audio_devices().ok()?;
            let index = devices
                .iter()
                .position(|device| device.is_default)
                .unwrap_or(0);
            devices.get_mut(index).map(|device| device.device.clone())
        }
    }

    fn build_input_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        callback_fired: Arc<AtomicBool>,
    ) -> cpal::Stream
    where
        T: cpal::SizedSample,
    {
        device
            .build_input_stream(
                (*config).into(),
                move |_: &[T], _: &cpal::InputCallbackInfo| {
                    callback_fired.store(true, std::sync::atomic::Ordering::Release);
                },
                |error| log::warn!("System-audio hardware test stream error: {error}"),
                None,
            )
            .expect("build a system-audio input stream")
    }

    #[cfg(not(target_os = "linux"))]
    fn build_silent_output_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
    ) -> cpal::Stream
    where
        T: cpal::Sample + cpal::SizedSample,
    {
        device
            .build_output_stream(
                (*config).into(),
                |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    data.fill(T::EQUILIBRIUM);
                },
                |error| log::warn!("System-audio hardware test output stream error: {error}"),
                None,
            )
            .expect("build a silent system-audio output stream")
    }

    #[test]
    fn negotiated_loopback_format_is_supported() {
        if std::env::var("CI").is_ok() {
            return;
        }

        let Some(device) = system_audio_device() else {
            eprintln!("No system-audio device is available; skipping hardware test");
            return;
        };
        let config = AudioRecorder::get_preferred_loopback_config(&device)
            .expect("negotiate a system-audio loopback config");
        assert!(
            AudioRecorder::SUPPORTED_FORMATS.contains(&config.sample_format()),
            "negotiated unsupported loopback format: {:?}",
            config.sample_format()
        );
        assert!(
            config.channels() > 0,
            "negotiated loopback config has no channels"
        );
    }

    #[test]
    fn system_audio_input_stream_delivers_frames() {
        if std::env::var("CI").is_ok() {
            return;
        }

        let Some(device) = system_audio_device() else {
            eprintln!("No system-audio device is available; skipping hardware test");
            return;
        };
        let config = AudioRecorder::get_preferred_loopback_config(&device)
            .expect("negotiate a system-audio loopback config");
        let callback_fired = Arc::new(AtomicBool::new(false));

        // WASAPI/CoreAudio loopback can remain idle until an output client is
        // active. Keep one silent client alive so the capture endpoint is
        // scheduled; this test still treats silence as valid capture data.
        #[cfg(not(target_os = "linux"))]
        let output_stream = match config.sample_format() {
            cpal::SampleFormat::U8 => build_silent_output_stream::<u8>(&device, &config),
            cpal::SampleFormat::I8 => build_silent_output_stream::<i8>(&device, &config),
            cpal::SampleFormat::I16 => build_silent_output_stream::<i16>(&device, &config),
            cpal::SampleFormat::I32 => build_silent_output_stream::<i32>(&device, &config),
            cpal::SampleFormat::F32 => build_silent_output_stream::<f32>(&device, &config),
            sample_format => panic!("unsupported loopback format: {sample_format:?}"),
        };
        #[cfg(not(target_os = "linux"))]
        output_stream
            .play()
            .expect("start a silent system-audio output stream");

        let stream = match config.sample_format() {
            cpal::SampleFormat::U8 => {
                build_input_stream::<u8>(&device, &config, Arc::clone(&callback_fired))
            }
            cpal::SampleFormat::I8 => {
                build_input_stream::<i8>(&device, &config, Arc::clone(&callback_fired))
            }
            cpal::SampleFormat::I16 => {
                build_input_stream::<i16>(&device, &config, Arc::clone(&callback_fired))
            }
            cpal::SampleFormat::I32 => {
                build_input_stream::<i32>(&device, &config, Arc::clone(&callback_fired))
            }
            cpal::SampleFormat::F32 => {
                build_input_stream::<f32>(&device, &config, Arc::clone(&callback_fired))
            }
            sample_format => panic!("unsupported loopback format: {sample_format:?}"),
        };
        stream.play().expect("start a system-audio input stream");
        std::thread::sleep(Duration::from_millis(750));

        // Silence is legitimate, and macOS may deliver all-zero buffers when
        // permission is denied. This test verifies frame arrival only.
        assert!(
            callback_fired.load(std::sync::atomic::Ordering::Acquire),
            "system-audio stream did not deliver a data callback"
        );
    }
}
