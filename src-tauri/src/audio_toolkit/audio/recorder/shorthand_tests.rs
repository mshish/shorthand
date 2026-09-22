//! Shorthand-only recorder tests: the system-audio (loopback) lane, the
//! two-lane `RecordedAudio` stop, and the fork's extra detection cases.
//! Upstream's tests live unmodified in `tests.rs`.

use super::{
    downmix_loopback, is_microphone_access_denied, is_no_input_device_error,
    loopback_error_callback, run_consumer, run_loopback_pump, run_system_consumer,
    spawn_system_audio_lane, AudioChunk, AudioRecorder, CaptureProcessor, CaptureTransportState,
    Cmd, LoopbackCallback, LoopbackChunk, LoopbackPumpCmd, SystemAudioSession, VadPolicy,
    SYSTEM_STOP_DRAIN_TIMEOUT,
};
use rtrb::RingBuffer;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::thread;
use std::time::{Duration, Instant};

fn system_processor(sample_rate: u32) -> CaptureProcessor {
    CaptureProcessor::new(sample_rate, None, None, None, Instant::now())
}

#[test]
fn system_consumer_shutdown_is_processed_without_audio_samples() {
    let (sample_tx, sample_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        run_system_consumer(system_processor(48_000), sample_rx, cmd_rx);
        let _ = done_tx.send(());
    });

    cmd_tx.send(Cmd::Shutdown).expect("send shutdown");
    let stopped = done_rx.recv_timeout(Duration::from_secs(1));

    // Unblock a consumer that ignores Shutdown so a failing test still exits cleanly.
    drop(sample_tx);
    worker.join().expect("join consumer");
    assert!(stopped.is_ok(), "shutdown waited for an audio sample");
}

#[test]
fn microphone_access_denied_error_is_detected() {
    assert!(is_microphone_access_denied("Access is denied"));
}

#[test]
fn microphone_permission_denied_error_is_detected() {
    assert!(is_microphone_access_denied("permission denied"));
}

#[test]
fn microphone_windows_access_denied_error_is_detected() {
    assert!(is_microphone_access_denied("WASAPI error: 0x80070005"));
}

#[test]
fn microphone_access_denied_ignores_unrelated_errors() {
    assert!(!is_microphone_access_denied("device not found"));
}

#[test]
fn no_input_device_error_is_detected() {
    assert!(is_no_input_device_error("No input device found"));
}

#[test]
fn coreaudio_config_error_is_detected_as_no_input_device() {
    assert!(is_no_input_device_error(
        "Failed to fetch preferred config: A backend-specific error has occurred: An unknown error unknown to the coreaudio-rs API occurred"
    ));
}

#[test]
fn no_input_device_error_ignores_other_errors() {
    assert!(!is_no_input_device_error("permission denied"));
    assert!(!is_no_input_device_error("device not found"));
}

