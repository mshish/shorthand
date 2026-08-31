use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader, Read, Write},
};

use interprocess::local_socket::{traits::Stream as _, Stream};
use serde_json::Value;

use crate::FollowStreamMode;

use super::socket_name_owned;

const NOT_RUNNING_MESSAGE: &str =
    "Handy is not running, or live transcript streaming is disabled in Settings";
const DELTA_REQUIRES_STREAMING_MESSAGE: &str =
    "Delta and text modes require a model that supports streaming";

pub fn run_client(mode: FollowStreamMode) -> i32 {
    attach_parent_console();

    let name = match socket_name_owned() {
        Ok(name) => name,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "Failed to determine follow-stream socket: {error}"
            );
            return exit_code(Err(ClientFailure::SocketName));
        }
    };
    run_client_with_name(mode, name)
}

fn run_client_with_name(mode: FollowStreamMode, name: interprocess::local_socket::Name<'_>) -> i32 {
    let stream = match Stream::connect(name) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = writeln!(io::stderr(), "{NOT_RUNNING_MESSAGE}");
            if is_not_running_connect_error(&error) {
                return exit_code(Err(ClientFailure::NotRunning));
            }
            let _ = writeln!(io::stderr(), "Follow-stream connection failed: {error}");
            return exit_code(Err(ClientFailure::Connect));
        }
    };

    let stdout = io::stdout();
    finish_client(
        process_stream(stream, stdout.lock(), mode),
        &mut io::stderr(),
    )
}

fn finish_client(result: Result<(), ProcessError>, stderr: &mut impl Write) -> i32 {
    let result = match result {
        Ok(()) => Ok(()),
        Err(ProcessError::DeltaRequiresStreaming) => {
            let _ = writeln!(stderr, "{DELTA_REQUIRES_STREAMING_MESSAGE}");
            Err(ClientFailure::DeltaRequiresStreaming)
        }
        Err(ProcessError::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe => {
            Err(ClientFailure::Io(io::ErrorKind::BrokenPipe))
        }
        Err(ProcessError::Io(error)) => {
            let _ = writeln!(stderr, "Follow-stream failed: {error}");
            Err(ClientFailure::Io(error.kind()))
        }
    };
    exit_code(result)
}

#[derive(Debug, PartialEq, Eq)]
enum ClientFailure {
    SocketName,
    NotRunning,
    Connect,
    DeltaRequiresStreaming,
    Io(io::ErrorKind),
}

fn exit_code(result: Result<(), ClientFailure>) -> i32 {
    match result {
        Ok(()) | Err(ClientFailure::Io(io::ErrorKind::BrokenPipe)) => 0,
        Err(ClientFailure::NotRunning) => 2,
        Err(
            ClientFailure::SocketName
            | ClientFailure::Connect
            | ClientFailure::DeltaRequiresStreaming
            | ClientFailure::Io(_),
        ) => 1,
    }
}

fn is_not_running_connect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    )
}

#[cfg(windows)]
fn attach_parent_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};

    // A release build uses the Windows GUI subsystem, so attach to the shell
    // that launched us before stdout or stderr is accessed. Failure is benign:
    // the process may already have a console or may have been launched detached.
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn process_stream<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    mode: FollowStreamMode,
) -> Result<(), ProcessError> {
    let mut reader = BufReader::new(reader);
    let mut committed = CommittedStream::default();
    let mut text = TextRenderer::default();
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) if is_clean_disconnect(&error) => return Ok(()),
            Err(error) => return Err(error.into()),
        }

        match mode {
            FollowStreamMode::Json => writer.write_all(line.as_bytes())?,
            // `delta` and `text` are two renderings of the same committed-chunk
            // extraction, so both walk the identical state machine.
            FollowStreamMode::Delta | FollowStreamMode::Text => {
                if let Some(event) = committed.push(&line)? {
                    let rendered = match mode {
                        FollowStreamMode::Delta => event.to_jsonl(),
                        _ => text.render(&event),
                    };
                    writer.write_all(rendered.as_bytes())?;
                }
            }
        }
        writer.flush()?;
    }
}

#[derive(Debug)]
enum ProcessError {
    DeltaRequiresStreaming,
    Io(io::Error),
}

impl From<io::Error> for ProcessError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn is_clean_disconnect(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    ) {
        return true;
    }

    #[cfg(windows)]
    if matches!(error.raw_os_error(), Some(109 | 232 | 233)) {
        // ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED.
        return true;
    }

    false
}

