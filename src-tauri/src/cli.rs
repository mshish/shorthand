use clap::Parser;
use std::path::PathBuf;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lowercase")]
pub enum FollowStreamMode {
    /// The full protocol stream, verbatim NDJSON.
    Json,
    /// One JSONL record per newly-committed suffix.
    Delta,
    /// The human-readable `me: `/`them: ` rendering of the same committed text.
    Text,
}

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "handy", about = "Handy - Speech to Text")]
pub struct CliArgs {
    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Transcribe this WAV (16 kHz mono) headlessly and exit. Runs the same
    /// batch transcription path as the app — no mic, no VAD, no download
    /// (the model must already be installed).
    #[arg(short = 'f', long, value_name = "WAV")]
    pub transcribe_file: Option<PathBuf>,

    /// Model id to load for --transcribe-file (default: the selected model).
    #[arg(long)]
    pub model: Option<String>,

    /// Hard-select the compute device for --transcribe-file by its registry
    /// index (see --list-devices). Omit to use the persisted accelerator
    /// setting. transcribe-cpp (whisper-family) models only.
    #[arg(long, value_name = "N")]
    pub device_index: Option<usize>,

    /// List the transcribe-cpp compute devices (with indices) and exit.
    #[arg(long)]
    pub list_devices: bool,

    /// List the available models (with ids) and exit. Pass an id to --model.
    /// Honors --json for machine-readable output.
    #[arg(long)]
    pub list_models: bool,

    /// Repeat the transcription N times (best_ms reports the fastest run).
    #[arg(long, value_name = "N")]
    pub repeat: Option<usize>,

    /// Emit --transcribe-file results as JSON.
    #[arg(long)]
    pub json: bool,

    /// Attach to the running Handy instance and stream live transcript events to
    /// stdout as NDJSON. Pass `delta` for append-only committed text as JSONL,
    /// or `text` for the plain human-readable rendering of the same.
    #[arg(
        long,
        value_name = "MODE",
        num_args = 0..=1,
        default_missing_value = "json",
        conflicts_with_all = ["toggle_transcription", "toggle_post_process", "cancel"]
    )]
    pub follow_stream: Option<FollowStreamMode>,
}

#[cfg(test)]
mod tests {
    use clap::{error::ErrorKind, Parser};

    use super::*;

    #[test]
    fn follow_stream_argument_shapes_parse_as_documented() {
        assert_eq!(CliArgs::parse_from(["handy"]).follow_stream, None);
        assert_eq!(
            CliArgs::parse_from(["handy", "--follow-stream"]).follow_stream,
            Some(FollowStreamMode::Json)
        );
        assert_eq!(
            CliArgs::parse_from(["handy", "--follow-stream", "delta"]).follow_stream,
            Some(FollowStreamMode::Delta)
        );
        assert_eq!(
            CliArgs::parse_from(["handy", "--follow-stream=delta"]).follow_stream,
            Some(FollowStreamMode::Delta)
        );
        assert_eq!(
            CliArgs::parse_from(["handy", "--follow-stream", "text"]).follow_stream,
            Some(FollowStreamMode::Text)
        );
        assert_eq!(
            CliArgs::parse_from(["handy", "--follow-stream=text"]).follow_stream,
            Some(FollowStreamMode::Text)
        );

        let error = CliArgs::try_parse_from(["handy", "--follow-stream", "--cancel"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error = CliArgs::try_parse_from(["handy", "--follow-stream", "bogus"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }
}