struct LoopbackHarness {
    raw_tx: Option<mpsc::SyncSender<LoopbackChunk>>,
    cmd_tx: mpsc::Sender<Cmd>,
    pump_tx: mpsc::Sender<LoopbackPumpCmd>,
    session: Arc<SystemAudioSession>,
    consumer_done: mpsc::Receiver<()>,
    pump_done: mpsc::Receiver<()>,
}

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
            run_system_consumer(system_processor(16_000), sample_rx, cmd_rx);
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
        let generation = self.session.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.session.active.store(true, Ordering::SeqCst);
        self.pump_tx
            .send(LoopbackPumpCmd::StartSession(generation))
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

    fn raw_sender(&self) -> mpsc::SyncSender<LoopbackChunk> {
        self.raw_tx.as_ref().expect("loopback sender").clone()
    }

    fn fail_device(&mut self) {
        self.raw_tx.take();
    }

    fn stop(&self) -> Vec<f32> {
        self.end_session();
        self.finish_stop()
    }

    /// The part of `AudioRecorder::stop()` that ends the session, before any
    /// command reaches the lane.
    fn end_session(&self) {
        self.session.active.store(false, Ordering::SeqCst);
        self.session.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// The rest of `AudioRecorder::stop()`: stop the consumer, end the pump's
    /// session, and wait for the recording.
    fn finish_stop(&self) -> Vec<f32> {
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
    assert!(!recorder.needs_reopen());
}

#[test]
fn disabled_recorder_has_no_loopback_runtime_resources() {
    let recorder = AudioRecorder::new().expect("recorder");
    assert!(recorder.system_vad.is_none(), "no second VAD");
    assert!(recorder.system_cmd_tx.is_none(), "no second consumer");
    assert!(recorder.loopback_pump_tx.is_none(), "no pump thread");
}

#[test]
fn surround_downmix_preserves_center_and_ignores_lfe() {
    let mut mono = Vec::new();
    downmix_loopback(&[0.0_f32, 0.0, 1.0, 1.0, 0.0, 0.0], 6, &mut mono);
    assert_eq!(mono.len(), 1);
    assert!((mono[0] - 0.4).abs() < 1e-6);
}

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

#[test]
fn stop_completes_when_loopback_fails_before_first_sample() {
    let mut harness = LoopbackHarness::new();
    harness.fail_device();
    harness.start();
    let _ = harness.stop();
    harness.shutdown();
}

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

#[test]
fn stop_immediately_after_start_completes() {
    let harness = LoopbackHarness::new();
    harness.start();
    let _ = harness.stop();
    harness.shutdown();
}

#[test]
fn shutdown_while_loopback_is_silent_completes() {
    LoopbackHarness::new().shutdown();
}

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
        .send(LoopbackPumpCmd::StartSession(1))
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
        if let AudioChunk::Samples(samples) = sample_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("real burst output")
        {
            sample_count += samples.len();
        }
    }
    control_tx
        .send(LoopbackPumpCmd::EndSession)
        .expect("end pump");

    while let AudioChunk::Samples(samples) = sample_rx
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