/// Schema version of the `delta` JSONL records. Independent of the wire
/// protocol version, because this format is produced entirely client-side and
/// versions on its own schedule — converting `delta` from plain text to JSONL
/// was a breaking change to this format and nothing else.
const DELTA_SCHEMA_VERSION: u32 = 1;

/// Timestamps carried straight through from the `partial` or terminal event a
/// record was derived from. Optional so a follower still works against a Handy
/// old enough to predate stamping.
#[derive(Debug, Default, PartialEq, Eq)]
struct LineStamp {
    emitted_at: Option<String>,
    session_elapsed_ms: Option<u64>,
}

impl LineStamp {
    fn read(value: &Value) -> Self {
        Self {
            emitted_at: value
                .get("emitted_at")
                .and_then(Value::as_str)
                .map(str::to_string),
            session_elapsed_ms: value.get("session_elapsed_ms").and_then(Value::as_u64),
        }
    }
}

/// A unit of transcript the follower can act on: either text that just became
/// committed, or the close of a session.
#[derive(Debug, PartialEq, Eq)]
enum CommittedEvent {
    Chunk {
        session: u64,
        speaker: String,
        text: String,
        stamp: LineStamp,
    },
    End {
        /// Absent for connection-level errors, which belong to no session.
        session: Option<u64>,
        reason: &'static str,
        message: Option<String>,
        stamp: LineStamp,
    },
}

/// The `delta` mode wire record. Field order here is the emitted order.
#[derive(serde::Serialize)]
struct DeltaRecord<'a> {
    t: &'static str,
    schema: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    session: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speaker: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emitted_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_elapsed_ms: Option<u64>,
}

impl CommittedEvent {
    fn to_jsonl(&self) -> String {
        let record = match self {
            Self::Chunk {
                session,
                speaker,
                text,
                stamp,
            } => DeltaRecord {
                t: "delta",
                schema: DELTA_SCHEMA_VERSION,
                session: Some(*session),
                speaker: Some(speaker),
                text: Some(text),
                reason: None,
                message: None,
                emitted_at: stamp.emitted_at.as_deref(),
                session_elapsed_ms: stamp.session_elapsed_ms,
            },
            Self::End {
                session,
                reason,
                message,
                stamp,
            } => DeltaRecord {
                t: "end",
                schema: DELTA_SCHEMA_VERSION,
                session: *session,
                speaker: None,
                text: None,
                reason: Some(reason),
                message: message.as_deref(),
                emitted_at: stamp.emitted_at.as_deref(),
                session_elapsed_ms: stamp.session_elapsed_ms,
            },
        };

        match serde_json::to_string(&record) {
            Ok(mut json) => {
                json.push('\n');
                json
            }
            // Every field is a plain string or integer, so this is unreachable
            // in practice; drop the record rather than emit an unparseable line.
            Err(_) => String::new(),
        }
    }
}

/// Turns the protocol stream into committed chunks. Both `delta` and `text`
/// build on this, which is why both require a streaming-capable model: there is
/// nothing to extract from a session that only ever produces a `final`.
#[derive(Default)]
struct CommittedStream {
    session: Option<u64>,
    committed: HashMap<String, String>,
}

