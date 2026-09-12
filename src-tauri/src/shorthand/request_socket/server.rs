//! The request socket listener. Mirrors `follow_stream/server.rs`'s
//! lifecycle (retry loop, protected DACL on Windows, peer-euid check on
//! Unix) closely enough to reuse its listener-creation helpers directly
//! rather than re-implementing them; see the `crate::follow_stream`
//! re-exports this module calls into.

use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use interprocess::local_socket::{
    tokio::{Listener, Stream},
    traits::tokio::{Listener as _, Stream as _},
    GenericNamespaced, Name, ToNsName,
};
use serde::Serialize;
use tauri::AppHandle;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, Semaphore},
};

use crate::shorthand::credentials::{
    CredentialError, CredentialSlot, CredentialStatus, CredentialStore,
};

use super::{
    discovery,
    protocol::{error_line, hello_line, ok_line, parse_line, ErrorCode, Request},
};

type TaskHandle = tauri::async_runtime::JoinHandle<()>;

/// Per-connection concurrent-request cap from the wire contract. A 33rd
/// concurrent request on the same connection waits for a permit rather than
/// being rejected: it is ordinary backpressure, not abuse.
const MAX_INFLIGHT_PER_CONNECTION: usize = 32;
/// Connection cap from the wire contract. Unlike the per-connection request
/// cap, a 17th connection is rejected outright — see `accept_loop`.
const MAX_CONNECTIONS: usize = 16;
/// Max NDJSON line size from the wire contract; see `handle_connection`.
const MAX_LINE_BYTES: u64 = 32 * 1024 * 1024;

pub struct RequestSocketServer {
    inner: Mutex<Option<RunningServer>>,
}

struct RunningServer {
    listener: TaskHandle,
    connections: Arc<Mutex<Vec<TaskHandle>>>,
}

impl Default for RequestSocketServer {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl RequestSocketServer {
    pub async fn start(&self, app: &AppHandle, store: Arc<CredentialStore>) -> io::Result<()> {
        let (name, client_path) = listener_name()?;
        self.start_with_name(name, &app.package_info().version.to_string(), store)
            .await?;
        // Not best-effort: a client has no way to find this run's socket
        // other than this file, so a failed write is as bad as the listener
        // never having started, even though the listener itself is already
        // up by this point.
        discovery::write_discovery(&client_path)
    }

