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
    "Delta mode requires a model that supports streaming";

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
    let mut renderer = DeltaRenderer::default();
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
            FollowStreamMode::Delta => writer.write_all(renderer.push(&line)?.as_bytes())?,
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

#[derive(Default)]
struct DeltaRenderer {
    session: Option<u64>,
    committed: HashMap<String, String>,
    active_speaker: Option<String>,
}

impl DeltaRenderer {
    fn push(&mut self, line: &str) -> Result<String, ProcessError> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Ok(String::new());
        };
        let Some(event_type) = value.get("t").and_then(Value::as_str) else {
            return Ok(String::new());
        };

        Ok(match event_type {
            "begin" => {
                let Some(session) = value.get("session").and_then(Value::as_u64) else {
                    return Ok(String::new());
                };
                if value.get("streaming").and_then(Value::as_bool) == Some(false) {
                    return Err(ProcessError::DeltaRequiresStreaming);
                }
                if self.session != Some(session) {
                    self.reset(Some(session));
                }
                String::new()
            }
            "partial" => self.push_partial(&value),
            "final" | "no_speech" | "cancel" => {
                if value.get("session").and_then(Value::as_u64).is_none() {
                    return Ok(String::new());
                }
                self.reset(None);
                "\n".to_string()
            }
            "error" => {
                let valid = value.get("session").and_then(Value::as_u64).is_some()
                    || value.get("code").and_then(Value::as_str).is_some();
                if !valid {
                    return Ok(String::new());
                }
                self.reset(None);
                "\n".to_string()
            }
            _ => String::new(),
        })
    }

    fn push_partial(&mut self, value: &Value) -> String {
        let Some(session) = value.get("session").and_then(Value::as_u64) else {
            return String::new();
        };
        let Some(speaker) = value.get("speaker").and_then(Value::as_str) else {
            return String::new();
        };
        let Some(committed) = value.get("committed").and_then(Value::as_str) else {
            return String::new();
        };

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
            return String::new();
        }

        let mut output = String::new();
        if self.active_speaker.as_deref() != Some(speaker) {
            if self.active_speaker.is_some() {
                output.push('\n');
            }
            output.push_str(speaker);
            output.push_str(": ");
            self.active_speaker = Some(speaker.to_string());
        }
        output.push_str(&delta);
        output
    }

    fn reset(&mut self, session: Option<u64>) {
        self.session = session;
        self.committed.clear();
        self.active_speaker = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use interprocess::local_socket::{GenericNamespaced, ToNsName};

    use crate::follow_stream::{FollowStreamHub, FollowStreamServer, Speaker};
    use crate::managers::transcription::StreamSource;

    use super::*;

    fn render(lines: &[&str]) -> String {
        let mut renderer = DeltaRenderer::default();
        lines
            .iter()
            .map(|line| renderer.push(line).unwrap())
            .collect()
    }

    #[test]
    fn delta_mode_rejects_a_non_streaming_session_without_printing_transcript_text() {
        let input = concat!(
            "{\"t\":\"begin\",\"session\":1,\"streaming\":false}\n",
            "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
        );
        let mut stdout = Vec::new();
        let result = process_stream(input.as_bytes(), &mut stdout, FollowStreamMode::Delta);
        let mut stderr = Vec::new();

        assert_eq!(finish_client(result, &mut stderr), 1);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            format!("{DELTA_REQUIRES_STREAMING_MESSAGE}\n")
        );
    }

    #[test]
    fn json_mode_passes_a_non_streaming_session_through_and_exits_cleanly() {
        let input = concat!(
            "{\"t\":\"begin\",\"session\":1,\"streaming\":false}\n",
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

    #[test]
    fn delta_renderer_handles_a_realistic_multi_speaker_session() {
        let output = render(&[
            r#"{"t":"hello","protocol":1,"version":"test"}"#,
            r#"{"t":"begin","session":1,"streaming":true}"#,
            r#"{"t":"partial","session":1,"speaker":"me","committed":"hel","tentative":"lo"}"#,
            r#"{"t":"partial","session":1,"speaker":"me","committed":"hello ","tentative":"wor"}"#,
            r#"{"t":"partial","session":1,"speaker":"me","committed":"hello world","tentative":""}"#,
            r#"{"t":"partial","session":1,"speaker":"them","committed":"yes","tentative":""}"#,
            r#"{"t":"final","session":1,"text":"Me: hello world\nThem: yes"}"#,
        ]);

        assert_eq!(output, "me: hello world\nthem: yes\n");
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

        hub.begin(true);
        wait_until(|| output.text().contains("\"t\":\"begin\"")).await;
        hub.partial(StreamSource::Mic, "hello ", "wor");
        wait_until(|| output.text().contains("\"t\":\"partial\"")).await;
        hub.finish(Some(Speaker::Me), "Hello world.");
        wait_until(|| output.text().contains("\"t\":\"final\"")).await;

        server.stop();
        client.await.unwrap().unwrap();

        assert_eq!(
            output.text(),
            concat!(
                "{\"t\":\"hello\",\"protocol\":1,\"version\":\"test-version\"}\n",
                "{\"t\":\"begin\",\"session\":1,\"streaming\":true}\n",
                "{\"t\":\"partial\",\"session\":1,\"speaker\":\"me\",\"committed\":\"hello \",\"tentative\":\"wor\"}\n",
                "{\"t\":\"final\",\"session\":1,\"speaker\":\"me\",\"text\":\"Hello world.\"}\n",
            )
        );
    }
}
