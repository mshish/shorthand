pub mod audio;
pub mod constants;
pub mod lang_id;
pub mod text;
pub mod utils;
pub mod vad;

#[cfg(target_os = "linux")]
pub use audio::resolve_linux_system_audio_device;
pub use audio::{
    is_microphone_access_denied, is_no_input_device_error, list_input_devices, list_output_devices,
    list_system_audio_devices, read_wav_samples, save_wav_file, verify_wav_file, AudioRecorder,
    CpalDeviceInfo, LoopbackOpenOutcome, RecordedAudio, SystemAudioDeviceInfo, VadPolicy,
};
pub use lang_id::detect_output_language;
pub use text::{
    apply_custom_words, normalize_transcription_output, remove_filler_words, OutputLanguageEvidence,
};
pub use utils::{get_cpal_host, get_system_audio_host};
pub use vad::{SileroVad, VoiceActivityDetector};