impl CommittedStream {
    fn push(&mut self, line: &str) -> Result<Option<CommittedEvent>, ProcessError> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Ok(None);
        };
        let Some(event_type) = value.get("t").and_then(Value::as_str) else {
            return Ok(None);
        };

        Ok(match event_type {
            "begin" => {
                let Some(session) = value.get("session").and_then(Value::as_u64) else {
                    return Ok(None);
                };
                if value.get("streaming").and_then(Value::as_bool) == Some(false) {
                    return Err(ProcessError::DeltaRequiresStreaming);
                }
                if self.session != Some(session) {
                    self.reset(Some(session));
                }
                None
            }
            "partial" => self.push_partial(&value),
            "final" | "no_speech" | "cancel" => {
                let session = value.get("session").and_then(Value::as_u64);
                if session.is_none() {
                    return Ok(None);
                }
                self.reset(None);
                Some(CommittedEvent::End {
                    session,
                    reason: match event_type {
                        "final" => "final",
                        "no_speech" => "no_speech",
                        _ => "cancel",
                    },
                    message: None,
                    stamp: LineStamp::read(&value),
                })
            }
            "error" => {
                let session = value.get("session").and_then(Value::as_u64);
                let code = value.get("code").and_then(Value::as_str);
                if session.is_none() && code.is_none() {
                    return Ok(None);
                }
                self.reset(None);
                Some(CommittedEvent::End {
                    session,
                    reason: "error",
                    // Carry the diagnostic through; a delta consumer has no
                    // other channel to learn why the session ended.
                    message: value
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    stamp: LineStamp::read(&value),
                })
            }
            _ => None,
        })
    }

    fn push_partial(&mut self, value: &Value) -> Option<CommittedEvent> {
        let session = value.get("session").and_then(Value::as_u64)?;
        let speaker = value.get("speaker").and_then(Value::as_str)?;
        let committed = value.get("committed").and_then(Value::as_str)?;

        if self.session != Some(session) {
            self.reset(Some(session));
        }

        let delta = self
            .committed
            .get(speaker)
            .and_then(|previous| committed.strip_prefix(previous))
            .unwrap_or(committed)
            .to_string();
        self.committed
            .insert(speaker.to_string(), committed.to_string());

        if delta.is_empty() {
            return None;
        }

        Some(CommittedEvent::Chunk {
            session,
            speaker: speaker.to_string(),
            text: delta,
            // The stamp of the snapshot this suffix arrived on. Under coalescing
            // one snapshot can carry several commits, so it marks when the whole
            // suffix was known committed, not when each word was spoken.
            stamp: LineStamp::read(value),
        })
    }

    fn reset(&mut self, session: Option<u64>) {
        self.session = session;
        self.committed.clear();
    }
}

/// The plain human-readable rendering: `me: `/`them: ` on the first output for a
/// speaker and at every speaker change, a newline when a session closes.
#[derive(Default)]
struct TextRenderer {
    session: Option<u64>,
    active_speaker: Option<String>,
}

