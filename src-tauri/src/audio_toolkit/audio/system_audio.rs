//! Shorthand: the system-audio (loopback) lane.
//!
//! Upstream records the microphone through `handy-recorder`, which captures
//! input devices only. Meetings also record what the computer plays, so the
//! fork opens the render endpoint (or, on Linux, a monitor source) through
//! cpal here and runs it beside the microphone recorder: its own stream,
//! detector, resampler and threads, started and stopped with each recording.
//!
//! A pump thread paces the lane and supplies the end-of-stream sentinel its
//! consumer drains to at stop, because an idle render endpoint can deliver no
//! callbacks at all.

use std::{
    io::Error,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, Sample, SizedSample,
};

use crate::audio_toolkit::{
    audio::{AudioFrameCallback, FrameResampler, VadPolicy},
    constants,
    vad::{VadFrame, VoiceActivityDetector},
};

enum Cmd {
    Start(VadPolicy),
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

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

/// The lane's detector. Never the microphone's: the two must not share Silero
/// recurrent state or smoothing counters.
pub struct LaneVad {
    detector: Box<dyn VoiceActivityDetector>,
    offline_hangover_frames: usize,
    streaming_hangover_frames: usize,
}

impl LaneVad {
    pub fn new(
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        assert!(
            detector.frame_samples() > 0,
            "VAD frame size must be non-zero"
        );
        Self {
            detector,
            offline_hangover_frames,
            streaming_hangover_frames,
        }
    }

    fn hangover_for(&self, policy: VadPolicy) -> usize {
        match policy {
            VadPolicy::Streaming => self.streaming_hangover_frames,
            VadPolicy::Offline | VadPolicy::Disabled => self.offline_hangover_frames,
        }
    }
}

/// What to do with a chunk the consumer receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkDisposition {
    /// Process as active recording audio, including during the final stop drain.
    Capture,
    /// Consume idle audio without processing it.
    Discard,
}

/// Resamples loopback audio to 16 kHz detector-sized frames, filters them
/// through the lane's VAD policy, and keeps what passes for the recording.
struct LaneProcessor {
    vad: Option<LaneVad>,
    on_audio: AudioFrameCallback,
    frame_resampler: FrameResampler,
    vad_policy: VadPolicy,
    processed_samples: Vec<f32>,
}

impl LaneProcessor {
    fn new(in_sample_rate: u32, vad: Option<LaneVad>, on_audio: AudioFrameCallback) -> Self {
        // Resample into frames sized for the detector (30 ms without one) so
        // it never sees a partial frame.
        let frame_samples = vad.as_ref().map_or(
            (constants::WHISPER_SAMPLE_RATE * 30 / 1000) as usize,
            |vad| vad.detector.frame_samples(),
        );
        let frame_duration =
            Duration::from_secs_f64(frame_samples as f64 / constants::WHISPER_SAMPLE_RATE as f64);
        Self {
            vad,
            on_audio,
            frame_resampler: FrameResampler::new(
                in_sample_rate as usize,
                constants::WHISPER_SAMPLE_RATE as usize,
                frame_duration,
            ),
            vad_policy: VadPolicy::Offline,
            processed_samples: Vec::new(),
        }
    }

    fn begin_recording(&mut self, policy: VadPolicy) {
        self.vad_policy = policy;
        self.processed_samples.clear();
        self.frame_resampler.reset();
        if policy != VadPolicy::Disabled {
            if let Some(vad) = &mut self.vad {
                let hangover = vad.hangover_for(policy);
                vad.detector.set_hangover_frames(hangover);
                vad.detector.reset();
            }
        }
    }

    fn process_raw_chunk(&mut self, raw: &[f32], disposition: ChunkDisposition) {
        if disposition == ChunkDisposition::Discard {
            return;
        }
        let policy = self.vad_policy;
        let (vad, on_audio, out) = (&mut self.vad, &self.on_audio, &mut self.processed_samples);
        self.frame_resampler
            .push(raw, |frame| handle_frame(frame, policy, vad, on_audio, out));
    }

    fn finish_recording(&mut self) -> Vec<f32> {
        let policy = self.vad_policy;
        let (vad, on_audio, out) = (&mut self.vad, &self.on_audio, &mut self.processed_samples);
        self.frame_resampler
            .finish(|frame| handle_frame(frame, policy, vad, on_audio, out));
        std::mem::take(&mut self.processed_samples)
    }
}

