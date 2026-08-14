use std::{
    io::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, Sample, SizedSample,
};

use crate::audio_toolkit::{
    audio::{AudioVisualiser, FrameResampler},
    constants,
    vad::{self, VadFrame},
    VoiceActivityDetector,
};

enum Cmd {
    /// Begin capturing. Carries the send timestamp so the consumer can log how
    /// long the command sat in the channel, plus a one-shot acknowledgement
    /// sent only after the first microphone sample chunk is processed.
    Start(VadPolicy, Instant, mpsc::Sender<()>),
    Stop(mpsc::Sender<Vec<f32>>),
    Shutdown,
}

#[cfg(windows)]
enum LoopbackPumpCmd {
    StartSession,
    EndSession,
}

enum AudioChunk {
    Samples(Vec<f32>),
    EndOfStream,
}

/// How long the loopback pump waits for real system audio before emitting an
/// equivalent run of silence.
///
/// This exists because `run_consumer` is driven entirely by its sample channel:
/// it only polls `cmd_rx` after a chunk arrives. A microphone satisfies that
/// implicitly — an open capture endpoint keeps delivering near-zero buffers every
/// device period — but WASAPI loopback goes completely silent on an idle render
/// endpoint. Without a pump, `Cmd::Stop` would never be observed and `stop()`
/// would block forever.
#[cfg(windows)]
const LOOPBACK_PUMP_INTERVAL_MS: usize = 10;

const CONSUMER_STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(windows)]
struct LoopbackChunk {
    samples: Vec<f32>,
    sample_rate: u32,
    session_generation: u64,
}

/// A Windows render endpoint to capture through WASAPI loopback.
#[cfg(windows)]
#[derive(Clone)]
pub struct SystemAudioCapture {
    pub device: Device,
}

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
    offline_hangover_frames: usize,
    streaming_hangover_frames: usize,
}

impl VadConfig {
    /// Post-speech hangover tail (in 30 ms frames) for the given policy.
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

/// The independently processed microphone and system-audio lanes returned by a
/// recording stop. Persistence continues to use `microphone` only.
pub struct RecordedAudio {
    pub microphone: Vec<f32>,
    pub system: Vec<f32>,
}

pub struct AudioRecorder {
    device: Option<Device>,
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    #[cfg(windows)]
    system_cmd_tx: Option<mpsc::Sender<Cmd>>,
    #[cfg(windows)]
    loopback_pump_tx: Option<mpsc::Sender<LoopbackPumpCmd>>,
    worker_handle: Option<std::thread::JoinHandle<()>>,
    vad: Option<VadConfig>,
    #[cfg(windows)]
    system_vad: Option<VadConfig>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    audio_cb: Option<AudioFrameCallback>,
    #[cfg(windows)]
    system_audio_cb: Option<AudioFrameCallback>,
    /// Which input channel to use. None = average all (original behavior).
    selected_channel: Option<usize>,
    /// Preferred stream config cached per device name. The two HAL property
    /// queries in `get_preferred_config` cost ~40-85ms per open (worse on
    /// USB/Bluetooth), which lands on the keypress->capture path in on-demand
    /// mode. Keyed by name so a system-default change misses naturally;
    /// cleared whenever an open fails so a stale rate/format self-heals on the
    /// caller's retry.
    config_cache: Arc<Mutex<Option<(String, cpal::SupportedStreamConfig)>>>,
    #[cfg(windows)]
    system_audio_session: Arc<SystemAudioSession>,
}

#[cfg(windows)]
#[derive(Default)]
struct SystemAudioSession {
    active: AtomicBool,
    generation: AtomicU64,
    stale_samples: AtomicUsize,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(AudioRecorder {
            device: None,
            cmd_tx: None,
            #[cfg(windows)]
            system_cmd_tx: None,
            #[cfg(windows)]
            loopback_pump_tx: None,
            worker_handle: None,
            vad: None,
            #[cfg(windows)]
            system_vad: None,
            level_cb: None,
            audio_cb: None,
            #[cfg(windows)]
            system_audio_cb: None,
            selected_channel: None,
            config_cache: Arc::new(Mutex::new(None)),
            #[cfg(windows)]
            system_audio_session: Arc::new(SystemAudioSession::default()),
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
        self.vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
            offline_hangover_frames,
            streaming_hangover_frames,
        });
        self
    }