impl TextRenderer {
    fn render(&mut self, event: &CommittedEvent) -> String {
        match event {
            CommittedEvent::Chunk {
                session,
                speaker,
                text,
                ..
            } => {
                if self.session != Some(*session) {
                    self.session = Some(*session);
                    self.active_speaker = None;
                }

                let mut output = String::new();
                if self.active_speaker.as_deref() != Some(speaker.as_str()) {
                    if self.active_speaker.is_some() {
                        output.push('\n');
                    }
                    output.push_str(speaker);
                    output.push_str(": ");
                    self.active_speaker = Some(speaker.clone());
                }
                output.push_str(text);
                output
            }
            CommittedEvent::End { .. } => {
                self.session = None;
                self.active_speaker = None;
                "\n".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use interprocess::local_socket::{GenericNamespaced, ToNsName};

    use crate::follow_stream::{FollowMode, FollowStreamHub, FollowStreamServer, Speaker};
    use crate::managers::transcription::StreamSource;

    use super::*;

    /// Feeds NDJSON lines through the real dispatch path for `mode`.
    fn run(mode: FollowStreamMode, lines: &[&str]) -> String {
        let input = lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        let mut stdout = Vec::new();
        process_stream(input.as_bytes(), &mut stdout, mode).unwrap();
        String::from_utf8(stdout).unwrap()
    }

    fn render(lines: &[&str]) -> String {
        run(FollowStreamMode::Text, lines)
    }

    #[test]
    fn derived_modes_reject_a_non_streaming_session_without_printing_transcript_text() {
        let input = concat!(
            "{\"t\":\"begin\",\"session\":1,\"streaming\":false,\"mode\":\"meeting\"}\n",
            "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
        );

        // Both derived modes are built from `partial.committed`, so neither can
        // work without a streaming model.
        for mode in [FollowStreamMode::Delta, FollowStreamMode::Text] {
            let mut stdout = Vec::new();
            let result = process_stream(input.as_bytes(), &mut stdout, mode);
            let mut stderr = Vec::new();

            assert_eq!(finish_client(result, &mut stderr), 1, "{mode:?}");
            assert!(stdout.is_empty(), "{mode:?}");
            assert_eq!(
                String::from_utf8(stderr).unwrap(),
                format!("{DELTA_REQUIRES_STREAMING_MESSAGE}\n"),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn json_mode_passes_a_non_streaming_session_through_and_exits_cleanly() {
        let input = concat!(
            "{\"t\":\"begin\",\"session\":1,\"streaming\":false,\"mode\":\"meeting\"}\n",
            "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
        );
        let mut stdout = Vec::new();
        let result = process_stream(input.as_bytes(), &mut stdout, FollowStreamMode::Json);
        let mut stderr = Vec::new();

        assert_eq!(finish_client(result, &mut stderr), 0);
        assert_eq!(stdout, input.as_bytes());
        assert!(stderr.is_empty());
    }

    #[test]
    fn exit_code_mapping_covers_every_client_outcome() {
        assert_eq!(exit_code(Ok(())), 0);
        assert_eq!(exit_code(Err(ClientFailure::SocketName)), 1);
        assert_eq!(exit_code(Err(ClientFailure::NotRunning)), 2);
        assert_eq!(exit_code(Err(ClientFailure::Connect)), 1);
        assert_eq!(exit_code(Err(ClientFailure::DeltaRequiresStreaming)), 1);
        assert_eq!(
            exit_code(Err(ClientFailure::Io(io::ErrorKind::BrokenPipe))),
            0
        );
        assert_eq!(
            exit_code(Err(ClientFailure::Io(io::ErrorKind::PermissionDenied))),
            1
        );
    }

    #[test]
    fn connect_error_classification_only_treats_missing_listener_kinds_as_not_running() {
        assert!(is_not_running_connect_error(&io::Error::from(
            io::ErrorKind::NotFound
        )));
        assert!(is_not_running_connect_error(&io::Error::from(
            io::ErrorKind::ConnectionRefused
        )));
        assert!(!is_not_running_connect_error(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        assert!(!is_not_running_connect_error(&io::Error::from(
            io::ErrorKind::ConnectionReset
        )));
    }

    #[test]
    fn real_client_returns_two_when_nothing_is_listening() {
        let name_text = format!(
            "{}.test.client_missing.{}",
            super::super::socket_name().unwrap(),
            std::process::id()
        );
        let name = name_text.to_ns_name::<GenericNamespaced>().unwrap();

        #[cfg(windows)]
        {
            let error = Stream::connect(name.clone()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
        }

        assert_eq!(run_client_with_name(FollowStreamMode::Json, name), 2);
    }

    /// The realistic session both derived modes are asserted against.
    const MULTI_SPEAKER_SESSION: &[&str] = &[
        r#"{"t":"hello","protocol":1,"version":"test","emitted_at":"2026-08-15T14:03:20.100-07:00"}"#,
        r#"{"t":"begin","session":1,"streaming":true,"emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}"#,
        r#"{"t":"partial","session":1,"speaker":"me","committed":"hel","tentative":"lo","emitted_at":"2026-08-15T14:03:21.000-07:00","session_elapsed_ms":800}"#,
        r#"{"t":"partial","session":1,"speaker":"me","committed":"hello ","tentative":"wor","emitted_at":"2026-08-15T14:03:21.412-07:00","session_elapsed_ms":1212}"#,
        r#"{"t":"partial","session":1,"speaker":"me","committed":"hello world","tentative":"","emitted_at":"2026-08-15T14:03:21.900-07:00","session_elapsed_ms":1700}"#,
        r#"{"t":"partial","session":1,"speaker":"them","committed":"yes","tentative":"","emitted_at":"2026-08-15T14:03:22.900-07:00","session_elapsed_ms":2700}"#,
        r#"{"t":"final","session":1,"text":"Me: hello world\nThem: yes","emitted_at":"2026-08-15T14:03:23.010-07:00","session_elapsed_ms":2810}"#,
    ];

    #[test]
    fn text_mode_renders_a_realistic_multi_speaker_session() {
        let output = render(MULTI_SPEAKER_SESSION);

        assert_eq!(output, "me: hello world\nthem: yes\n");
    }

    #[test]
    fn delta_mode_emits_one_stamped_record_per_committed_suffix() {
        let output = run(FollowStreamMode::Delta, MULTI_SPEAKER_SESSION);

        // Each record's stamp is copied from the partial the suffix arrived on,
        // and the session closes with an explicit reason.
        assert_eq!(
            output,
            concat!(
                "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"hel\",\"emitted_at\":\"2026-08-15T14:03:21.000-07:00\",\"session_elapsed_ms\":800}\n",
                "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"lo \",\"emitted_at\":\"2026-08-15T14:03:21.412-07:00\",\"session_elapsed_ms\":1212}\n",
                "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"world\",\"emitted_at\":\"2026-08-15T14:03:21.900-07:00\",\"session_elapsed_ms\":1700}\n",
                "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"them\",\"text\":\"yes\",\"emitted_at\":\"2026-08-15T14:03:22.900-07:00\",\"session_elapsed_ms\":2700}\n",
                "{\"t\":\"end\",\"schema\":1,\"session\":1,\"reason\":\"final\",\"emitted_at\":\"2026-08-15T14:03:23.010-07:00\",\"session_elapsed_ms\":2810}\n",
            )
        );
    }

    #[test]
    fn delta_end_records_name_the_reason_and_carry_an_error_message() {
        let end_records = |terminal: &str| {
            run(
                FollowStreamMode::Delta,
                &[
                    r#"{"t":"begin","session":7,"streaming":true,"emitted_at":"2026-08-15T14:03:20.200-07:00","session_elapsed_ms":0}"#,
                    terminal,
                ],
            )
        };

        assert_eq!(
            end_records(r#"{"t":"no_speech","session":7,"emitted_at":"2026-08-15T14:03:21.000-07:00","session_elapsed_ms":800}"#),
            "{\"t\":\"end\",\"schema\":1,\"session\":7,\"reason\":\"no_speech\",\"emitted_at\":\"2026-08-15T14:03:21.000-07:00\",\"session_elapsed_ms\":800}\n"
        );
        assert_eq!(
            end_records(r#"{"t":"cancel","session":7,"emitted_at":"2026-08-15T14:03:21.000-07:00","session_elapsed_ms":800}"#),
            "{\"t\":\"end\",\"schema\":1,\"session\":7,\"reason\":\"cancel\",\"emitted_at\":\"2026-08-15T14:03:21.000-07:00\",\"session_elapsed_ms\":800}\n"
        );
        assert_eq!(
            end_records(r#"{"t":"error","session":7,"message":"transcription failed","emitted_at":"2026-08-15T14:03:21.000-07:00","session_elapsed_ms":800}"#),
            "{\"t\":\"end\",\"schema\":1,\"session\":7,\"reason\":\"error\",\"message\":\"transcription failed\",\"emitted_at\":\"2026-08-15T14:03:21.000-07:00\",\"session_elapsed_ms\":800}\n"
        );
        // A connection-level rejection has no session at all.
        assert_eq!(
            end_records(r#"{"t":"error","code":"follower_limit","message":"too many followers","emitted_at":"2026-08-15T14:03:21.000-07:00"}"#),
            "{\"t\":\"end\",\"schema\":1,\"reason\":\"error\",\"message\":\"too many followers\",\"emitted_at\":\"2026-08-15T14:03:21.000-07:00\"}\n"
        );
    }

    #[test]
    fn delta_records_omit_timestamps_a_stampless_server_never_sent() {
        let output = run(
            FollowStreamMode::Delta,
            &[r#"{"t":"partial","session":1,"speaker":"me","committed":"hi","tentative":""}"#],
        );

        assert_eq!(
            output,
            "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"hi\"}\n"
        );
    }

    #[test]
    fn delta_records_escape_transcript_text_rather_than_breaking_the_line() {
        let output = run(
            FollowStreamMode::Delta,
            &[
                r#"{"t":"partial","session":1,"speaker":"me","committed":"a \"quote\"\nand a newline","tentative":""}"#,
            ],
        );

        assert_eq!(
            output,
            "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"a \\\"quote\\\"\\nand a newline\"}\n"
        );
        assert_eq!(output.matches('\n').count(), 1, "one record, one line");
    }

    #[test]
    fn rewritten_committed_prefix_emits_the_whole_replacement() {
        let output = render(&[
            r#"{"t":"partial","session":1,"speaker":"me","committed":"hello","tentative":""}"#,
            r#"{"t":"partial","session":1,"speaker":"me","committed":"hullo","tentative":""}"#,
        ]);

        assert_eq!(output, "me: hellohullo");
    }

    #[test]
    fn unknown_malformed_and_blank_lines_are_skipped() {
        let output = render(&[
            "",
            "not json",
            r#"{"t":"future_event","session":1,"payload":"ignored"}"#,
            r#"{"t":"partial","session":"wrong","speaker":"me","committed":"ignored"}"#,
            r#"{"t":"partial","session":1,"speaker":"me","committed":"kept","tentative":""}"#,
        ]);

        assert_eq!(output, "me: kept");
    }

    #[test]
    fn changing_sessions_resets_committed_and_speaker_state() {
        let output = render(&[
            r#"{"t":"partial","session":1,"speaker":"me","committed":"shared prefix","tentative":""}"#,
            r#"{"t":"partial","session":2,"speaker":"me","committed":"shared prefix plus","tentative":""}"#,
        ]);

        assert_eq!(output, "me: shared prefixme: shared prefix plus");
    }

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl SharedWriter {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    async fn wait_until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !condition() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("condition was not met before timeout");
    }

    #[tokio::test]
    async fn client_loop_reads_real_server_socket_and_preserves_ndjson_bytes() {
        let name_text = format!(
            "{}.test.client_e2e.{}",
            super::super::socket_name().unwrap(),
            std::process::id()
        );
        let name = name_text.to_ns_name::<GenericNamespaced>().unwrap();
        let hub = Arc::new(FollowStreamHub::default());
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();

        let output = SharedWriter::default();
        let output_for_client = output.clone();
        let client = tokio::task::spawn_blocking(move || {
            let stream = Stream::connect(name).unwrap();
            process_stream(stream, output_for_client, FollowStreamMode::Json)
        });

        wait_until(|| hub.follower_count() == 1).await;
        wait_until(|| output.text().contains("\"t\":\"hello\"")).await;

        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        wait_until(|| output.text().contains("\"t\":\"begin\"")).await;
        hub.partial(StreamSource::Mic, "hello ", "wor");
        wait_until(|| output.text().contains("\"t\":\"partial\"")).await;
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        wait_until(|| output.text().contains("\"t\":\"final\"")).await;

        server.stop();
        client.await.unwrap().unwrap();

        // The transport must not reorder or reshape lines. Stamps come from the
        // real clock here, so assert the payload and the stamp's presence
        // separately; hub tests pin the exact stamped bytes.
        let text = output.text();
        assert_eq!(
            text.lines()
                .map(|line| {
                    let start = line
                        .find(",\"emitted_at\":")
                        .unwrap_or_else(|| panic!("every event is stamped, got {line}"));
                    format!("{}}}", &line[..start])
                })
                .collect::<Vec<_>>(),
            [
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"test-version\",\"capabilities\":[\"toggle-assisted-notes\",\"start-assisted-notes\",\"stop-assisted-notes\",\"begin-mode\",\"idle\",\"refused\",\"start-failed\"]}",
                "{\"t\":\"idle\"}",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true,\"mode\":\"meeting\"}",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}",
            ]
        );
    }

    #[tokio::test]
    async fn delta_mode_over_a_real_socket_carries_the_hubs_own_timestamps() {
        let name_text = format!(
            "{}.test.delta_e2e.{}",
            super::super::socket_name().unwrap(),
            std::process::id()
        );
        let name = name_text.to_ns_name::<GenericNamespaced>().unwrap();
        let clock = crate::follow_stream::hub::TestClock::new();
        let hub = Arc::new(FollowStreamHub::with_clock(Arc::clone(&clock) as Arc<_>));
        let server = FollowStreamServer::default();
        server
            .start_with_name(name.clone(), "test-version", Arc::clone(&hub))
            .await
            .unwrap();

        let output = SharedWriter::default();
        let output_for_client = output.clone();
        let client = tokio::task::spawn_blocking(move || {
            let stream = Stream::connect(name).unwrap();
            process_stream(stream, output_for_client, FollowStreamMode::Delta)
        });

        wait_until(|| hub.follower_count() == 1).await;
        clock.advance(100);
        let session = hub.begin(true, FollowMode::Meeting).unwrap();
        clock.advance(1112);
        hub.partial(StreamSource::Mic, "hello ", "wor");
        wait_until(|| output.text().contains("\"t\":\"delta\"")).await;
        clock.advance(638);
        hub.finish(session, Some(Speaker::Me), "Hello world.");
        wait_until(|| output.text().contains("\"t\":\"end\"")).await;

        server.stop();
        client.await.unwrap().unwrap();

        // End to end: the hub stamps, the socket carries, the renderer copies
        // the stamp onto the record it derived from that partial.
        assert_eq!(
            output.text(),
            concat!(
                "{\"t\":\"delta\",\"schema\":1,\"session\":1,\"speaker\":\"me\",\"text\":\"hello \",\"emitted_at\":\"2026-08-15T14:03:21.312-07:00\",\"session_elapsed_ms\":1112}\n",
                "{\"t\":\"end\",\"schema\":1,\"session\":1,\"reason\":\"final\",\"emitted_at\":\"2026-08-15T14:03:21.950-07:00\",\"session_elapsed_ms\":1750}\n",
            )
        );
    }
}
