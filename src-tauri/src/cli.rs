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
#[command(name = "shorthand", about = "Shorthand - live transcript capture")]
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

    /// Toggle an Assisted Notes capture on/off (sent to running instance)
    #[arg(
        long,
        conflicts_with_all = ["start_assisted_notes", "stop_assisted_notes"]
    )]
    pub toggle_assisted_notes: bool,

    /// Start an Assisted Notes capture (sent to running instance). Idempotent:
    /// a no-op if that capture is already running, refused (not a toggle-off)
    /// if a different capture is running or the mode is disabled.
    //
    // Conflicts with --stop-assisted-notes: without this, clap accepts both
    // flags at once and callback ordering silently picks start, so a caller
    // that passes a contradictory pair gets one of its two requests executed
    // without any indication the other was ignored. The remaining four
    // entries are every other mutually exclusive remote-control flag: the
    // single-instance dispatch in `lib.rs` is an `else if` chain that acts on
    // only the first match, so e.g. `--toggle-transcription
    // --start-assisted-notes` would otherwise parse and silently drop the
    // assisted-notes request instead of rejecting the combination at parse
    // time. Declared here (matching how `--follow-stream` declares its own
    // full list below) rather than added to those four flags' own attributes,
    // so the new flags carry this constraint without touching upstream's
    // existing ones.
    #[arg(
        long,
        conflicts_with_all = [
            "stop_assisted_notes",
            "toggle_assisted_notes",
            "toggle_transcription",
            "toggle_post_process",
            "cancel",
            "follow_stream",
        ]
    )]
    pub start_assisted_notes: bool,

    /// Stop an Assisted Notes capture (sent to running instance). Idempotent:
    /// a no-op if that capture is not the one running.
    //
    // See `start_assisted_notes`'s own comment for why this list is longer
    // than just the other two assisted-notes flags.
    #[arg(
        long,
        conflicts_with_all = [
            "start_assisted_notes",
            "toggle_assisted_notes",
            "toggle_transcription",
            "toggle_post_process",
            "cancel",
            "follow_stream",
        ]
    )]
    pub stop_assisted_notes: bool,

    /// Start a Meeting capture (sent to running instance). Idempotent: a
    /// no-op if that capture is already running, refused (not a toggle-off)
    /// if a different capture is running.
    //
    // The explicit half of `--toggle-transcription`, and the same pairing
    // `--start-assisted-notes` is to `--toggle-assisted-notes`: named after
    // the toggle it replaces rather than after the mode's wire spelling
    // (`meeting`), so the flag family reads as one. See
    // `start_assisted_notes`'s own comment for why the conflict list names
    // every other remote-control flag rather than just this pair.
    #[arg(
        long,
        conflicts_with_all = [
            "stop_transcription",
            "toggle_transcription",
            "toggle_post_process",
            "cancel",
            "toggle_assisted_notes",
            "start_assisted_notes",
            "stop_assisted_notes",
            "follow_stream",
        ]
    )]
    pub start_transcription: bool,

    /// Stop a Meeting capture (sent to running instance). Idempotent: a
    /// no-op if that capture is not the one running.
    //
    // See `start_transcription`'s own comment.
    #[arg(
        long,
        conflicts_with_all = [
            "start_transcription",
            "toggle_transcription",
            "toggle_post_process",
            "cancel",
            "toggle_assisted_notes",
            "start_assisted_notes",
            "stop_assisted_notes",
            "follow_stream",
        ]
    )]
    pub stop_transcription: bool,

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

    /// Attach to the running Shorthand instance and stream live transcript events to
    /// stdout as NDJSON. Pass `delta` for append-only committed text as JSONL,
    /// or `text` for the plain human-readable rendering of the same.
    #[arg(
        long,
        value_name = "MODE",
        num_args = 0..=1,
        default_missing_value = "json",
        conflicts_with_all = ["toggle_transcription", "toggle_post_process", "cancel", "toggle_assisted_notes", "start_assisted_notes", "stop_assisted_notes", "start_transcription", "stop_transcription"]
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

        let error =
            CliArgs::try_parse_from(["handy", "--follow-stream", "--toggle-assisted-notes"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error = CliArgs::try_parse_from(["handy", "--follow-stream", "--start-assisted-notes"])
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error = CliArgs::try_parse_from(["handy", "--follow-stream", "--stop-assisted-notes"])
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error = CliArgs::try_parse_from(["handy", "--follow-stream", "bogus"]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn explicit_assisted_notes_flags_parse_independently_of_the_toggle() {
        assert!(!CliArgs::parse_from(["handy"]).start_assisted_notes);
        assert!(!CliArgs::parse_from(["handy"]).stop_assisted_notes);
        assert!(CliArgs::parse_from(["handy", "--start-assisted-notes"]).start_assisted_notes);
        assert!(CliArgs::parse_from(["handy", "--stop-assisted-notes"]).stop_assisted_notes);
        // Kept alongside the toggle rather than replacing it: fork-only and
        // harmless for manual use.
        assert!(CliArgs::parse_from(["handy", "--toggle-assisted-notes"]).toggle_assisted_notes);
    }

    #[test]
    fn contradictory_assisted_notes_flags_fail_to_parse() {
        // Without these conflicts, clap accepts both flags in a pair and
        // callback ordering silently picks one (start), so a caller that
        // sends a contradictory combination gets no indication the other
        // half of it was ignored. Every pairing among the three
        // assisted-notes flags must be rejected at parse time instead.
        let error =
            CliArgs::try_parse_from(["handy", "--start-assisted-notes", "--stop-assisted-notes"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error =
            CliArgs::try_parse_from(["handy", "--start-assisted-notes", "--toggle-assisted-notes"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);

        let error =
            CliArgs::try_parse_from(["handy", "--stop-assisted-notes", "--toggle-assisted-notes"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn explicit_assisted_notes_flags_conflict_with_every_other_remote_control_flag() {
        // Before this fix, `--start-assisted-notes`/`--stop-assisted-notes`
        // only conflicted with each other and `--toggle-assisted-notes`, so a
        // combination like `--toggle-transcription --start-assisted-notes`
        // parsed successfully -- and then the single-instance `else if` chain
        // in `lib.rs` acted on whichever flag it checks first, silently
        // dropping the other. Every one of these must now be rejected at
        // parse time instead.
        for other in [
            "--toggle-transcription",
            "--toggle-post-process",
            "--cancel",
            "--follow-stream",
            "--start-transcription",
            "--stop-transcription",
        ] {
            let error =
                CliArgs::try_parse_from(["handy", "--start-assisted-notes", other]).unwrap_err();
            assert_eq!(
                error.kind(),
                ErrorKind::ArgumentConflict,
                "--start-assisted-notes with {other} should conflict"
            );

            let error =
                CliArgs::try_parse_from(["handy", "--stop-assisted-notes", other]).unwrap_err();
            assert_eq!(
                error.kind(),
                ErrorKind::ArgumentConflict,
                "--stop-assisted-notes with {other} should conflict"
            );
        }
    }

    #[test]
    fn explicit_meeting_flags_parse_independently_of_the_toggle() {
        assert!(!CliArgs::parse_from(["handy"]).start_transcription);
        assert!(!CliArgs::parse_from(["handy"]).stop_transcription);
        assert!(CliArgs::parse_from(["handy", "--start-transcription"]).start_transcription);
        assert!(CliArgs::parse_from(["handy", "--stop-transcription"]).stop_transcription);
        // Kept alongside the toggle for the same reason
        // `--toggle-assisted-notes` is: fork-only and harmless for manual use.
        assert!(CliArgs::parse_from(["handy", "--toggle-transcription"]).toggle_transcription);
    }

    #[test]
    fn explicit_meeting_flags_conflict_with_every_other_remote_control_flag() {
        // Same hazard the assisted-notes pair documents: the single-instance
        // dispatch in `lib.rs` is an `else if` chain, so any accepted
        // combination would silently drop all but the first match.
        for other in [
            "--toggle-transcription",
            "--toggle-post-process",
            "--cancel",
            "--follow-stream",
            "--toggle-assisted-notes",
            "--start-assisted-notes",
            "--stop-assisted-notes",
        ] {
            let error =
                CliArgs::try_parse_from(["handy", "--start-transcription", other]).unwrap_err();
            assert_eq!(
                error.kind(),
                ErrorKind::ArgumentConflict,
                "--start-transcription with {other} should conflict"
            );

            let error =
                CliArgs::try_parse_from(["handy", "--stop-transcription", other]).unwrap_err();
            assert_eq!(
                error.kind(),
                ErrorKind::ArgumentConflict,
                "--stop-transcription with {other} should conflict"
            );
        }

        let error =
            CliArgs::try_parse_from(["handy", "--start-transcription", "--stop-transcription"])
                .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}
