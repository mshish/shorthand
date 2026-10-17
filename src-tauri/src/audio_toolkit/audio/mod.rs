// Re-export all audio components
mod device;
mod sink;
mod utils;
mod visualizer;

// Shorthand: the system-audio (loopback) lane and the resampler it needs.
mod resampler;
mod system_audio;

pub use device::{list_output_devices, output_device_name, OutputDeviceInfo};
pub use sink::{
    AudioFrameCallback, CaptureSink, CaptureVad, CapturedAudio, LevelCallback, VadPolicy,
};
pub use utils::{read_wav_samples, save_wav_file, verify_wav_file};
pub use visualizer::AudioVisualiser;

#[cfg(target_os = "linux")]
pub use device::resolve_linux_system_audio_device;
pub use device::{
    device_display_name, list_system_audio_devices, pinned_default_output, SystemAudioDeviceInfo,
};
pub use resampler::FrameResampler;
pub use system_audio::{LaneVad, LoopbackOpenOutcome, SystemAudioCapture, SystemAudioLane};