    /// Attach an independent detector for the system-audio consumer. It must not
    /// share Silero recurrent state or smoothing counters with the microphone.
    #[cfg(windows)]
    pub fn with_system_vad(
        mut self,
        detector: Box<dyn VoiceActivityDetector>,
        offline_hangover_frames: usize,
        streaming_hangover_frames: usize,
    ) -> Self {
        self.system_vad = Some(VadConfig {
            detector: Arc::new(Mutex::new(detector)),
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

    #[cfg(windows)]
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
        #[cfg(windows)] system_audio: Option<SystemAudioCapture>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            if !self.is_capture_worker_dead() {
                return Ok(()); // already open
            }
            // The worker exited on its own (see `is_capture_worker_dead`). Reap
            // it so we rebuild the stream below instead of handing the caller
            // back a recorder whose channels are already closed.
            log::warn!("Capture worker exited; rebuilding microphone stream");
            let _ = self.close();
        }

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        #[cfg(windows)]
        let (system_sample_tx, system_sample_rx) = mpsc::channel::<AudioChunk>();
        #[cfg(windows)]
        let (system_cmd_tx, system_cmd_rx) = mpsc::channel::<Cmd>();
        #[cfg(windows)]
        let system_cmd_tx_for_worker = system_cmd_tx.clone();
        #[cfg(windows)]
        let (loopback_pump_tx, loopback_pump_rx) = mpsc::channel::<LoopbackPumpCmd>();
        let (init_tx, init_rx) = mpsc::sync_channel::<Result<bool, String>>(1);

        let host = crate::audio_toolkit::get_cpal_host();
        let device = match device {
            Some(dev) => dev,
            None => host
                .default_input_device()
                .ok_or_else(|| Error::new(std::io::ErrorKind::NotFound, "No input device found"))?,
        };

        let thread_device = device.clone();
        let vad = self.vad.clone();
        #[cfg(windows)]
        let system_vad = self.system_vad.clone();
        // Move the optional level callback into the worker thread
        let level_cb = self.level_cb.clone();
        // Move the optional real-time audio frame callback into the worker thread
        let audio_cb = self.audio_cb.clone();
        #[cfg(windows)]
        let system_audio_cb = self.system_audio_cb.clone();
        let selected_channel = self.selected_channel;
        let config_cache = Arc::clone(&self.config_cache);
        #[cfg(windows)]
        let system_audio_session = Arc::clone(&self.system_audio_session);

        let worker = std::thread::spawn(move || {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_for_stream = stop_flag.clone();
            #[cfg(windows)]
            let (loopback_tx, loopback_rx) = mpsc::sync_channel::<LoopbackChunk>(16);
            #[cfg(windows)]
            let (loopback_buffer_tx, loopback_buffer_rx) = mpsc::sync_channel::<Vec<f32>>(16);
            #[cfg(windows)]
            for _ in 0..16 {
                let _ = loopback_buffer_tx.try_send(Vec::with_capacity(4096));
            }
            #[cfg(windows)]
            let dropped_loopback_samples = Arc::new(AtomicUsize::new(0));
            #[cfg(windows)]
            let loopback_available = Arc::new(AtomicBool::new(false));
            #[cfg(windows)]
            let loopback_sample_rate = Arc::new(AtomicU32::new(0));
            let init_result = (|| -> Result<_, String> {
                let config_started = Instant::now();
                let device_name = thread_device.name().unwrap_or_default();
                let cached_config = config_cache
                    .lock()
                    .unwrap()
                    .as_ref()
                    .filter(|(name, _)| !device_name.is_empty() && *name == device_name)
                    .map(|(_, cfg)| cfg.clone());
                let config_was_cached = cached_config.is_some();
                let config = match cached_config {
                    Some(cfg) => cfg,
                    None => AudioRecorder::get_preferred_config(&thread_device)
                        .map_err(|e| format!("Failed to fetch preferred config: {e}"))?,
                };
                let config_elapsed = config_started.elapsed();

                let sample_rate = config.sample_rate().0;
                let channels = config.channels() as usize;

                log::info!(
                    "Using device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
                    thread_device.name(),
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
                let stream = match config.sample_format() {
                    cpal::SampleFormat::U8 => AudioRecorder::build_stream::<u8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I8 => AudioRecorder::build_stream::<i8>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I16 => AudioRecorder::build_stream::<i16>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::I32 => AudioRecorder::build_stream::<i32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    cpal::SampleFormat::F32 => AudioRecorder::build_stream::<f32>(
                        &thread_device,
                        &config,
                        sample_tx,
                        channels,
                        selected_channel,
                        stop_flag_for_stream,
                    )
                    .map_err(|e| format!("Failed to build input stream: {e}"))?,
                    sample_format => {
                        return Err(format!("Unsupported sample format: {sample_format:?}"));
                    }
                };
                let build_elapsed = build_started.elapsed();

                let play_started = Instant::now();
                stream
                    .play()
                    .map_err(|e| format!("Failed to start microphone stream: {e}"))?;

                #[cfg(windows)]
                let loopback_stream = system_audio.and_then(|capture| {
                    match AudioRecorder::build_loopback_stream(
                        &capture.device,
                        loopback_tx,
                        loopback_buffer_rx,
                        Arc::clone(&system_audio_session),
                        Arc::clone(&loopback_available),
                        Arc::clone(&loopback_sample_rate),
                        Arc::clone(&dropped_loopback_samples),
                    ) {
                        Ok(stream) => Some(stream),
                        Err(error) => {
                            log::warn!(
                                "System audio capture unavailable; continuing microphone-only: {error}"
                            );
                            None
                        }
                    }
                });
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

                #[cfg(windows)]
                return Ok((stream, loopback_stream, sample_rate));
                #[cfg(not(windows))]
                Ok((stream, sample_rate))
            })();

            match init_result {
                #[cfg(windows)]
                Ok((stream, loopback_stream, sample_rate)) => {
                    let system_sample_rate = match loopback_sample_rate.load(Ordering::Acquire) {
                        0 => constants::WHISPER_SAMPLE_RATE,
                        sample_rate => sample_rate,
                    };
                    let (system_consumer, loopback_pump) = if loopback_stream.is_some() {
                        let system_stream_running_at = Instant::now();
                        let system_stop_flag = Arc::new(AtomicBool::new(false));
                        let system_consumer = std::thread::Builder::new()
                            .name("audio-loopback-consumer".to_string())
                            .spawn(move || {
                                run_consumer(
                                    system_sample_rate,
                                    system_vad,
                                    system_sample_rx,
                                    system_cmd_rx,
                                    None,
                                    system_audio_cb,
                                    system_stop_flag,
                                    system_stream_running_at,
                                );
                            });
                        let system_consumer = match system_consumer {
                            Ok(handle) => handle,
                            Err(error) => {
                                let error_message =
                                    format!("Failed to start loopback consumer thread: {error}");
                                let _ = init_tx.send(Err(error_message));
                                return;
                            }
                        };
                        let loopback_pump = std::thread::Builder::new()
                            .name("audio-loopback-pump".to_string())
                            .spawn(move || {
                                run_loopback_pump(
                                    loopback_rx,
                                    loopback_buffer_tx,
                                    system_sample_tx,
                                    loopback_pump_rx,
                                    system_audio_session,
                                    system_sample_rate,
                                );
                            });
                        let loopback_pump = match loopback_pump {
                            Ok(handle) => handle,
                            Err(error) => {
                                let error_message =
                                    format!("Failed to start loopback pump thread: {error}");
                                let _ = init_tx.send(Err(error_message));
                                let _ = system_consumer.join();
                                return;
                            }
                        };
                        (Some(system_consumer), Some(loopback_pump))
                    } else {
                        (None, None)
                    };
                    let _ = init_tx.send(Ok(loopback_stream.is_some()));
                    let stream_running_at = Instant::now();
                    run_consumer(
                        sample_rate,
                        vad,
                        sample_rx,
                        cmd_rx,
                        level_cb,
                        audio_cb,
                        stop_flag,
                        stream_running_at,
                    );
                    if system_consumer.is_some() {
                        let _ = system_cmd_tx_for_worker.send(Cmd::Shutdown);
                    }
                    drop(loopback_stream);
                    drop(stream);
                    if let Some(handle) = system_consumer {
                        let _ = handle.join();
                    }
                    if let Some(handle) = loopback_pump {
                        let _ = handle.join();
                    }
                }
                #[cfg(not(windows))]
                Ok((stream, sample_rate)) => {
                    let _ = init_tx.send(Ok(false));
                    // Timestamp for the play()-returned -> first-samples gap the
                    // init handshake can't see (hardware dependent).
                    let stream_running_at = Instant::now();
                    // Keep the stream alive while we process samples.
                    run_consumer(
                        sample_rate,
                        vad,
                        sample_rx,
                        cmd_rx,
                        level_cb,
                        audio_cb,
                        stop_flag,
                        stream_running_at,
                    );
                    drop(stream);
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
            Ok(Ok(system_audio_active)) => {
                #[cfg(not(windows))]
                let _ = system_audio_active;
                self.device = Some(device);
                self.cmd_tx = Some(cmd_tx);
                #[cfg(windows)]
                {
                    self.system_cmd_tx = system_audio_active.then_some(system_cmd_tx);
                    self.loopback_pump_tx = system_audio_active.then_some(loopback_pump_tx);
                }
                self.worker_handle = Some(worker);
                Ok(())
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
        #[cfg(windows)]
        {
            self.system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel);
            self.system_audio_session
                .active
                .store(true, Ordering::Release);
        }
        #[cfg(windows)]
        if let Some(pump_tx) = &self.loopback_pump_tx {
            pump_tx.send(LoopbackPumpCmd::StartSession)?;
        }
        #[cfg(windows)]
        if let Some(system_tx) = &self.system_cmd_tx {
            let (system_ready_tx, _system_ready_rx) = mpsc::channel();
            if let Err(error) =
                system_tx.send(Cmd::Start(vad_policy, Instant::now(), system_ready_tx))
            {
                log::warn!(
                    "Failed to start system-audio consumer; continuing microphone-only: {error}"
                );
            }
        }
        if let Err(error) = tx.send(Cmd::Start(vad_policy, Instant::now(), ready_tx)) {
            #[cfg(windows)]
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
        #[cfg(windows)]
        {
            self.system_audio_session
                .active
                .store(false, Ordering::Release);
            self.system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        #[cfg(windows)]
        let system_response = if let Some(system_tx) = &self.system_cmd_tx {
            let (system_resp_tx, system_resp_rx) = mpsc::channel();
            if let Err(error) = system_tx.send(Cmd::Stop(system_resp_tx)) {
                log::warn!("Failed to stop system-audio consumer: {error}");
                None
            } else {
                if let Some(pump_tx) = &self.loopback_pump_tx {
                    if let Err(error) = pump_tx.send(LoopbackPumpCmd::EndSession) {
                        log::warn!("Failed to end loopback pump session: {error}");
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
        #[cfg(windows)]
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
        #[cfg(not(windows))]
        let system = Vec::new();

        Ok(RecordedAudio { microphone, system })
    }

    /// True once the capture worker has exited without anyone calling `close`.
    ///
    /// `run_consumer` is driven entirely by the sample channel, so when cpal
    /// tears the stream down mid-session (device unplugged, USB/Bluetooth
    /// dropout) `sample_rx.recv()` returns `Err`, the loop ends and the worker
    /// thread finishes. `cmd_tx` and `worker_handle` are still populated at
    /// that point, so the recorder looks open from the outside while every
    /// command sent to it fails on a closed channel.
    pub fn is_capture_worker_dead(&self) -> bool {
        self.worker_handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
    }

    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(windows)]
        {
            self.system_audio_session
                .active
                .store(false, Ordering::Release);
            self.system_audio_session
                .generation
                .fetch_add(1, Ordering::AcqRel);
        }
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        #[cfg(windows)]
        if let Some(tx) = self.system_cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        #[cfg(windows)]
        {
            self.loopback_pump_tx.take();
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        self.device = None;
        Ok(())
    }

    // The cfg-specific channel ownership is clearest when kept explicit at the stream boundary.
    #[allow(clippy::too_many_arguments)]
    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        sample_tx: mpsc::Sender<AudioChunk>,
        channels: usize,
        selected_channel: Option<usize>,
        stop_flag: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let mut output_buffer = Vec::new();
        let mut eos_sent = false;
        // Resolve the effective channel to use. If the selected channel is
        // out of range for this device, fall back to averaging all channels.
        let use_channel: Option<usize> = match selected_channel {
            Some(ch) if ch < channels => Some(ch),
            Some(_) => None, // out of range, fall back to average
            None => None,    // user chose "average all"
        };

        let stream_cb = move |data: &[T], _: &cpal::InputCallbackInfo| {
            if stop_flag.load(Ordering::Relaxed) {
                if !eos_sent {
                    let _ = sample_tx.send(AudioChunk::EndOfStream);
                    eos_sent = true;
                }
                return;
            }
            eos_sent = false;

            output_buffer.clear();

            if channels == 1 {
                output_buffer.extend(data.iter().map(|&sample| sample.to_sample::<f32>()));
            } else {
                let frame_count = data.len() / channels;
                output_buffer.reserve(frame_count);

                if let Some(ch) = use_channel {
                    for frame in data.chunks_exact(channels) {
                        let mono_sample = frame[ch].to_sample::<f32>();
                        output_buffer.push(mono_sample);
                    }
                } else {
                    for frame in data.chunks_exact(channels) {
                        let mono_sample = frame
                            .iter()
                            .map(|&sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / channels as f32;
                        output_buffer.push(mono_sample);
                    }
                }
            }

            if sample_tx
                .send(AudioChunk::Samples(output_buffer.clone()))
                .is_err()
            {
                log::error!("Failed to send samples");
            }
        };

        device.build_input_stream(
            &config.clone().into(),
            stream_cb,
            |err| log::error!("Stream error: {}", err),
            None,
        )
    }

    #[cfg(windows)]
    fn build_loopback_stream(
        device: &cpal::Device,
        loopback_tx: mpsc::SyncSender<LoopbackChunk>,
        loopback_buffer_rx: mpsc::Receiver<Vec<f32>>,
        session: Arc<SystemAudioSession>,
        available: Arc<AtomicBool>,
        shared_sample_rate: Arc<AtomicU32>,
        dropped_samples: Arc<AtomicUsize>,
    ) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
        let config = Self::get_preferred_loopback_config(device)?;
        let channels = usize::from(config.channels());
        let sample_rate = config.sample_rate().0;
        shared_sample_rate.store(sample_rate, Ordering::Release);
        log::info!(
            "Using system audio device: {:?}\nSample rate: {}\nChannels: {}\nFormat: {:?}",
            device.name(),
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
                Arc::clone(&available),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::I16 => Self::build_loopback_stream_typed::<i16>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&available),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::I32 => Self::build_loopback_stream_typed::<i32>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&available),
                Arc::clone(&dropped_samples),
            )?,
            cpal::SampleFormat::F32 => Self::build_loopback_stream_typed::<f32>(
                device,
                &config,
                loopback_tx,
                loopback_buffer_rx,
                session,
                Arc::clone(&available),
                Arc::clone(&dropped_samples),
            )?,
            sample_format => {
                return Err(Box::new(Error::other(format!(
                    "Unsupported loopback sample format: {sample_format:?}"
                ))));
            }
        };

        stream.play()?;
        available.store(true, Ordering::Release);
        Ok(stream)
    }

    #[cfg(windows)]
    fn build_loopback_stream_typed<T>(
        device: &cpal::Device,
        config: &cpal::SupportedStreamConfig,
        loopback_tx: mpsc::SyncSender<LoopbackChunk>,
        loopback_buffer_rx: mpsc::Receiver<Vec<f32>>,
        session: Arc<SystemAudioSession>,
        available: Arc<AtomicBool>,
        dropped_samples: Arc<AtomicUsize>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: Sample + SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let channels = usize::from(config.channels());
        let sample_rate = config.sample_rate().0;
        let mut emergency_buffer = Some(Vec::with_capacity(4096));
        let dropped_samples_for_callback = dropped_samples;

        let data_callback = move |data: &[T], _: &cpal::InputCallbackInfo| {
            let session_generation = session.generation.load(Ordering::Acquire);
            if !session.active.load(Ordering::Acquire) {
                return;
            }

            let Some(mut mono_buffer) = emergency_buffer
                .take()
                .or_else(|| loopback_buffer_rx.try_recv().ok())
            else {
                dropped_samples_for_callback.fetch_add(data.len() / channels, Ordering::Relaxed);
                return;
            };
            downmix_loopback(data, channels, &mut mono_buffer);
            let sample_count = mono_buffer.len();
            if session.generation.load(Ordering::Acquire) != session_generation {
                session
                    .stale_samples
                    .fetch_add(sample_count, Ordering::Relaxed);
                mono_buffer.clear();
                emergency_buffer = Some(mono_buffer);
                return;
            }
            let chunk = LoopbackChunk {
                samples: mono_buffer,
                sample_rate,
                session_generation,
            };
            match loopback_tx.try_send(chunk) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(chunk)) => {
                    dropped_samples_for_callback.fetch_add(sample_count, Ordering::Relaxed);
                    emergency_buffer = Some(chunk.samples);
                }
                Err(mpsc::TrySendError::Disconnected(chunk)) => {
                    emergency_buffer = Some(chunk.samples);
                }
            }
        };

        let error_available = Arc::clone(&available);
        device.build_input_stream(
            &config.clone().into(),
            data_callback,
            move |error| {
                error_available.store(false, Ordering::Release);
                log::warn!("System audio stream error; continuing microphone-only: {error}");
            },
            None,
        )
    }

    #[cfg(windows)]
    fn get_preferred_loopback_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // WASAPI render endpoints reject input-config enumeration even though
        // cpal can open them for shared-mode loopback. Their output default is
        // therefore the authoritative loopback format.
        Ok(device.default_output_config()?)
    }

    pub fn preferred_input_channel_count(
        device: &cpal::Device,
    ) -> Result<u16, Box<dyn std::error::Error>> {
        Ok(Self::get_preferred_config(device)?.channels())
    }

    fn get_preferred_config(
        device: &cpal::Device,
    ) -> Result<cpal::SupportedStreamConfig, Box<dyn std::error::Error>> {
        // Use the device's native/default sample rate and let the FrameResampler
        // in run_consumer() downsample to 16kHz. This avoids forcing hardware into
        // a non-native rate which can cause issues on some devices (Bluetooth
        // codecs, certain ALSA drivers, etc.).
        let default_config = device.default_input_config()?;
        let target_rate = default_config.sample_rate();

        // Try to find the best sample format at the device's default rate
        let supported_configs = match device.supported_input_configs() {
            Ok(configs) => configs,
            Err(e) => {
                log::warn!("Could not enumerate input configs ({e}), using device default");
                return Ok(default_config);
            }
        };
        let mut best_config: Option<cpal::SupportedStreamConfigRange> = None;

        for config_range in supported_configs {
            if config_range.min_sample_rate() <= target_rate
                && config_range.max_sample_rate() >= target_rate
            {
                match best_config {
                    None => best_config = Some(config_range),
                    Some(ref current) => {
                        // Prioritize F32 > I16 > I32 > others
                        let score = |fmt: cpal::SampleFormat| match fmt {
                            cpal::SampleFormat::F32 => 4,
                            cpal::SampleFormat::I16 => 3,
                            cpal::SampleFormat::I32 => 2,
                            _ => 1,
                        };

                        if score(config_range.sample_format()) > score(current.sample_format()) {
                            best_config = Some(config_range);
                        }
                    }
                }
            }
        }

        if let Some(config) = best_config {
            return Ok(config.with_sample_rate(target_rate));
        }

        // Fall back to device default if no config matched (exotic/virtual devices)
        log::warn!(
            "No supported config matched device default rate {:?}, using default config",
            target_rate
        );
        Ok(default_config)
    }
}

#[cfg(windows)]
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

#[cfg(windows)]
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
        match control_rx.recv() {
            Ok(LoopbackPumpCmd::StartSession) => {}
            Ok(LoopbackPumpCmd::EndSession) => continue,
            Err(_) => return,
        }
        let mut next_tick = Instant::now() + pump_interval;

        'session: loop {
            while let Ok(command) = control_rx.try_recv() {
                match command {
                    LoopbackPumpCmd::StartSession => {
                        next_tick = Instant::now() + pump_interval;
                    }
                    LoopbackPumpCmd::EndSession => {
                        // Wake `run_consumer` after Cmd::Stop is queued, then place
                        // exactly one sentinel behind that wake-up chunk for its drain.
                        if sample_tx
                            .send(AudioChunk::Samples(vec![0.0; silence_len]))
                            .is_err()
                            || sample_tx.send(AudioChunk::EndOfStream).is_err()
                        {
                            return;
                        }
                        break 'session;
                    }
                }
            }

            let now = Instant::now();
            let wait = next_tick.saturating_duration_since(now);
            let receive_result = if loopback_connected {
                loopback_rx.recv_timeout(wait)
            } else {
                match control_rx.recv_timeout(wait) {
                    Ok(LoopbackPumpCmd::EndSession) => {
                        if sample_tx
                            .send(AudioChunk::Samples(vec![0.0; silence_len]))
                            .is_err()
                            || sample_tx.send(AudioChunk::EndOfStream).is_err()
                        {
                            return;
                        }
                        break 'session;
                    }
                    Ok(LoopbackPumpCmd::StartSession) => {
                        next_tick = Instant::now() + pump_interval;
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => Err(mpsc::RecvTimeoutError::Timeout),
                }
            };

            match receive_result {
                Ok(mut chunk) => {
                    let current_generation = session.generation.load(Ordering::Acquire);
                    if session.active.load(Ordering::Acquire)
                        && chunk.session_generation == current_generation
                    {
                        if chunk.sample_rate != sample_rate {
                            log::warn!(
                            "Loopback sample rate changed from {sample_rate} to {}; dropping packet",
                            chunk.sample_rate
                        );
                            chunk.samples.clear();
                            let _ = loopback_buffer_tx.try_send(chunk.samples);
                            continue;
                        }
                        let sample_count = chunk.samples.len();
                        let samples = std::mem::take(&mut chunk.samples);
                        if sample_tx.send(AudioChunk::Samples(samples)).is_err() {
                            return;
                        }
                        next_tick +=
                            Duration::from_secs_f64(sample_count as f64 / sample_rate as f64);
                    } else if chunk.session_generation != current_generation {
                        session
                            .stale_samples
                            .fetch_add(chunk.samples.len(), Ordering::Relaxed);
                    }

                    chunk.samples.clear();
                    let _ = loopback_buffer_tx.try_send(chunk.samples);
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

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::{
        downmix_loopback, run_consumer, run_loopback_pump, Cmd, LoopbackChunk, LoopbackPumpCmd,
        SystemAudioSession, VadPolicy,
    };
    use super::{is_microphone_access_denied, is_no_input_device_error, AudioRecorder};
    #[cfg(windows)]
    use std::sync::{atomic::Ordering, mpsc, Arc};
    #[cfg(windows)]
    use std::time::{Duration, Instant};

    #[cfg(windows)]
    struct LoopbackHarness {
        raw_tx: Option<mpsc::SyncSender<LoopbackChunk>>,
        cmd_tx: mpsc::Sender<Cmd>,
        pump_tx: mpsc::Sender<LoopbackPumpCmd>,
        session: Arc<SystemAudioSession>,
        consumer_done: mpsc::Receiver<()>,
        pump_done: mpsc::Receiver<()>,
    }

    #[cfg(windows)]
    impl LoopbackHarness {
        fn new() -> Self {
            let (raw_tx, raw_rx) = mpsc::sync_channel(4);
            let (buffer_tx, _buffer_rx) = mpsc::sync_channel(4);
            let (sample_tx, sample_rx) = mpsc::channel();
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let (pump_tx, pump_rx) = mpsc::channel();
            let session = Arc::new(SystemAudioSession::default());
            let (consumer_done_tx, consumer_done) = mpsc::channel();
            let consumer = std::thread::spawn(move || {
                run_consumer(
                    16_000,
                    None,
                    sample_rx,
                    cmd_rx,
                    None,
                    None,
                    Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    Instant::now(),
                );
                let _ = consumer_done_tx.send(());
            });
            let pump_session = Arc::clone(&session);
            let (pump_done_tx, pump_done) = mpsc::channel();
            std::thread::spawn(move || {
                run_loopback_pump(raw_rx, buffer_tx, sample_tx, pump_rx, pump_session, 16_000);
                let _ = pump_done_tx.send(());
                let _ = consumer.join();
            });

            Self {
                raw_tx: Some(raw_tx),
                cmd_tx,
                pump_tx,
                session,
                consumer_done,
                pump_done,
            }
        }

        fn start(&self) -> mpsc::Receiver<()> {
            self.session.generation.fetch_add(1, Ordering::AcqRel);
            self.session.active.store(true, Ordering::Release);
            self.pump_tx
                .send(LoopbackPumpCmd::StartSession)
                .expect("pump start session");
            let (ready_tx, ready_rx) = mpsc::channel();
            self.cmd_tx
                .send(Cmd::Start(VadPolicy::Disabled, Instant::now(), ready_tx))
                .expect("start command");
            ready_rx
        }

        fn send(&self, samples: Vec<f32>, generation: u64) {
            self.raw_tx
                .as_ref()
                .expect("loopback sender")
                .send(LoopbackChunk {
                    samples,
                    sample_rate: 16_000,
                    session_generation: generation,
                })
                .expect("loopback samples");
        }

        fn fail_device(&mut self) {
            self.raw_tx.take();
        }

        fn stop(&self) -> Vec<f32> {
            self.session.active.store(false, Ordering::Release);
            self.session.generation.fetch_add(1, Ordering::AcqRel);
            let (reply_tx, reply_rx) = mpsc::channel();
            self.cmd_tx.send(Cmd::Stop(reply_tx)).expect("stop command");
            self.pump_tx
                .send(LoopbackPumpCmd::EndSession)
                .expect("pump end session");
            reply_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("stop must not hang")
        }

        fn shutdown(self) {
            let Self {
                raw_tx,
                cmd_tx,
                pump_tx,
                consumer_done,
                pump_done,
                ..
            } = self;
            let _ = cmd_tx.send(Cmd::Shutdown);
            drop(raw_tx);
            drop(pump_tx);
            consumer_done
                .recv_timeout(Duration::from_secs(1))
                .expect("consumer shutdown must not hang");
            pump_done
                .recv_timeout(Duration::from_secs(1))
                .expect("pump shutdown must not hang");
        }
    }

    #[test]
    fn unopened_recorder_is_not_reported_dead() {
        // No worker has been spawned yet, so there is nothing to reap. Guards
        // against inverting the "no worker" case, which would make every first
        // open() take the rebuild path.
        let recorder = AudioRecorder::new().expect("recorder");
        assert!(!recorder.is_capture_worker_dead());
    }

    #[cfg(windows)]
    #[test]
    fn disabled_recorder_has_no_loopback_runtime_resources() {
        let recorder = AudioRecorder::new().expect("recorder");
        assert!(recorder.system_vad.is_none(), "no second VAD");
        assert!(recorder.system_cmd_tx.is_none(), "no second consumer");
        assert!(recorder.loopback_pump_tx.is_none(), "no pump thread");
    }

    #[test]
    fn detects_access_is_denied() {
        assert!(is_microphone_access_denied("Access is denied"));
    }

    #[test]
    fn detects_permission_denied() {
        assert!(is_microphone_access_denied("permission denied"));
    }

    #[test]
    fn detects_windows_error_code() {
        assert!(is_microphone_access_denied("WASAPI error: 0x80070005"));
    }

    #[test]
    fn does_not_match_unrelated_errors() {
        assert!(!is_microphone_access_denied("device not found"));
    }

    #[test]
    fn detects_no_input_device() {
        assert!(is_no_input_device_error("No input device found"));
    }

    #[test]
    fn detects_coreaudio_config_error() {
        assert!(is_no_input_device_error(
            "Failed to fetch preferred config: A backend-specific error has occurred: An unknown error unknown to the coreaudio-rs API occurred"
        ));
    }

    #[test]
    fn does_not_match_other_errors_for_no_device() {
        assert!(!is_no_input_device_error("permission denied"));
        assert!(!is_no_input_device_error("device not found"));
    }

    #[cfg(windows)]
    #[test]
    fn surround_downmix_preserves_center_and_ignores_lfe() {
        let mut mono = Vec::new();
        downmix_loopback(&[0.0_f32, 0.0, 1.0, 1.0, 0.0, 0.0], 6, &mut mono);
        assert_eq!(mono.len(), 1);
        assert!((mono[0] - 0.4).abs() < 1e-6);
    }

    #[cfg(windows)]
    #[test]
    fn stop_completes_with_zero_loopback_callbacks() {
        let harness = LoopbackHarness::new();
        harness
            .start()
            .recv_timeout(Duration::from_secs(1))
            .expect("silence pump should make capture ready");
        let _ = harness.stop();
        harness.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn stop_completes_when_loopback_fails_before_first_sample() {
        let mut harness = LoopbackHarness::new();
        harness.fail_device();
        harness.start();
        let _ = harness.stop();
        harness.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn stop_completes_when_loopback_fails_mid_recording() {
        let mut harness = LoopbackHarness::new();
        harness.start();
        let generation = harness.session.generation.load(Ordering::Acquire);
        harness.send(vec![0.5; 480], generation);
        harness.fail_device();
        let _ = harness.stop();
        harness.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn stop_immediately_after_start_completes() {
        let harness = LoopbackHarness::new();
        harness.start();
        let _ = harness.stop();
        harness.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn shutdown_while_loopback_is_silent_completes() {
        LoopbackHarness::new().shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn pump_is_idle_until_a_session_starts() {
        let (_raw_tx, raw_rx) = mpsc::sync_channel(1);
        let (buffer_tx, _buffer_rx) = mpsc::sync_channel(1);
        let (sample_tx, sample_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let session = Arc::new(SystemAudioSession::default());
        let pump = std::thread::spawn(move || {
            run_loopback_pump(raw_rx, buffer_tx, sample_tx, control_rx, session, 16_000)
        });

        assert!(sample_rx.recv_timeout(Duration::from_millis(30)).is_err());
        drop(control_tx);
        pump.join().expect("pump exits when control closes");
    }

    #[cfg(windows)]
    #[test]
    fn bursty_packets_follow_elapsed_time_without_phantom_ticks() {
        let (raw_tx, raw_rx) = mpsc::sync_channel(8);
        let (buffer_tx, _buffer_rx) = mpsc::sync_channel(8);
        let (sample_tx, sample_rx) = mpsc::channel();
        let (control_tx, control_rx) = mpsc::channel();
        let session = Arc::new(SystemAudioSession::default());
        session.active.store(true, Ordering::Release);
        session.generation.store(1, Ordering::Release);
        let pump_session = Arc::clone(&session);
        let pump = std::thread::spawn(move || {
            run_loopback_pump(
                raw_rx,
                buffer_tx,
                sample_tx,
                control_rx,
                pump_session,
                16_000,
            )
        });
        control_tx
            .send(LoopbackPumpCmd::StartSession)
            .expect("start pump");
        // Deliver 20 packets (200 ms of audio) back-to-back in a few ms. This is
        // the shape WASAPI loopback actually uses, and it is what separates the two
        // designs: a deadline-driven pump advances `next_tick` by each packet's
        // duration, so the audio timeline runs ahead of the wall clock and almost
        // no silence is owed. A timeout-driven pump adds a full tick per timeout
        // regardless, inflating the stream well past the real audio.
        const BURST_PACKETS: usize = 20;
        const PACKET_SAMPLES: usize = 160;
        const REAL_SAMPLES: usize = BURST_PACKETS * PACKET_SAMPLES;
        for _ in 0..BURST_PACKETS {
            raw_tx
                .send(LoopbackChunk {
                    samples: vec![0.5; PACKET_SAMPLES],
                    sample_rate: 16_000,
                    session_generation: 1,
                })
                .expect("burst packet");
        }
        let mut sample_count = 0;
        while sample_count < REAL_SAMPLES {
            if let super::AudioChunk::Samples(samples) = sample_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("real burst output")
            {
                sample_count += samples.len();
            }
        }
        control_tx
            .send(LoopbackPumpCmd::EndSession)
            .expect("end pump");

        while let super::AudioChunk::Samples(samples) = sample_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("pump output")
        {
            sample_count += samples.len();
        }

        // Every real sample must survive the burst — none may be dropped or
        // displaced by synthetic silence.
        assert_eq!(
            sample_count.min(REAL_SAMPLES),
            REAL_SAMPLES,
            "real burst audio was lost: sample_count={sample_count}"
        );

        // Silence is asserted as a bounded ALLOWANCE rather than an exact tick
        // count, because the pump blocks on the loopback channel: at stop it waits
        // out the remaining deadline, emits one catch-up tick, then emits the
        // deliberate EndSession wake-up chunk. A startup race can add one more.
        // Four ticks is generous for those; a timeout-driven pump would add on the
        // order of one tick per packet across a 20-packet burst and blow past it.
        let silence = sample_count - REAL_SAMPLES;
        let allowance = 4 * PACKET_SAMPLES;
        assert!(
            silence <= allowance,
            "pump injected phantom silence during a back-to-back burst: \
             silence={silence} samples ({} ticks) exceeds allowance={allowance}; \
             sample_count={sample_count}, real={REAL_SAMPLES}",
            silence / PACKET_SAMPLES
        );
        drop(control_tx);
        pump.join().expect("pump exits");
    }

    #[cfg(windows)]
    #[test]
    fn real_recorder_stop_completes_while_loopback_packets_continue() {
        let harness = LoopbackHarness::new();
        harness.start();
        let generation = harness.session.generation.load(Ordering::Acquire);
        let raw_tx = harness.raw_tx.as_ref().expect("raw sender").clone();
        let producing = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let producer_flag = Arc::clone(&producing);
        let producer = std::thread::spawn(move || {
            while producer_flag.load(Ordering::Acquire) {
                let _ = raw_tx.try_send(LoopbackChunk {
                    samples: vec![0.25; 160],
                    sample_rate: 16_000,
                    session_generation: generation,
                });
                std::thread::yield_now();
            }
        });

        let (mic_tx, mic_rx) = mpsc::channel();
        let mic_worker = std::thread::spawn(move || {
            while let Ok(command) = mic_rx.recv() {
                match command {
                    Cmd::Stop(reply) => {
                        let _ = reply.send(vec![0.75; 160]);
                    }
                    Cmd::Shutdown => break,
                    Cmd::Start(..) => {}
                }
            }
        });
        let mut recorder = AudioRecorder::new().expect("recorder");
        recorder.cmd_tx = Some(mic_tx.clone());
        recorder.system_cmd_tx = Some(harness.cmd_tx.clone());
        recorder.loopback_pump_tx = Some(harness.pump_tx.clone());
        recorder.system_audio_session = Arc::clone(&harness.session);

        let recorded = recorder.stop().expect("real stop must complete");
        assert_eq!(recorded.microphone, vec![0.75; 160]);
        producing.store(false, Ordering::Release);
        producer.join().expect("producer exits");
        let _ = mic_tx.send(Cmd::Shutdown);
        mic_worker.join().expect("mic worker exits");
        drop(recorder);
        harness.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn immediate_stop_start_drops_stale_loopback_audio() {
        let harness = LoopbackHarness::new();
        harness.start();
        let first_generation = harness.session.generation.load(Ordering::Acquire);
        harness.send(vec![0.75; 480], first_generation);
        let _ = harness.stop();

        harness.start();
        let current_generation = harness.session.generation.load(Ordering::Acquire);
        harness.send(vec![0.75; 480], first_generation);
        harness.send(vec![0.25; 480], current_generation);
        std::thread::sleep(Duration::from_millis(50));
        let second = harness.stop();
        assert!(second.iter().any(|sample| (*sample - 0.25).abs() < 1e-6));
        assert!(!second.iter().any(|sample| (*sample - 0.75).abs() < 1e-6));
        harness.shutdown();
    }
}

#[allow(clippy::too_many_arguments)]
fn run_consumer(
    in_sample_rate: u32,
    vad: Option<VadConfig>,
    sample_rx: mpsc::Receiver<AudioChunk>,
    cmd_rx: mpsc::Receiver<Cmd>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    audio_cb: Option<AudioFrameCallback>,
    stop_flag: Arc<AtomicBool>,
    stream_running_at: Instant,
) {
    let mut frame_resampler = FrameResampler::new(
        in_sample_rate as usize,
        constants::WHISPER_SAMPLE_RATE as usize,
        Duration::from_millis(30),
    );

    let mut processed_samples = Vec::<f32>::new();
    let mut recording = false;
    let mut vad_policy = VadPolicy::Offline;

    // ---------- latency instrumentation ---------------------------------- //
    // First-chunk arrival exposes the play()->samples-flowing gap; the
    // first-captured log confirms capture begins with the chunk in flight
    // when Cmd::Start lands.
    let mut first_chunk_logged = false;
    let mut awaiting_first_captured_chunk: Option<Instant> = None;
    let mut capture_ready_tx: Option<mpsc::Sender<()>> = None;

    // ---------- spectrum visualisation setup ---------------------------- //
    const BUCKETS: usize = 16;
    // Scale the FFT window to the device sample rate so the analysis window
    // (~33 ms) and frequency resolution (~30 Hz/bin) stay roughly constant
    // across devices. A fixed 512-sample window collapses the low vocal
    // buckets onto a single bin at 48 kHz (e.g. built-in laptop mics), and
    // would stutter at ~4-8 updates/sec on an 8-16 kHz Bluetooth headset.
    // Targets: 48 kHz -> 2048, 16 kHz -> 512, 8 kHz -> 256.
    let target_window = (f64::from(in_sample_rate) / 30.0).round() as usize;
    let window_size = [256usize, 512, 1024, 2048]
        .into_iter()
        .min_by_key(|w| w.abs_diff(target_window))
        .unwrap();
    let mut visualizer = AudioVisualiser::new(
        in_sample_rate,
        window_size,
        BUCKETS,
        400.0,  // vocal_min_hz
        4000.0, // vocal_max_hz
    );

    fn handle_frame(
        samples: &[f32],
        recording: bool,
        vad_policy: VadPolicy,
        vad: &Option<VadConfig>,
        audio_cb: &Option<AudioFrameCallback>,
        out_buf: &mut Vec<f32>,
    ) {
        if !recording {
            return;
        }

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
            let mut det = cfg.detector.lock().unwrap();
            match det.push_frame(samples).unwrap_or(VadFrame::Speech(samples)) {
                VadFrame::Speech(buf) => emit(buf),
                VadFrame::Noise => {}
            }
        } else {
            emit(samples);
        }
    }

    // Runs until the stream closes and `recv` returns `Err`.
    while let Ok(chunk) = sample_rx.recv() {
        // Handle pending commands BEFORE the in-flight chunk so a Start
        // captures it. Commands used to be polled after processing, which
        // silently dropped one buffer period of audio (~10ms built-in, up to
        // ~100ms on Bluetooth) at every recording start.
        let mut pending = Some(chunk);
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                Cmd::Start(policy, sent_at, ready_tx) => {
                    log::debug!(
                        "Cmd::Start processed {:?} after send; capture begins with the in-flight chunk",
                        sent_at.elapsed()
                    );
                    awaiting_first_captured_chunk = Some(Instant::now());
                    capture_ready_tx = Some(ready_tx);
                    stop_flag.store(false, Ordering::Relaxed);
                    vad_policy = policy;
                    processed_samples.clear();
                    recording = true;
                    visualizer.reset();
                    frame_resampler.reset();
                    // Reconfigure the single VAD engine for this session's policy
                    // and clear its smoothing + recurrent state before it sees
                    // any frames.
                    if vad_policy != VadPolicy::Disabled {
                        if let Some(cfg) = &vad {
                            let mut det = cfg.detector.lock().unwrap();
                            det.set_hangover_frames(cfg.hangover_for(vad_policy));
                            det.reset();
                        }
                    }
                }
                Cmd::Stop(reply_tx) => {
                    recording = false;
                    // If Stop was queued before the first chunk, dropping this
                    // sender prevents a stale ready UI event or start chime.
                    capture_ready_tx = None;
                    awaiting_first_captured_chunk = None;
                    stop_flag.store(true, Ordering::Relaxed);

                    // The chunk in hand arrived before the stop; it belongs to
                    // the recording, so feed it ahead of the drain below.
                    if let Some(AudioChunk::Samples(raw)) = pending.take() {
                        frame_resampler.push(&raw, &mut |frame: &[f32]| {
                            handle_frame(
                                frame,
                                true,
                                vad_policy,
                                &vad,
                                &audio_cb,
                                &mut processed_samples,
                            )
                        });
                    }

                    // Drain all remaining audio until the producer confirms end-of-stream.
                    // The cpal callback sees the stop flag, sends EndOfStream, and goes
                    // silent — guaranteeing every captured sample is in the channel
                    // ahead of the sentinel.
                    loop {
                        match sample_rx.recv_timeout(Duration::from_secs(2)) {
                            Ok(AudioChunk::Samples(remaining)) => {
                                frame_resampler.push(&remaining, &mut |frame: &[f32]| {
                                    handle_frame(
                                        frame,
                                        true,
                                        vad_policy,
                                        &vad,
                                        &audio_cb,
                                        &mut processed_samples,
                                    )
                                });
                            }
                            Ok(AudioChunk::EndOfStream) => break,
                            Err(_) => {
                                log::warn!("Timed out waiting for EndOfStream from audio callback");
                                break;
                            }
                        }
                    }

                    frame_resampler.finish(&mut |frame: &[f32]| {
                        handle_frame(
                            frame,
                            true,
                            vad_policy,
                            &vad,
                            &audio_cb,
                            &mut processed_samples,
                        )
                    });

                    let _ = reply_tx.send(std::mem::take(&mut processed_samples));

                    // Resume the audio callback so the consumer loop can continue
                    // receiving chunks (important for always-on microphone mode).
                    stop_flag.store(false, Ordering::Relaxed);
                }
                Cmd::Shutdown => {
                    stop_flag.store(true, Ordering::Relaxed);
                    return;
                }
            }
        }

        let raw = match pending.take() {
            Some(AudioChunk::Samples(s)) => s,
            // EndOfStream, or the chunk was consumed by a Stop above.
            _ => continue,
        };

        let chunk_ms = raw.len() as f64 * 1000.0 / in_sample_rate as f64;
        if !first_chunk_logged {
            first_chunk_logged = true;
            log::debug!(
                "first audio chunk arrived {:?} after stream start ({:.1}ms of audio)",
                stream_running_at.elapsed(),
                chunk_ms
            );
        }

        // ---------- recording-time processing ---------------------------- //
        // In always-on mode the capture stream stays open continuously for
        // zero-latency start, so while idle (not recording) there is nothing to
        // do with a chunk: handle_frame returns early when not recording, which
        // means the resampled output would be discarded, and the level meter has
        // no idle consumer. Skip both the level-meter FFT and the resampler while
        // idle to avoid doing unnecessary work whose output is thrown away. Both
        // are reset on Cmd::Start (visualizer.reset() / frame_resampler.reset()),
        // so they resume cleanly the moment recording begins.
        if recording {
            if let Some(buckets) = visualizer.feed(&raw) {
                if let Some(cb) = &level_cb {
                    cb(buckets);
                }
            }

            frame_resampler.push(&raw, &mut |frame: &[f32]| {
                handle_frame(
                    frame,
                    recording,
                    vad_policy,
                    &vad,
                    &audio_cb,
                    &mut processed_samples,
                )
            });
        }

        if recording {
            if let Some(started) = awaiting_first_captured_chunk.take() {
                log::debug!(
                    "first captured chunk ({:.1}ms of audio) processed {:?} after Cmd::Start",
                    chunk_ms,
                    started.elapsed()
                );
            }
            if let Some(ready_tx) = capture_ready_tx.take() {
                // Signal only after this chunk has passed through the visualizer
                // and resampler. Silence still counts: readiness means the host
                // is delivering samples, not that VAD has detected speech.
                let _ = ready_tx.send(());
            }
        }
    }
}