#[test]
fn real_recorder_stop_completes_while_loopback_packets_continue() {
    let harness = LoopbackHarness::new();
    harness.start();
    let generation = harness.session.generation.load(Ordering::Acquire);
    let raw_tx = harness.raw_tx.as_ref().expect("raw sender").clone();
    let producing = Arc::new(AtomicBool::new(true));
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

/// Both lanes through the public `start()`/`stop()`: the microphone on
/// upstream's ring-buffer consumer, the system audio on the loopback pump.
/// Guards the integration itself — that the mic's pause handshake and the
/// system lane's end-of-stream drain complete together and land in the right
/// `RecordedAudio` field.
#[test]
fn recorder_stop_returns_both_lanes_with_ring_microphone() {
    let harness = LoopbackHarness::new();

    let (mut producer, consumer) = RingBuffer::<f32>::new(16_000);
    let transport = Arc::new(CaptureTransportState::default());
    let (mic_tx, mic_rx) = mpsc::channel();
    let consumer_transport = Arc::clone(&transport);
    let mic_worker = thread::spawn(move || {
        run_consumer(
            CaptureProcessor::new(16_000, None, None, None, Instant::now()),
            consumer,
            mic_rx,
            consumer_transport,
            Arc::new(AtomicBool::new(false)),
        );
    });
    // Stands in for the cpal callback: a steady stream of mic blocks.
    let producing = Arc::new(AtomicBool::new(true));
    let producer_flag = Arc::clone(&producing);
    let callback_transport = Arc::clone(&transport);
    let mic_callback = thread::spawn(move || {
        while producer_flag.load(Ordering::Acquire) {
            AudioRecorder::write_input_to_ring(
                &[0.5f32; 160],
                1,
                None,
                &mut producer,
                &callback_transport,
            );
            thread::sleep(Duration::from_millis(1));
        }
    });

    let mut recorder = AudioRecorder::new().expect("recorder");
    recorder.cmd_tx = Some(mic_tx.clone());
    recorder.system_cmd_tx = Some(harness.cmd_tx.clone());
    recorder.loopback_pump_tx = Some(harness.pump_tx.clone());
    recorder.system_audio_session = Arc::clone(&harness.session);

    recorder
        .start(VadPolicy::Disabled)
        .expect("start")
        .recv_timeout(Duration::from_secs(1))
        .expect("microphone capture ready");
    let generation = harness.session.generation.load(Ordering::Acquire);
    harness.send(vec![0.25; 480], generation);
    thread::sleep(Duration::from_millis(50));

    let recorded = recorder.stop().expect("two-lane stop must complete");
    assert!(recorded.microphone.iter().any(|s| (*s - 0.5).abs() < 1e-6));
    assert!(!recorded.microphone.iter().any(|s| (*s - 0.25).abs() < 1e-6));
    assert!(recorded.system.iter().any(|s| (*s - 0.25).abs() < 1e-6));
    assert!(!recorded.system.iter().any(|s| (*s - 0.5).abs() < 1e-6));
    assert!(
        !transport.pause_requested.load(Ordering::Acquire),
        "microphone capture must resume before stop() returns"
    );

    producing.store(false, Ordering::Release);
    mic_callback.join().expect("mic callback exits");
    let _ = mic_tx.send(Cmd::Shutdown);
    mic_worker.join().expect("mic consumer exits");
    drop(recorder);
    harness.shutdown();
}

fn contains(samples: &[f32], value: f32) -> bool {
    samples.iter().any(|sample| (*sample - value).abs() < 1e-6)
}

/// A loopback stream that dies (headphones plugged in, a Bluetooth switch)
/// must rebuild capture before the next recording; one that reports a
/// condition it recovers from must not.
#[test]
fn fatal_loopback_stream_error_requests_reopen() {
    let recorder = AudioRecorder::new().expect("recorder");
    let mut on_error = loopback_error_callback(Arc::clone(&recorder.system_audio_error));

    on_error(cpal::Error::new(cpal::ErrorKind::Xrun));
    on_error(cpal::Error::new(cpal::ErrorKind::DeviceChanged));
    on_error(cpal::Error::new(cpal::ErrorKind::RealtimeDenied));
    assert!(
        !recorder.needs_reopen(),
        "a loopback stream that keeps capturing must not be rebuilt"
    );

    on_error(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));
    assert!(
        recorder.needs_reopen(),
        "a dead loopback stream must be rebuilt before the next recording"
    );
}

/// A recorder whose system-audio lane threads have exited, as after a panic
/// in the system consumer.
fn recorder_with_dead_system_lane(mic_tx: mpsc::Sender<Cmd>) -> AudioRecorder {
    let (system_tx, _) = mpsc::channel::<Cmd>();
    let (pump_tx, _) = mpsc::channel::<LoopbackPumpCmd>();
    let mut recorder = AudioRecorder::new().expect("recorder");
    recorder.cmd_tx = Some(mic_tx);
    recorder.system_cmd_tx = Some(system_tx);
    recorder.loopback_pump_tx = Some(pump_tx);
    recorder
}

#[test]
fn start_records_microphone_only_when_the_system_lane_is_dead() {
    let (mic_tx, mic_rx) = mpsc::channel();
    let recorder = recorder_with_dead_system_lane(mic_tx);

    recorder
        .start(VadPolicy::Disabled)
        .expect("a dead system-audio lane must not fail the microphone");
    assert!(
        matches!(mic_rx.try_recv(), Ok(Cmd::Start(..))),
        "the microphone was not started"
    );
    assert!(
        recorder.needs_reopen(),
        "the dead lane must be rebuilt before the next recording"
    );
}

