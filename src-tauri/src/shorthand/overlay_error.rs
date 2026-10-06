//! Maps capture failures to the fixed kinds the overlay's error state shows.
//! Kinds are codes, never messages: the overlay looks up the copy.

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