/// Route one 16 kHz frame through VAD to the recording and the live callback.
fn handle_frame(
    samples: &[f32],
    vad_policy: VadPolicy,
    vad: &mut Option<LaneVad>,
    on_audio: &AudioFrameCallback,
    out: &mut Vec<f32>,
) {
    let mut emit = |buf: &[f32]| {
        out.extend_from_slice(buf);
        on_audio(buf);
    };
    match (vad_policy, vad) {
        (VadPolicy::Disabled, _) | (_, None) => emit(samples),
        (_, Some(vad)) => match vad
            .detector
            .push_frame(samples)
            .unwrap_or(VadFrame::Speech(samples))
        {
            VadFrame::Speech(buf) => emit(buf),
            VadFrame::Noise => {}
        },
    }
}

/// An open system-audio lane. Its cpal stream lives on an owner thread for
/// the lane's whole life (cpal streams are not `Send` on every platform);
/// dropping the lane shuts it down.
pub struct SystemAudioLane {
    cmd_tx: mpsc::Sender<Cmd>,
    pump_tx: mpsc::Sender<LoopbackPumpCmd>,
    session: Arc<SystemAudioSession>,
    /// Set when the lane can no longer capture: its loopback stream reported
    /// a fatal error, or one of its threads stopped taking commands.
    error: Arc<AtomicBool>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    owner: Option<JoinHandle<()>>,
}

impl SystemAudioLane {
    /// Opens loopback capture on `capture`. An error leaves nothing running;
    /// callers record microphone-only.
    pub fn open(
        capture: SystemAudioCapture,
        vad: Option<LaneVad>,
        on_audio: AudioFrameCallback,
    ) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (pump_tx, pump_rx) = mpsc::channel::<LoopbackPumpCmd>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let session = Arc::new(SystemAudioSession::default());
        let error = Arc::new(AtomicBool::new(false));

        let owner = {
            let cmd_tx = cmd_tx.clone();
            let pump_tx = pump_tx.clone();
            let session = Arc::clone(&session);
            let error = Arc::clone(&error);
            std::thread::Builder::new()
                .name("audio-loopback-owner".to_string())
                .spawn(move || {
                    let (loopback_tx, loopback_rx) = mpsc::sync_channel::<LoopbackChunk>(16);
                    let (buffer_tx, buffer_rx) = mpsc::sync_channel::<Vec<f32>>(16);
                    for _ in 0..16 {
                        let _ = buffer_tx.try_send(Vec::with_capacity(4096));
                    }
                    let dropped = Arc::new(AtomicUsize::new(0));
                    let rate = Arc::new(AtomicU32::new(0));
                    let stream = match Self::build_loopback_stream(
                        &capture.device,
                        loopback_tx,
                        buffer_rx,
                        Arc::clone(&session),
                        Arc::clone(&error),
                        Arc::clone(&rate),
                        Arc::clone(&dropped),
                    ) {
                        Ok(stream) => stream,
                        Err(e) => {
                            let _ = init_tx.send(Err(e.to_string()));
                            return;
                        }
                    };
                    let sample_rate = match rate.load(Ordering::Acquire) {
                        0 => constants::WHISPER_SAMPLE_RATE,
                        sample_rate => sample_rate,
                    };
                    let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
                    let threads = match spawn_lane_threads(
                        sample_rate,
                        LaneProcessor::new(sample_rate, vad, on_audio),
                        sample_tx,
                        sample_rx,
                        cmd_rx,
                        loopback_rx,
                        buffer_tx,
                        pump_rx,
                        Arc::clone(&session),
                    ) {
                        Ok(threads) => threads,
                        Err(e) => {
                            let _ = init_tx.send(Err(e));
                            return;
                        }
                    };
                    let _ = init_tx.send(Ok(()));
                    // Until `close()` or a drop of the lane.
                    let _ = shutdown_rx.recv();
                    drop(stream);
                    threads.shut_down(&cmd_tx, &pump_tx);
                    let dropped = dropped.load(Ordering::Relaxed);
                    if dropped > 0 {
                        log::warn!("System-audio lane dropped {dropped} loopback samples");
                    }
                })
                .map_err(|e| format!("Failed to start loopback owner thread: {e}"))?
        };