    pub(crate) async fn start_with_name(
        &self,
        name: Name<'static>,
        app_version: &str,
        store: Arc<CredentialStore>,
    ) -> io::Result<()> {
        const MAX_ATTEMPTS: usize = 10;
        const RETRY_DELAY: Duration = Duration::from_millis(50);

        for attempt in 1..=MAX_ATTEMPTS {
            match self.start_inner(name.clone(), app_version.to_string(), Arc::clone(&store)) {
                Ok(()) => return Ok(()),
                Err(error) if attempt < MAX_ATTEMPTS && is_retryable_listener_error(&error) => {
                    tokio::time::sleep(RETRY_DELAY).await;
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("listener retry loop always returns")
    }

    fn start_inner(
        &self,
        name: Name<'static>,
        app_version: String,
        store: Arc<CredentialStore>,
    ) -> io::Result<()> {
        let mut running = self.inner.lock().unwrap();
        if running.is_some() {
            return Ok(());
        }

        // Tokio's Windows named-pipe constructor requires an entered runtime
        // with its I/O driver enabled, so enter Tauri's runtime explicitly
        // for construction (mirrors follow_stream::server::start_inner).
        let listener_result = {
            let runtime = tauri::async_runtime::handle();
            let _runtime_guard = runtime.inner().enter();
            crate::follow_stream::create_listener(name)
        };
        let listener = match listener_result {
            Ok(listener) => listener,
            Err(error) => {
                log::error!("Failed to create request-socket listener: {error}");
                crate::shorthand::telemetry::report_error(
                    "request_socket_listen",
                    // The io error kind only: the message can name the
                    // per-user socket path.
                    Some(&format!("{:?}", error.kind())),
                );
                return Err(error);
            }
        };

        let ctx = Arc::new(ConnectionContext::new(store, app_version));
        let connections = Arc::new(Mutex::new(Vec::new()));
        let listener_handle =
            tauri::async_runtime::spawn(accept_loop(listener, ctx, Arc::clone(&connections)));
        *running = Some(RunningServer {
            listener: listener_handle,
            connections,
        });
        log::info!("Request-socket listener started");
        Ok(())
    }

    pub fn stop(&self) {
        let Some(running) = self.inner.lock().unwrap().take() else {
            return;
        };

        running.listener.abort();
        let mut connections = running.connections.lock().unwrap();
        for connection in connections.drain(..) {
            connection.abort();
        }
        log::info!("Request-socket listener stopped");
    }

    #[cfg(test)]
    fn is_running(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }
}

impl Drop for RequestSocketServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_retryable_listener_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::AddrInUse | io::ErrorKind::AlreadyExists
    )
}

/// The production listener name and the path a client needs in order to
/// connect: `shorthand.request.<identity>` on Windows (a named pipe, same
/// namespaced-name mechanism follow-stream uses), `<config dir>/request.sock`
/// on Unix (a real filesystem path, because unlike follow-stream's clients
/// — this app's own CLI, which can derive the deterministic name itself —
/// the request socket's clients are separate processes that only learn the
/// path from the discovery file).
#[cfg(windows)]
fn listener_name() -> io::Result<(Name<'static>, String)> {
    let identity = crate::follow_stream::current_identity()?;
    let text = format!("shorthand.request.{identity}");
    let client_path = format!(r"\\.\pipe\{text}");
    Ok((text.to_ns_name::<GenericNamespaced>()?, client_path))
}

#[cfg(unix)]
fn listener_name() -> io::Result<(Name<'static>, String)> {
    use interprocess::{local_socket::ToFsName, os::unix::local_socket::FilesystemUdSocket};

    let path = discovery::config_directory()?.join("request.sock");
    // A prior unclean shutdown can leave the socket file behind; binding to
    // an existing path fails, so clear it first. Best-effort: a removal
    // failure here just surfaces as the bind error from `create_listener`.
    if let Err(error) = std::fs::remove_file(&path) {
        if error.kind() != io::ErrorKind::NotFound {
            log::warn!("Could not remove stale request-socket file {path:?}: {error}");
        }
    }
    let client_path = path.to_string_lossy().into_owned();
    Ok((path.to_fs_name::<FilesystemUdSocket>()?, client_path))
}

async fn accept_loop(
    listener: Listener,
    ctx: Arc<ConnectionContext>,
    connection_handles: Arc<Mutex<Vec<TaskHandle>>>,
) {
    loop {
        let stream = match listener.accept().await {
            Ok(stream) => stream,
            Err(error) => {
                // Mirrors follow_stream::server::accept_loop: listener
                // errors can be transient, so retry with a delay rather than
                // let one bad accept spin the runtime or end the listener.
                log::warn!("Request-socket accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        #[cfg(unix)]
        if !crate::follow_stream::peer_is_current_user(&stream) {
            continue;
        }

        let mut handles = connection_handles.lock().unwrap();
        handles.retain(|handle| !handle.inner().is_finished());
        if handles.len() >= MAX_CONNECTIONS {
            drop(handles);
            tauri::async_runtime::spawn(reject_over_connection_limit(stream));
            continue;
        }

        let connection_ctx = Arc::clone(&ctx);
        handles.push(tauri::async_runtime::spawn(async move {
            let (reader, writer) = stream.split();
            handle_connection(reader, writer, connection_ctx).await;
        }));
    }
}

async fn reject_over_connection_limit(stream: Stream) {
    let (_, mut writer) = stream.split();
    let line = error_line(
        "",
        ErrorCode::Limit,
        "maximum number of request-socket connections reached",
    );
    if let Err(error) = async {
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await
    }
    .await
    {
        log::debug!("Request-socket over-limit write ended early: {error}");
    }
}

/// Per-connection state the credential and (later) http/ws handlers share.
/// `http`, `inflight` and `streams` are not read yet in this task: A5
/// (`http.fetch`/`http.abort`) and A6 (`ws.*`) are the first handlers that
/// populate and drain them. They are part of the struct now, `#[allow
/// (dead_code)]` in the meantime, so its shape does not change under those
/// tasks.
pub(crate) struct ConnectionContext {
    store: Arc<CredentialStore>,
    version: String,
    #[allow(dead_code)]
    pub(crate) http: reqwest::Client,
    #[allow(dead_code)]
    pub(crate) inflight: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    #[allow(dead_code)]
    pub(crate) streams: Mutex<HashMap<String, WsHandle>>,
}

/// Placeholder for the ws-relay handle A6 introduces; present now only so
/// `ConnectionContext::streams` already has a concrete value type.
#[allow(dead_code)]
pub(crate) struct WsHandle {
    pub(crate) abort: tokio::task::AbortHandle,
}

impl ConnectionContext {
    fn new(store: Arc<CredentialStore>, version: String) -> Self {
        Self {
            store,
            version,
            // No redirects: `http.fetch`'s origin check (A5) must not be
            // bypassed by a redirect to a different origin. No timeout here
            // — per-request timeouts are applied when A5 issues the request.
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("a client with no proxy/TLS overrides always builds"),
            inflight: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests(store: Arc<CredentialStore>, version: &str) -> Self {
        Self::new(store, version.to_string())
    }
}

/// Serves one connection: writes `hello`, then reads NDJSON lines and
/// dispatches each to its own task so a slow credential lookup cannot block
/// the next request on the same connection. A single writer task owns the
/// transport's write half so responses from those concurrent tasks (and
/// events, from A6 onward) are never interleaved mid-line.
pub(crate) async fn handle_connection<R, W>(reader: R, writer: W, ctx: Arc<ConnectionContext>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<String>(64);

    let writer_task = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(line) = rx.recv().await {
            if let Err(error) = writer.write_all(line.as_bytes()).await {
                log::debug!("Request-socket write failed: {error}");
                break;
            }
            if let Err(error) = writer.flush().await {
                log::debug!("Request-socket flush failed: {error}");
                break;
            }
        }
    });

    if tx.send(hello_line(&ctx.version)).await.is_err() {
        drop(tx);
        let _ = writer_task.await;
        return;
    }

    let semaphore = Arc::new(Semaphore::new(MAX_INFLIGHT_PER_CONNECTION));
    // `take`'s limit is reset before every line: it bounds one line, not the
    // whole connection. Reaching it without a '\n' is indistinguishable from
    // real EOF to `read_until` (`Take` reports 0 further bytes either way),
    // so the length check below is what tells the two apart.
    let mut reader = BufReader::new(reader).take(MAX_LINE_BYTES + 1);
    let mut buf = Vec::new();

    loop {
        buf.clear();
        reader.set_limit(MAX_LINE_BYTES + 1);
        let read = reader.read_until(b'\n', &mut buf).await;
        let bytes_read = match read {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                log::debug!("Request-socket read failed: {error}");
                break;
            }
        };
        if bytes_read == 0 {
            break; // Real EOF: the peer closed the connection.
        }
        if buf.last() != Some(&b'\n') {
            if buf.len() as u64 > MAX_LINE_BYTES {
                let _ = tx
                    .send(error_line(
                        "",
                        ErrorCode::TooLarge,
                        "line exceeds the 32 MiB limit",
                    ))
                    .await;
            }
            // Either oversized (reported above) or a genuine EOF mid-line
            // (nothing worth reporting); both end the connection.
            break;
        }

        let Ok(text) = std::str::from_utf8(&buf[..buf.len() - 1]) else {
            let _ = tx
                .send(error_line(
                    "",
                    ErrorCode::BadRequest,
                    "line is not valid UTF-8",
                ))
                .await;
            continue;
        };
        let line = text.trim_end_matches('\r');

        match parse_line(line) {
            Ok((id, request)) => {
                let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                    break; // Semaphore closed: the connection is tearing down.
                };
                let task_ctx = Arc::clone(&ctx);
                let task_tx = tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let response = dispatch(&id, request, &task_ctx).await;
                    let _ = task_tx.send(response).await;
                });
            }
            Err(code) => {
                // See parse_line's doc comment: a line that failed to parse
                // never yields a trustworthy id to echo.
                let _ = tx
                    .send(error_line("", code, "the request could not be parsed"))
                    .await;
            }
        }
    }

