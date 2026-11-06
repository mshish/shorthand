//! Maps capture failures to the fixed kinds the overlay's error state shows.
//! Kinds are codes, never messages: the overlay looks up the copy.

use crate::notices::NoticeCode;

/// The fixed `error_type` for a recording that couldn't start, from the
/// notice code the recorder's error classifies to
/// (`managers::audio::recording_notice_code`). Shared by the `--follow-stream`
/// `start_failed` code, the overlay error and telemetry.
pub fn start_failure_error_type(code: NoticeCode) -> &'static str {
    match code {
        NoticeCode::MicPermissionDenied => "microphone_permission_denied",
        NoticeCode::NoMicrophone => "no_input_device",
        _ => "unknown",
    }
}

/// `error_type` is the code `actions.rs` already sends with `recording-error`.
pub fn for_start_failure(error_type: &str) -> &'static str {
    match error_type {
        "microphone_permission_denied" => {
            if cfg!(windows) {
                "mic_permission_windows"
            } else if cfg!(target_os = "macos") {
                "mic_permission_macos"
            } else {
                "mic_permission"
            }
        }
        "no_input_device" => "mic_missing",
        _ => "mic_failed",
    }
}

/// `reason` is `telemetry::transcription_reason`'s code.
pub fn for_transcription_reason(reason: &str) -> &'static str {
    match reason {
        "model_not_loaded" => "not_ready",
        "engine_busy" | "finalize_timeout" => "busy",
        _ => "transcribe_failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_notice_codes_map_to_their_error_types() {
        // A recorder error of these kinds can't be built without a device, so
        // the mapping is tested from the notice code it is derived from.
        assert_eq!(
            start_failure_error_type(NoticeCode::MicPermissionDenied),
            "microphone_permission_denied"
        );
        assert_eq!(
            start_failure_error_type(NoticeCode::NoMicrophone),
            "no_input_device"
        );
        assert_eq!(
            start_failure_error_type(NoticeCode::RecordingFailed),
            "unknown"
        );
        assert_eq!(
            start_failure_error_type(NoticeCode::TranscriptionFailed),
            "unknown"
        );
    }

    #[test]
    fn start_failures_map_to_their_kinds() {
        assert_eq!(for_start_failure("no_input_device"), "mic_missing");
        assert_eq!(for_start_failure("unknown"), "mic_failed");
        assert_eq!(for_start_failure("anything_else"), "mic_failed");
        #[cfg(windows)]
        assert_eq!(
            for_start_failure("microphone_permission_denied"),
            "mic_permission_windows"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            for_start_failure("microphone_permission_denied"),
            "mic_permission_macos"
        );
    }

    #[test]
    fn transcription_reasons_map_to_their_kinds() {
        assert_eq!(for_transcription_reason("model_not_loaded"), "not_ready");
        assert_eq!(for_transcription_reason("engine_busy"), "busy");
        assert_eq!(for_transcription_reason("finalize_timeout"), "busy");
        assert_eq!(
            for_transcription_reason("engine_error"),
            "transcribe_failed"
        );
        assert_eq!(
            for_transcription_reason("engine_panic"),
            "transcribe_failed"
        );
        assert_eq!(for_transcription_reason("other"), "transcribe_failed");
    }
}