        match init_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                cmd_tx,
                pump_tx,
                session,
                error,
                shutdown_tx: Some(shutdown_tx),
                owner: Some(owner),
            }),
            Ok(Err(e)) => {
                let _ = owner.join();
                Err(e)
            }
            Err(e) => {
                let _ = owner.join();
                Err(format!("Failed to initialize the loopback lane: {e}"))
            }
        }
    }

    /// Begins a recording session. A lane that can't start is flagged for a
    /// rebuild; the microphone's recording goes ahead without it.
    pub fn start(&self, policy: VadPolicy) {
        let generation = self.session.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.session.active.store(true, Ordering::SeqCst);
        if let Err(e) = self.pump_tx.send(LoopbackPumpCmd::StartSession(generation)) {
            log::warn!("Failed to start loopback pump; continuing microphone-only: {e}");
            self.error.store(true, Ordering::Release);
        }
        if let Err(e) = self.cmd_tx.send(Cmd::Start(policy)) {
            log::warn!("Failed to start system-audio consumer; continuing microphone-only: {e}");
            self.error.store(true, Ordering::Release);
        }
    }

    /// Ends the session and returns its 16 kHz audio. Empty if the lane
    /// failed or timed out; the lane is then flagged for a rebuild.
    pub fn stop(&self) -> Vec<f32> {
        // SeqCst pairs with the loopback callback's `callback_busy` store:
        // once the pump sees no callback in flight, every later callback sees
        // this session as stopped.
        self.session.active.store(false, Ordering::SeqCst);
        self.session.generation.fetch_add(1, Ordering::AcqRel);
        let (reply_tx, reply_rx) = mpsc::channel();
        if let Err(e) = self.cmd_tx.send(Cmd::Stop(reply_tx)) {
            log::warn!("Failed to stop system-audio consumer: {e}");
            self.error.store(true, Ordering::Release);
            return Vec::new();
        }
        if let Err(e) = self.pump_tx.send(LoopbackPumpCmd::EndSession) {
            log::warn!("Failed to end loopback pump session: {e}");
            self.error.store(true, Ordering::Release);
        }
        let samples = match reply_rx.recv_timeout(CONSUMER_STOP_TIMEOUT) {
            Ok(samples) => samples,
            Err(e) => {
                log::warn!("Timed out waiting for system-audio consumer stop: {e}");
                self.error.store(true, Ordering::Release);
                Vec::new()
            }
        };
        // The loopback error callback only sets the flag; report it here.
        if self.error.load(Ordering::Acquire) {
            log::warn!("System audio capture failed; it will be rebuilt before the next recording");
        }
        samples
    }

    /// True when the lane must be rebuilt before the next recording.
    pub fn needs_reopen(&self) -> bool {
        self.error.load(Ordering::Acquire)
            || self.owner.as_ref().is_some_and(|owner| owner.is_finished())
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

impl Drop for SystemAudioLane {
    fn drop(&mut self) {
        self.session.active.store(false, Ordering::SeqCst);
        self.session.generation.fetch_add(1, Ordering::AcqRel);
        self.shutdown_tx.take();
        if let Some(owner) = self.owner.take() {
            let _ = owner.join();
        }
    }
}

/// The system-audio consumer and loopback pump threads of one open lane.
struct LaneThreads {
    consumer: JoinHandle<()>,
    pump: JoinHandle<()>,
}

impl LaneThreads {
    /// Tell both threads to exit, then wait for them. The lane keeps its own
    /// senders, so dropped senders alone would not stop them.
    fn shut_down(self, cmd_tx: &mpsc::Sender<Cmd>, pump_tx: &mpsc::Sender<LoopbackPumpCmd>) {
        let _ = cmd_tx.send(Cmd::Shutdown);
        let _ = pump_tx.send(LoopbackPumpCmd::Shutdown);
        let _ = self.consumer.join();
        let _ = self.pump.join();
    }
}

/// Start the system-audio consumer and the loopback pump that feeds it.
#[allow(clippy::too_many_arguments)]
fn spawn_lane_threads(
    sample_rate: u32,
    processor: LaneProcessor,
    sample_tx: mpsc::Sender<AudioChunk>,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    loopback_rx: mpsc::Receiver<LoopbackChunk>,
    loopback_buffer_tx: mpsc::SyncSender<Vec<f32>>,
    pump_rx: mpsc::Receiver<LoopbackPumpCmd>,
    session: Arc<SystemAudioSession>,
) -> Result<LaneThreads, String> {
    let consumer = std::thread::Builder::new()
        .name("audio-loopback-consumer".to_string())
        .spawn(move || run_system_consumer(processor, sample_rx, cmd_rx))
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
        Ok(pump) => Ok(LaneThreads { consumer, pump }),
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

/// Consumer for the system-audio lane. Processing is [`LaneProcessor`]'s;
/// only the transport differs from the microphone's. Loopback arrives as
/// `AudioChunk`s from `run_loopback_pump`, and a stop drains up to the pump's
/// `EndOfStream` sentinel rather than to a ring pause acknowledgement.
fn run_system_consumer(
    mut processor: LaneProcessor,
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
                Cmd::Start(policy) => {
                    processor.begin_recording(policy);
                    recording = true;
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;

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

#[cfg(test)]
mod tests;