    drop(tx);
    let _ = writer_task.await;
}

async fn dispatch(id: &str, request: Request, ctx: &ConnectionContext) -> String {
    match request {
        Request::CredentialSet { slot, secret } => credential_set(id, ctx, slot, secret).await,
        Request::CredentialClear { slot } => credential_clear(id, ctx, slot).await,
        Request::CredentialStatus { slots } => credential_status(id, ctx, slots).await,
        Request::HttpFetch(_)
        | Request::HttpAbort { .. }
        | Request::WsOpen(_)
        | Request::WsSend { .. }
        | Request::WsClose { .. } => {
            // A5 implements http.*, A6 implements ws.*. Both answer the same
            // code until then, so a client cannot tell "this server has
            // never heard of this method" apart from "not implemented yet"
            // — it doesn't need to.
            error_line(id, ErrorCode::UnknownMethod, "not yet")
        }
    }
}

async fn credential_set(
    id: &str,
    ctx: &ConnectionContext,
    slot: CredentialSlot,
    secret: String,
) -> String {
    let store = Arc::clone(&ctx.store);
    // The keyring API blocks, so it runs on the blocking pool rather than
    // stalling this connection's other in-flight requests.
    match tokio::task::spawn_blocking(move || store.set(&slot, &secret)).await {
        Ok(Ok(())) => ok_line(id, serde_json::json!({})),
        Ok(Err(error)) => credential_error_line(id, &error),
        Err(join_error) => error_line(
            id,
            ErrorCode::CredentialUnavailable,
            &join_error.to_string(),
        ),
    }
}

async fn credential_clear(id: &str, ctx: &ConnectionContext, slot: CredentialSlot) -> String {
    let store = Arc::clone(&ctx.store);
    match tokio::task::spawn_blocking(move || store.clear(&slot)).await {
        Ok(Ok(())) => ok_line(id, serde_json::json!({})),
        Ok(Err(error)) => credential_error_line(id, &error),
        Err(join_error) => error_line(
            id,
            ErrorCode::CredentialUnavailable,
            &join_error.to_string(),
        ),
    }
}

async fn credential_status(
    id: &str,
    ctx: &ConnectionContext,
    slots: Vec<CredentialSlot>,
) -> String {
    let store = Arc::clone(&ctx.store);
    let result = tokio::task::spawn_blocking(move || {
        slots
            .into_iter()
            .map(|slot| {
                let status = store.status(&slot);
                StatusEntry { slot, status }
            })
            .collect::<Vec<_>>()
    })
    .await;
    match result {
        Ok(statuses) => ok_line(
            id,
            serde_json::to_value(StatusResult { statuses })
                .expect("status result always serializes"),
        ),
        Err(join_error) => error_line(
            id,
            ErrorCode::CredentialUnavailable,
            &join_error.to_string(),
        ),
    }
}

#[derive(Serialize)]
struct StatusEntry {
    slot: CredentialSlot,
    status: CredentialStatus,
}

#[derive(Serialize)]
struct StatusResult {
    statuses: Vec<StatusEntry>,
}

/// `CredentialError`'s `Display` never includes the secret (see
/// credentials.rs's module doc); this only chooses the wire error code.
fn credential_error_line(id: &str, error: &CredentialError) -> String {
    let code = match error {
        CredentialError::Invalid(_) => ErrorCode::BadRequest,
        CredentialError::Unavailable(_) => ErrorCode::CredentialUnavailable,
    };
    error_line(id, code, &error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    use interprocess::local_socket::{
        traits::tokio::Stream as _, GenericNamespaced, Name, ToNsName,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use crate::shorthand::credentials::MemoryBackend;

    use super::*;

    fn store() -> Arc<CredentialStore> {
        Arc::new(CredentialStore::with_backend(Box::new(
            MemoryBackend::default(),
        )))
    }

    #[tokio::test]
    async fn credential_round_trip_over_the_wire() {
        let store = Arc::new(CredentialStore::with_backend(Box::new(
            MemoryBackend::default(),
        )));
        let (client, server) = tokio::io::duplex(1 << 16);
        let (server_read, server_write) = tokio::io::split(server);
        let ctx = Arc::new(ConnectionContext::for_tests(store.clone(), "0.5.0"));
        tokio::spawn(handle_connection(server_read, server_write, ctx));
        let (mut read, mut write) = tokio::io::split(client);
        let mut lines = BufReader::new(&mut read).lines();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("{\"t\":\"hello\""));
        write.write_all(b"{\"id\":\"1\",\"method\":\"credential.status\",\"params\":{\"slots\":[{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"}]}}\n").await.unwrap();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .contains("\"status\":\"missing\""));
        write.write_all(b"{\"id\":\"2\",\"method\":\"credential.set\",\"params\":{\"slot\":{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"},\"secret\":\"sk-test\"}}\n").await.unwrap();
        assert_eq!(
            lines.next_line().await.unwrap().unwrap(),
            "{\"id\":\"2\",\"ok\":true,\"result\":{}}"
        );
        write.write_all(b"{\"id\":\"3\",\"method\":\"credential.status\",\"params\":{\"slots\":[{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"}]}}\n").await.unwrap();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .contains("\"status\":\"configured\""));
    }

    #[tokio::test]
    async fn oversized_line_closes_the_connection_with_too_large() {
        let (client, server) = tokio::io::duplex(1 << 16);
        let (server_read, server_write) = tokio::io::split(server);
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        tokio::spawn(handle_connection(server_read, server_write, ctx));
        let (mut read, mut write) = tokio::io::split(client);
        let mut lines = BufReader::new(&mut read).lines();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("{\"t\":\"hello\""));

        let huge = vec![b'a'; (MAX_LINE_BYTES + 1) as usize];
        let writer_task = tokio::spawn(async move {
            // The client deliberately never sends a newline; write_all still
            // completes once the server has read everything (the duplex
            // applies backpressure), and dropping `write` afterwards signals
            // EOF once the server closes its own end too.
            let _ = write.write_all(&huge).await;
        });

        assert_eq!(
            lines.next_line().await.unwrap().unwrap(),
            "{\"id\":\"\",\"ok\":false,\"error\":{\"code\":\"too_large\",\"message\":\"line exceeds the 32 MiB limit\"}}"
        );
        assert!(
            lines.next_line().await.unwrap().is_none(),
            "connection should close after a too_large line"
        );

        writer_task.await.unwrap();
    }

    #[tokio::test]
    async fn malformed_json_answers_bad_request_and_keeps_the_connection() {
        let (client, server) = tokio::io::duplex(1 << 16);
        let (server_read, server_write) = tokio::io::split(server);
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        tokio::spawn(handle_connection(server_read, server_write, ctx));
        let (mut read, mut write) = tokio::io::split(client);
        let mut lines = BufReader::new(&mut read).lines();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("{\"t\":\"hello\""));

        write.write_all(b"{\n").await.unwrap();
        let error = lines.next_line().await.unwrap().unwrap();
        assert!(error.starts_with("{\"id\":\"\",\"ok\":false,\"error\":{\"code\":\"bad_request\""));

        write.write_all(b"{\"id\":\"9\",\"method\":\"credential.status\",\"params\":{\"slots\":[{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"}]}}\n").await.unwrap();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .contains("\"status\":\"missing\""));
    }

    static NEXT_TEST_NAME: AtomicU64 = AtomicU64::new(1);

    fn unique_name_text(test: &str) -> String {
        format!(
            "shorthand.request.test.{test}.{}.{}",
            std::process::id(),
            NEXT_TEST_NAME.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn unique_name(test: &str) -> Name<'static> {
        unique_name_text(test)
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
    }

    #[tokio::test]
    async fn transport_round_trip_preserves_exact_ndjson_order() {
        let name = unique_name("round_trip");
        let server = RequestSocketServer::default();
        server
            .start_with_name(name.clone(), "0.5.0", store())
            .await
            .unwrap();

        let stream = tokio::time::timeout(Duration::from_secs(2), Stream::connect(name))
            .await
            .expect("client connection timed out")
            .expect("client failed to connect");
        let mut stream = BufReader::new(stream);

        let mut hello = String::new();
        stream.read_line(&mut hello).await.unwrap();
        assert_eq!(
            hello,
            "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.5.0\",\"capabilities\":[\"credential\",\"http-fetch\",\"ws-relay\"]}\n"
        );

        stream.write_all(b"{\"id\":\"r1\",\"method\":\"credential.status\",\"params\":{\"slots\":[{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"}]}}\n").await.unwrap();
        let mut status = String::new();
        stream.read_line(&mut status).await.unwrap();
        assert!(status.contains("\"status\":\"missing\""));

        stream.write_all(b"{\"id\":\"r2\",\"method\":\"credential.set\",\"params\":{\"slot\":{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"},\"secret\":\"sk-test\"}}\n").await.unwrap();
        let mut set_result = String::new();
        stream.read_line(&mut set_result).await.unwrap();
        assert_eq!(set_result, "{\"id\":\"r2\",\"ok\":true,\"result\":{}}\n");

        server.stop();
        assert!(!server.is_running());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn created_listener_has_protected_current_user_only_dacl() {
        use windows::{
            core::PWSTR,
            Win32::{
                Foundation::{CloseHandle, LocalFree, HLOCAL},
                Security::{
                    Authorization::{
                        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo,
                        SDDL_REVISION_1, SE_FILE_OBJECT,
                    },
                    DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
                },
                Storage::FileSystem::{
                    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                    OPEN_EXISTING, READ_CONTROL,
                },
            },
        };

        let test_name = unique_name_text("kernel_dacl");
        let name = test_name.clone().to_ns_name::<GenericNamespaced>().unwrap();
        let _listener = crate::follow_stream::create_listener(name).unwrap();
        // `current_identity()` delegates to the SID on Windows; used here
        // instead of re-exporting `current_user_sid` too, which nothing
        // outside a test would otherwise call.
        let sid = crate::follow_stream::current_identity().unwrap();

        let pipe_path = widestring::U16CString::from_str(format!(r"\\.\pipe\{test_name}")).unwrap();
        let handle = unsafe {
            CreateFileW(
                windows::core::PCWSTR(pipe_path.as_ptr()),
                READ_CONTROL.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .unwrap();

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            )
            .ok()
            .expect("GetSecurityInfo should read back the pipe's DACL");
        }

        let mut rendered = PWSTR::null();
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut rendered,
                None,
            )
            .unwrap();
        }
        let sddl = unsafe { rendered.to_string().unwrap() };

        unsafe {
            let _ = LocalFree(Some(HLOCAL(rendered.0.cast())));
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            CloseHandle(handle).unwrap();
        }

        assert!(sddl.starts_with("D:P"));
        assert!(sddl.contains(&sid));
        assert!(!sddl.contains(";;;WD)"), "Everyone must not have access");
        assert!(!sddl.contains(";;;AN)"), "Anonymous must not have access");
        assert_eq!(sddl, format!("D:P(A;;FA;;;{sid})"));
    }
}