#[test]
fn failed_start_ends_the_system_audio_session() {
    let (mic_tx, _) = mpsc::channel();
    let recorder = recorder_with_dead_system_lane(mic_tx);

    assert!(recorder.start(VadPolicy::Disabled).is_err());
    assert!(
        !recorder.system_audio_session.active.load(Ordering::SeqCst),
        "a failed start left the system-audio session active"
    );
}

#[test]
fn loopback_callback_forwards_its_pool_buffer_without_reallocating() {
    let (buffer_tx, buffer_rx) = mpsc::sync_channel(1);
    let pooled = Vec::with_capacity(4096);
    let pooled_ptr = pooled.as_ptr();
    buffer_tx.send(pooled).expect("fill the pool");
    let (loopback_tx, loopback_rx) = mpsc::sync_channel(1);
    let session = Arc::new(SystemAudioSession::default());
    session.generation.store(1, Ordering::SeqCst);
    session.active.store(true, Ordering::SeqCst);
    let mut callback = LoopbackCallback {
        channels: 2,
        sample_rate: 48_000,
        session: Arc::clone(&session),
        emergency_buffer: None,
        buffer_rx,
        loopback_tx,
        dropped_samples: Arc::new(AtomicUsize::new(0)),
    };

    callback.process(&[0.5_f32; 960]);

    let chunk = loopback_rx.try_recv().expect("forwarded block");
    assert_eq!(chunk.samples.len(), 480);
    assert_eq!(chunk.session_generation, 1);
    assert_eq!(
        chunk.samples.as_ptr(),
        pooled_ptr,
        "the callback reallocated its pool buffer"
    );
    assert!(!session.callback_busy.load(Ordering::SeqCst));
}

#[test]
fn pump_returns_pool_buffers_with_their_capacity() {
    let (raw_tx, raw_rx) = mpsc::sync_channel(1);
    let (buffer_tx, buffer_rx) = mpsc::sync_channel(1);
    let (sample_tx, sample_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();
    let session = Arc::new(SystemAudioSession::default());
    session.generation.store(1, Ordering::SeqCst);
    session.active.store(true, Ordering::SeqCst);
    let pump_session = Arc::clone(&session);
    let pump = thread::spawn(move || {
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
        .send(LoopbackPumpCmd::StartSession(1))
        .expect("start pump");
    let mut samples = Vec::with_capacity(4096);
    samples.extend_from_slice(&[0.5; 160]);
    raw_tx
        .send(LoopbackChunk {
            samples,
            sample_rate: 16_000,
            session_generation: 1,
        })
        .expect("loopback block");

    let returned = buffer_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("buffer returned to the pool");
    assert!(returned.is_empty());
    assert!(
        returned.capacity() >= 4096,
        "the pool got back a buffer the callback must reallocate: capacity {}",
        returned.capacity()
    );
    let forwarded = loop {
        match sample_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("forwarded samples")
        {
            AudioChunk::Samples(samples) if contains(&samples, 0.5) => break samples,
            AudioChunk::Samples(_) => {}
            AudioChunk::EndOfStream => panic!("no session end was requested"),
        }
    };
    assert_eq!(forwarded, vec![0.5; 160]);

    drop(control_tx);
    drop(raw_tx);
    pump.join().expect("pump exits");
}

#[test]
fn stop_keeps_the_loopback_block_handed_over_after_stop() {
    let harness = LoopbackHarness::new();
    harness.start();
    let generation = harness.session.generation.load(Ordering::Acquire);

    harness.end_session();
    // The callback that was mid-block when stop() ran hands it over only now.
    harness.send(vec![0.25; 480], generation);
    let recorded = harness.finish_stop();

    assert!(contains(&recorded, 0.25), "the recording lost its tail");
    harness.shutdown();
}

#[test]
fn stop_waits_for_a_loopback_callback_still_in_flight() {
    let harness = LoopbackHarness::new();
    harness.start();
    let generation = harness.session.generation.load(Ordering::Acquire);

    harness.session.callback_busy.store(true, Ordering::SeqCst);
    harness.end_session();
    let raw_tx = harness.raw_sender();
    let session = Arc::clone(&harness.session);
    let callback = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        raw_tx
            .send(LoopbackChunk {
                samples: vec![0.25; 480],
                sample_rate: 16_000,
                session_generation: generation,
            })
            .expect("in-flight block");
        session.callback_busy.store(false, Ordering::SeqCst);
    });
    let recorded = harness.finish_stop();
    callback.join().expect("callback exits");

    assert!(
        contains(&recorded, 0.25),
        "the block in flight at stop was lost"
    );
    harness.shutdown();
}

/// A stop that times out waiting for its end-of-stream marker leaves that
/// marker, and the session tail ahead of it, to arrive later. They must not
/// end the next stop's drain early or be counted as the next recording.
#[test]
fn late_end_marker_from_a_timed_out_stop_does_not_cut_the_next_recording() {
    let (sample_tx, sample_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let consumer = thread::spawn(move || {
        run_system_consumer(system_processor(16_000), sample_rx, cmd_rx);
    });
    let start = || {
        let (ready_tx, _) = mpsc::channel();
        cmd_tx
            .send(Cmd::Start(VadPolicy::Disabled, Instant::now(), ready_tx))
            .expect("start command");
    };

    start();
    let (first_tx, first_rx) = mpsc::channel();
    cmd_tx.send(Cmd::Stop(first_tx)).expect("first stop");
    first_rx
        .recv_timeout(SYSTEM_STOP_DRAIN_TIMEOUT + Duration::from_secs(1))
        .expect("a timed-out stop still replies");

    // The second recording is stopped before the first session's late tail
    // and marker arrive, followed by its own audio and marker.
    start();
    let (second_tx, second_rx) = mpsc::channel();
    cmd_tx.send(Cmd::Stop(second_tx)).expect("second stop");
    sample_tx
        .send(AudioChunk::Samples(vec![0.75; 480]))
        .expect("late tail");
    sample_tx
        .send(AudioChunk::EndOfStream)
        .expect("late marker");
    sample_tx
        .send(AudioChunk::Samples(vec![0.25; 480]))
        .expect("second recording");
    sample_tx
        .send(AudioChunk::EndOfStream)
        .expect("second marker");

    let second = second_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second stop replies");
    assert!(
        contains(&second, 0.25),
        "the late marker ended the second recording's drain"
    );
    assert!(
        !contains(&second, 0.75),
        "the first session's tail was credited to the second recording"
    );

    cmd_tx.send(Cmd::Shutdown).expect("shutdown");
    consumer.join().expect("consumer exits");
}

/// After upstream's pause-timeout early return the capture worker drops its
/// streams and shuts the lane down while the recorder still holds its own
/// senders. The lane must exit anyway, or the worker never finishes.
#[test]
fn system_lane_exits_while_the_recorder_still_holds_its_senders() {
    let (sample_tx, sample_rx) = mpsc::channel();
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (_raw_tx, raw_rx) = mpsc::sync_channel(1);
    let (buffer_tx, _buffer_rx) = mpsc::sync_channel(1);
    let (pump_tx, pump_rx) = mpsc::channel();
    let lane = spawn_system_audio_lane(
        16_000,
        None,
        None,
        sample_tx,
        sample_rx,
        cmd_rx,
        raw_rx,
        buffer_tx,
        pump_rx,
        Arc::new(SystemAudioSession::default()),
    )
    .expect("spawn lane");
    let recorder_cmd_tx = cmd_tx.clone();
    let recorder_pump_tx = pump_tx.clone();

    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        lane.shut_down(&cmd_tx, &pump_tx);
        let _ = done_tx.send(());
    });
    let finished = done_rx.recv_timeout(Duration::from_secs(1));

    // Release a hung lane so a failure does not leak its threads.
    drop(recorder_cmd_tx);
    drop(recorder_pump_tx);
    assert!(
        finished.is_ok(),
        "the lane waited for the recorder's senders to drop"
    );
}
