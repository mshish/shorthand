//! The request socket listener. Mirrors `follow_stream/server.rs`'s
//! lifecycle (retry loop, protected DACL on Windows, peer-euid check on
//! Unix) closely enough to reuse its listener-creation helpers directly
//! rather than re-implementing them; see the `crate::follow_stream`
//! re-exports this module calls into.

use std::{
    collections::HashMap,
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
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
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
};

use crate::shorthand::credentials::{
    CredentialError, CredentialSlot, CredentialStatus, CredentialStore,
};

use super::{
    discovery, http_proxy,
    protocol::{
        error_line, hello_line, ok_line, parse_line, ErrorCode, HttpFetchParams, Request,
        StatusSlot,
    },
    ws_relay::{self, WsHandle},
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
/// Per-connection cap on open `ws.*` streams from the wire contract. Enforced
/// as a `Semaphore` on `ConnectionContext` (see `stream_capacity`) rather
/// than a post-hoc length check on `streams`, so two concurrent `ws.open`
/// calls racing the 8th slot cannot both succeed.
pub(crate) const MAX_WS_STREAMS_PER_CONNECTION: usize = 8;

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
        // up by this point. Tear that listener back down rather than
        // returning an error while leaving it running with no discovery file
        // pointing at it — a caller that sees `start` fail should be able to
        // assume nothing is left listening.
        if let Err(error) = discovery::write_discovery(&client_path) {
            self.stop();
            return Err(error);
        }
        Ok(())
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

        let shared = Arc::new(ServerShared::new(store, app_version));
        let connections = Arc::new(Mutex::new(Vec::new()));
        let listener_handle =
            tauri::async_runtime::spawn(accept_loop(listener, shared, Arc::clone(&connections)));
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
        drop(connections);
        // Otherwise a stale file keeps pointing a future client at a socket
        // nobody is listening on anymore.
        discovery::remove_discovery();
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

    let dir = discovery::config_directory()?;
    // On a fresh install nothing has created the config directory yet —
    // discovery.rs normally does, but only after this bind succeeds — so
    // binding here first would fail `NotFound` every time. Create it eagerly
    // instead of waiting for `write_discovery`.
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("request.sock");

    // A prior unclean shutdown can leave the socket file behind; binding to
    // an existing path fails, so it normally needs clearing first. But a
    // *live* instance can also be listening at this exact path (a second app
    // launch racing the first), and unlinking it out from under that listener
    // would strand its clients. Only remove the file once a connect attempt
    // proves nothing is listening: `ConnectionRefused` (the socket file is
    // stale) or `NotFound` (nothing to remove). Any other outcome — including
    // a successful connect — leaves the file alone.
    match std::os::unix::net::UnixStream::connect(&path) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("a request-socket listener is already running at {path:?}"),
            ));
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            if let Err(remove_error) = std::fs::remove_file(&path) {
                if remove_error.kind() != io::ErrorKind::NotFound {
                    log::warn!(
                        "Could not remove stale request-socket file {path:?}: {remove_error}"
                    );
                }
            }
        }
        Err(error) => {
            // Some other probe failure (e.g. permission denied): leave the
            // file alone and let the bind attempt below surface the real
            // problem instead of guessing.
            log::warn!("Could not probe existing request-socket file {path:?}: {error}");
        }
    }

    let client_path = path.to_string_lossy().into_owned();
    Ok((path.to_fs_name::<FilesystemUdSocket>()?, client_path))
}

async fn accept_loop(
    listener: Listener,
    shared: Arc<ServerShared>,
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
            tauri::async_runtime::spawn(reject_over_connection_limit(
                stream,
                shared.version.clone(),
            ));
            continue;
        }

        // A fresh `ConnectionContext` per accepted connection, not a clone of
        // one shared across the whole server: `inflight` (and, from A6,
        // `streams`) are request-id namespaces that the wire contract scopes
        // to one connection, so two connections must never share a map. Only
        // the store, version and HTTP client (which shares its connection
        // pool across clones) come from `shared`. See review finding
        // Critical-1 in app-A5-review-findings.md.
        let connection_ctx = Arc::new(ConnectionContext::new(&shared));
        handles.push(tauri::async_runtime::spawn(async move {
            let (reader, writer) = stream.split();
            handle_connection(reader, writer, connection_ctx).await;
        }));
    }
}

/// Every connection's mandatory first line is `hello` (the wire contract's
/// only promise a client can rely on before it has read anything else); a
/// client that gets an error line first, with nothing preceding it, cannot
/// tell this apart from talking to something that never came up at all. So
/// this still writes `hello`, then the limit error, then closes.
async fn reject_over_connection_limit(stream: Stream, version: String) {
    let (_, mut writer) = stream.split();
    let hello = hello_line(&version);
    let line = error_line(
        "",
        ErrorCode::Limit,
        "maximum number of request-socket connections reached",
    );
    if let Err(error) = async {
        writer.write_all(hello.as_bytes()).await?;
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await
    }
    .await
    {
        log::debug!("Request-socket over-limit write ended early: {error}");
    }
}

/// State shared by every connection: the credential store, the advertised
/// app version, and one `reqwest::Client`. The client is built once here and
/// cloned into each `ConnectionContext` — a `reqwest::Client` clone shares
/// the same underlying connection pool, so this still gets connection reuse
/// across connections without sharing any per-connection request state (see
/// `ConnectionContext`'s doc comment and review finding Critical-1).
struct ServerShared {
    store: Arc<CredentialStore>,
    version: String,
    http: reqwest::Client,
}

impl ServerShared {
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
        }
    }
}

/// Per-connection state the credential, http and ws handlers share. `store`
/// and `http` are `pub(crate)` because `http_proxy.rs`'s handlers (A5) read
/// them directly rather than through server.rs; `ws_relay.rs`'s handlers (A6)
/// do the same for `store`, `stream_capacity` and `streams`. `inflight` and
/// `streams` are request-id namespaces the wire contract scopes to one
/// connection — each connection gets its own `ConnectionContext`, built by
/// `accept_loop` from the server-wide `ServerShared`, precisely so two
/// connections never share either map (see review finding Critical-1).
pub(crate) struct ConnectionContext {
    pub(crate) store: Arc<CredentialStore>,
    version: String,
    pub(crate) http: reqwest::Client,
    pub(crate) inflight: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    pub(crate) streams: Mutex<HashMap<String, WsHandle>>,
    /// Gates `ws.open` at `MAX_WS_STREAMS_PER_CONNECTION` permits. An
    /// `Arc` of its own (not just a field behind `ConnectionContext`'s Arc)
    /// because `Semaphore::try_acquire_owned` needs to hold a permit whose
    /// lifetime is independent of any single request — the permit ends up
    /// stored inside the `WsHandle` for as long as that stream stays open,
    /// well after `ws_relay::run_ws_open` itself has returned.
    pub(crate) stream_capacity: Arc<tokio::sync::Semaphore>,
    next_stream_id: AtomicU64,
}

impl ConnectionContext {
    fn new(shared: &ServerShared) -> Self {
        Self {
            store: Arc::clone(&shared.store),
            version: shared.version.clone(),
            http: shared.http.clone(),
            inflight: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            stream_capacity: Arc::new(tokio::sync::Semaphore::new(MAX_WS_STREAMS_PER_CONNECTION)),
            next_stream_id: AtomicU64::new(1),
        }
    }

    /// A panic while a handler holds `inflight`'s lock must not wedge every
    /// later `http.fetch`/`http.abort` on this connection — same reasoning as
    /// `credentials::lock` (see that module's doc comment). Used by
    /// server.rs and http_proxy.rs alike so neither reaches for the bare
    /// `.lock().unwrap()` the review flagged (Minor-10).
    pub(crate) fn inflight_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, tokio::task::AbortHandle>> {
        self.inflight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Same reasoning as `inflight_lock`, for the `ws.*` stream map.
    pub(crate) fn streams_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, WsHandle>> {
        self.streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A stream id unique within this connection, per the wire contract
    /// ("server-generated stream id unique per connection"). Connection-local
    /// rather than global: two different connections legitimately reusing
    /// `s1` is fine, since `streams` is itself per-connection.
    pub(crate) fn next_stream_id(&self) -> String {
        format!("s{}", self.next_stream_id.fetch_add(1, Ordering::Relaxed))
    }

    #[cfg(test)]
    pub(crate) fn for_tests(store: Arc<CredentialStore>, version: &str) -> Self {
        let shared = ServerShared::new(store, version.to_string());
        Self::new(&shared)
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
        // Checked before the newline test, not only when one is missing: a
        // line whose *content* is exactly `MAX_LINE_BYTES` still produces
        // `MAX_LINE_BYTES + 1` total bytes once its trailing `\n` is
        // included, which is over the documented 32 MiB line limit even
        // though `read_until` did find its delimiter. Checking length first
        // catches that boundary case instead of silently accepting it.
        if buf.len() as u64 > MAX_LINE_BYTES {
            let _ = tx
                .send(error_line(
                    "",
                    ErrorCode::TooLarge,
                    "line exceeds the 32 MiB limit",
                ))
                .await;
            break;
        }
        if buf.last() != Some(&b'\n') {
            // Genuine EOF mid-line (not oversized, or it would have been
            // caught above): nothing worth reporting.
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
                let task_ctx = Arc::clone(&ctx);
                let task_tx = tx.clone();
                match request {
                    // `http.fetch` sends several lines over the connection's
                    // lifetime (an `ok`, then streamed `http.body`/`http.end`),
                    // not the one-line-back shape `dispatch` gives every other
                    // method, so it is spawned directly rather than through
                    // it. The abort handle is registered before this task is
                    // polled at all, so an `http.abort` for this id arriving
                    // on the very next line can never race an unregistered id
                    // — see `spawn_http_fetch`'s doc comment for how that
                    // ordering is actually enforced.
                    Request::HttpFetch(params) => {
                        let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                            break; // Semaphore closed: the connection is tearing down.
                        };
                        spawn_http_fetch(id, params, task_ctx, task_tx, permit);
                    }
                    // No permit here: once 32 fetches hold every permit on
                    // this connection, `http.abort` is the only way to free
                    // one short of the (renewable) per-request timeout. If it
                    // queued behind the acquire like every other method, a
                    // client that fills the limit and then cancels one could
                    // never get the cancel through. See review finding
                    // Important-6.
                    Request::HttpAbort { request: target } => {
                        tokio::spawn(async move {
                            http_proxy::handle_abort(&id, &target, &task_ctx, &task_tx).await;
                        });
                    }
                    other => {
                        let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                            break; // Semaphore closed: the connection is tearing down.
                        };
                        tokio::spawn(async move {
                            let _permit = permit;
                            let response = dispatch(&id, other, &task_ctx, &task_tx).await;
                            if let Some(response) = response {
                                let _ = task_tx.send(response).await;
                            }
                        });
                    }
                }
            }
            Err((id, code)) => {
                // See parse_line's doc comment: `id` is only ever "" when
                // the line itself gave us nothing trustworthy to echo.
                let _ = tx
                    .send(error_line(&id, code, "the request could not be parsed"))
                    .await;
            }
        }
    }

    drop(tx);
    // Every fetch still in flight on this connection holds its own clone of
    // `tx` (see `spawn_http_fetch`), so without this the writer task's
    // channel would never close for a connection whose upstream answered
    // headers and then stalled mid-body: nothing else notices the socket is
    // dead, `handle_connection` never returns, and the slot it holds in
    // `accept_loop`'s connection count is never freed. Aborting every
    // registered fetch here — rather than waiting out each one's own
    // (renewable, per finding 5) 15-minute bound — is what actually releases
    // them. See review finding Important-7.
    for (_, handle) in ctx.inflight_lock().drain() {
        handle.abort();
    }
    // Same reasoning for `ws.*`: a stream nobody is reading events for
    // anymore must not keep its reader task (and the upstream TCP/TLS
    // connection it holds open) alive indefinitely. Aborting the reader task
    // drops its half of the split `WebSocketStream`; once the writer-side
    // `WsHandle` in the same drained entry is dropped too, nothing keeps the
    // upstream connection open.
    for (_, handle) in ctx.streams_lock().drain() {
        handle.abort.abort();
    }
    let _ = writer_task.await;
}

/// Spawns `http.fetch` as its own task rather than routing it through
/// `dispatch`: unlike every other method, it writes more than one line over
/// the connection's lifetime (an `ok`, then streamed `http.body`/`http.end`
/// events), so it needs direct access to the shared writer channel instead of
/// handing back a single response line.
///
/// The abort handle is registered in `ctx.inflight` before the task body
/// (`http_proxy::run_fetch`) ever runs — not just before `spawn_http_fetch`
/// returns. Tokio's runtime is multi-threaded, so a spawned task can start
/// running on another worker immediately; without the `oneshot` gate below, a
/// task that fails fast (e.g. `origin_mismatch`, checked before any network
/// I/O) could finish and have its `InflightGuard` remove nothing from the map
/// before `insert` even runs, leaking the entry forever (review finding
/// Critical-2). The gate makes "registered" happen-before "polled", which is
/// exactly the ordering an `http.abort` for this id arriving on the very next
/// line needs.
fn spawn_http_fetch(
    id: String,
    params: HttpFetchParams,
    ctx: Arc<ConnectionContext>,
    tx: mpsc::Sender<String>,
    permit: OwnedSemaphorePermit,
) {
    let id_for_map = id.clone();
    let ctx_for_insert = Arc::clone(&ctx);
    let (start_tx, start_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _permit = permit;
        if start_rx.await.is_err() {
            // The sender side was dropped without a signal, which only
            // happens if `insert` below panicked — nothing was registered,
            // so there is nothing to run for.
            return;
        }
        http_proxy::run_fetch(ctx, id, params, tx).await;
    });
    ctx_for_insert
        .inflight_lock()
        .insert(id_for_map, handle.abort_handle());
    let _ = start_tx.send(());
}

/// Handles every method except `http.fetch` and `http.abort` — both are
/// routed directly from `handle_connection`'s read loop instead: `http.fetch`
/// because it writes more than one reply line (see `spawn_http_fetch`),
/// `http.abort` because it must skip the in-flight permit that fetches
/// acquire (see the read loop's `Request::HttpAbort` arm and review finding
/// Important-6). Every method `dispatch` does handle answers with exactly one
/// response line, which the caller sends — including `ws.open`, whose
/// background reader task then goes on to send its own `ws.message`/
/// `ws.closed`/`ws.error` events straight over `tx`, independently of this
/// function's own single reply. `ctx` is `&Arc<ConnectionContext>` (not
/// `&ConnectionContext`, unlike every other private helper here) precisely so
/// `ws_relay::run_ws_open` can clone it into that longer-lived task.
async fn dispatch(
    id: &str,
    request: Request,
    ctx: &Arc<ConnectionContext>,
    tx: &mpsc::Sender<String>,
) -> Option<String> {
    match request {
        Request::CredentialSet { slot, secret } => {
            Some(credential_set(id, ctx, slot, secret).await)
        }
        Request::CredentialClear { slot } => Some(credential_clear(id, ctx, slot).await),
        Request::CredentialStatus { slots } => Some(credential_status(id, ctx, slots).await),
        Request::HttpFetch(_) | Request::HttpAbort { .. } => {
            // Not `unreachable!`: the read loop routing both of these away
            // from `dispatch` is a call-site choice, not something the
            // compiler enforces, so a future edit that forgets one of those
            // special cases should get a wire error instead of panicking the
            // connection's request task (review finding Minor-9).
            Some(error_line(
                id,
                ErrorCode::BadRequest,
                "this method must not be routed through dispatch",
            ))
        }
        Request::WsOpen(params) => {
            Some(ws_relay::run_ws_open(id, Arc::clone(ctx), tx.clone(), params).await)
        }
        Request::WsSend { stream, data } => {
            Some(ws_relay::run_ws_send(id, ctx, stream, data).await)
        }
        Request::WsClose {
            stream,
            code,
            reason,
        } => Some(ws_relay::run_ws_close(id, ctx, tx, stream, code, reason).await),
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
        Err(join_error) => blocking_pool_error_line(id, "credential.set", &join_error),
    }
}

async fn credential_clear(id: &str, ctx: &ConnectionContext, slot: CredentialSlot) -> String {
    let store = Arc::clone(&ctx.store);
    match tokio::task::spawn_blocking(move || store.clear(&slot)).await {
        Ok(Ok(())) => ok_line(id, serde_json::json!({})),
        Ok(Err(error)) => credential_error_line(id, &error),
        Err(join_error) => blocking_pool_error_line(id, "credential.clear", &join_error),
    }
}

async fn credential_status(id: &str, ctx: &ConnectionContext, slots: Vec<StatusSlot>) -> String {
    let store = Arc::clone(&ctx.store);
    let result = tokio::task::spawn_blocking(move || {
        slots
            .into_iter()
            .map(|StatusSlot { raw, canonical }| {
                let status = store.status(&canonical);
                StatusEntry { slot: raw, status }
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
        Err(join_error) => blocking_pool_error_line(id, "credential.status", &join_error),
    }
}

/// A `JoinError` from the blocking pool means the credential task panicked or
/// was cancelled, not anything about the request itself — its `Display` can
/// include a panic payload, which may not be safe to hand to a client. `code`
/// stays logged, not sent; `id` still needs its one reply.
fn blocking_pool_error_line(id: &str, method: &str, error: &tokio::task::JoinError) -> String {
    log::error!("{method} did not complete on the blocking pool: {error}");
    error_line(
        id,
        ErrorCode::CredentialUnavailable,
        "the credential store did not answer",
    )
}

/// `slot` carries the raw JSON the client sent (see `StatusSlot`'s doc
/// comment), not `CredentialSlot`, so the echo cannot differ from what the
/// client asked about even after canonicalisation.
#[derive(Serialize)]
struct StatusEntry {
    slot: serde_json::Value,
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

    /// Every request on a connection is dispatched to its own task (see
    /// `handle_connection`'s doc comment), so their replies race each other
    /// into the single writer task's channel. This pins that the writer
    /// task's one-line-at-a-time sends keep each reply intact and separate —
    /// never merged, truncated, or dropped — even under real concurrency,
    /// by firing a batch of distinctly-id'd requests at once and checking
    /// every id comes back exactly once as a well-formed JSON line.
    #[tokio::test]
    async fn concurrent_credential_status_requests_do_not_interleave_or_drop() {
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

        const REQUESTS: usize = 20;
        let mut batch = String::new();
        for i in 0..REQUESTS {
            batch.push_str(&format!(
                "{{\"id\":\"r{i}\",\"method\":\"credential.status\",\"params\":{{\"slots\":[{{\"kind\":\"notes-llm\",\"provider\":\"openai\",\"origin\":\"https://api.openai.com\"}}]}}}}\n"
            ));
        }
        // One write, so every request is queued before the server has
        // answered any of them — the concurrency this test exists to check.
        write.write_all(batch.as_bytes()).await.unwrap();

        let mut seen_ids = std::collections::HashSet::new();
        for _ in 0..REQUESTS {
            let line = lines
                .next_line()
                .await
                .unwrap()
                .expect("connection ended before every reply arrived");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|error| {
                panic!("reply was not valid JSON ({error}), interleaving suspected: {line}")
            });
            assert_eq!(value["ok"], true, "unexpected reply: {line}");
            assert!(
                line.contains("\"status\":\"missing\""),
                "unexpected reply: {line}"
            );
            let id = value["id"].as_str().unwrap().to_string();
            assert!(
                seen_ids.insert(id),
                "duplicate id in reply, interleaving suspected: {line}"
            );
        }
        assert_eq!(seen_ids.len(), REQUESTS);
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

    /// The 17th concurrent connection must be turned away, but still gets
    /// `hello` first — see `reject_over_connection_limit`'s doc comment — so
    /// this checks both the limit itself and that ordering, then that the
    /// connection is closed afterward rather than left open.
    #[tokio::test]
    async fn seventeenth_connection_is_rejected_over_the_limit() {
        let name = unique_name("conn_limit");
        let server = RequestSocketServer::default();
        server
            .start_with_name(name.clone(), "0.5.0", store())
            .await
            .unwrap();

        // Held open for the test's duration: a connection that closed right
        // away would be pruned from the live-handle count before the 17th
        // connection is even attempted (see `accept_loop`'s `retain`), which
        // would defeat the point of this test.
        let mut clients = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            let stream =
                tokio::time::timeout(Duration::from_secs(2), Stream::connect(name.clone()))
                    .await
                    .expect("client connection timed out")
                    .expect("client failed to connect");
            let mut stream = BufReader::new(stream);
            let mut hello = String::new();
            stream.read_line(&mut hello).await.unwrap();
            assert!(hello.starts_with("{\"t\":\"hello\""));
            clients.push(stream);
        }

        let seventeenth = tokio::time::timeout(Duration::from_secs(2), Stream::connect(name))
            .await
            .expect("17th connection timed out")
            .expect("17th connection failed to connect");
        let mut seventeenth = BufReader::new(seventeenth);

        let mut hello = String::new();
        seventeenth.read_line(&mut hello).await.unwrap();
        assert!(
            hello.starts_with("{\"t\":\"hello\""),
            "the 17th connection must still get hello first: {hello}"
        );

        let mut limit_error = String::new();
        seventeenth.read_line(&mut limit_error).await.unwrap();
        assert!(
            limit_error.contains("\"code\":\"limit\""),
            "expected a limit error, got: {limit_error}"
        );

        let mut rest = String::new();
        let trailing = seventeenth.read_line(&mut rest).await.unwrap();
        assert_eq!(trailing, 0, "connection should close after the limit error");

        server.stop();
        drop(clients);
    }

    /// Before the Critical-1 fix, one `ConnectionContext` (and its `inflight`
    /// map) was shared by every connection on the server, so `http.abort` on
    /// one connection could cancel a same-id fetch registered by a completely
    /// different connection — a real hazard even between two well-behaved
    /// clients that both use the wire contract's own example id, `r1`. Each
    /// connection here gets its own `ConnectionContext`, exactly as
    /// `accept_loop` now builds them, so connection B's abort for "r1" must
    /// be a no-op against connection A's entry of the same name.
    #[tokio::test]
    async fn http_abort_never_reaches_across_connections_for_the_same_id() {
        let backing_store = store();

        let (client_a, server_a) = tokio::io::duplex(1 << 16);
        let (server_a_read, server_a_write) = tokio::io::split(server_a);
        let ctx_a = Arc::new(ConnectionContext::for_tests(backing_store.clone(), "0.5.0"));
        // A never-finishing task stands in for a real in-flight fetch, same
        // as http_proxy's own abort test: this test is about which
        // connection's map an abort reaches, not about a real network call.
        let never_finishes = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        ctx_a
            .inflight_lock()
            .insert("r1".to_string(), never_finishes.abort_handle());
        tokio::spawn(handle_connection(
            server_a_read,
            server_a_write,
            Arc::clone(&ctx_a),
        ));
        let (mut read_a, write_a) = tokio::io::split(client_a);
        let mut lines_a = BufReader::new(&mut read_a).lines();
        assert!(lines_a
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("{\"t\":\"hello\""));

        let (client_b, server_b) = tokio::io::duplex(1 << 16);
        let (server_b_read, server_b_write) = tokio::io::split(server_b);
        let ctx_b = Arc::new(ConnectionContext::for_tests(backing_store, "0.5.0"));
        tokio::spawn(handle_connection(server_b_read, server_b_write, ctx_b));
        let (mut read_b, mut write_b) = tokio::io::split(client_b);
        let mut lines_b = BufReader::new(&mut read_b).lines();
        assert!(lines_b
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("{\"t\":\"hello\""));

        write_b
            .write_all(
                b"{\"id\":\"b1\",\"method\":\"http.abort\",\"params\":{\"request\":\"r1\"}}\n",
            )
            .await
            .unwrap();
        let ack: serde_json::Value =
            serde_json::from_str(&lines_b.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(ack["id"], "b1");
        assert_eq!(ack["ok"], true);

        // B's inflight map never had "r1" — it is B's own, separate map — so
        // its abort must be a no-op: no `http.error` event follows the ack.
        assert!(
            tokio::time::timeout(Duration::from_millis(200), lines_b.next_line())
                .await
                .is_err(),
            "connection B must not receive an event for a request it never held"
        );

        // Connection A's entry must be untouched: still registered, and the
        // stand-in task still running.
        assert!(ctx_a.inflight_lock().contains_key("r1"));
        assert!(!never_finishes.is_finished());

        drop(write_a);
        drop(write_b);
    }

    /// Registration in `ctx.inflight` happens synchronously inside
    /// `spawn_http_fetch`, before the spawned task is ever polled (see its
    /// doc comment) — so the entry must already exist the instant
    /// `spawn_http_fetch` returns, on any runtime. This does not need the
    /// task to run at all to prove the ordering the Critical-2 fix relies on.
    #[tokio::test]
    async fn spawn_http_fetch_registers_before_the_task_is_polled() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, _rx) = mpsc::channel::<String>(64);
        let semaphore = Arc::new(Semaphore::new(MAX_INFLIGHT_PER_CONNECTION));
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let params = HttpFetchParams {
            slot: crate::shorthand::credentials::CredentialSlot::NotesLlm {
                provider: crate::shorthand::credentials::LlmProvider::Ollama,
                origin: "http://127.0.0.1:1".to_string(),
            },
            url: "http://127.0.0.1:1/".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        spawn_http_fetch("ordering".to_string(), params, Arc::clone(&ctx), tx, permit);

        assert!(ctx.inflight_lock().contains_key("ordering"));
    }

    /// The registration race Critical-2 describes only reproduces on a
    /// multi-thread runtime: on the default current-thread `#[tokio::test]`,
    /// a spawned task genuinely cannot run until the spawner yields, which
    /// hides it. A request that fails inside `plan_request` (no network I/O,
    /// microseconds of work) is the fast path most likely to win the race
    /// against `insert` if the ordering guarantee ever regressed.
    #[tokio::test(flavor = "multi_thread")]
    async fn fast_failing_fetch_does_not_leak_its_inflight_entry() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let semaphore = Arc::new(Semaphore::new(MAX_INFLIGHT_PER_CONNECTION));
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();
        let params = HttpFetchParams {
            slot: crate::shorthand::credentials::CredentialSlot::NotesLlm {
                provider: crate::shorthand::credentials::LlmProvider::Openai,
                origin: "https://api.openai.com".to_string(),
            },
            url: "https://evil.example/".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };

        spawn_http_fetch("r1".to_string(), params, Arc::clone(&ctx), tx, permit);

        let error = rx.recv().await.unwrap();
        assert!(
            error.contains("\"code\":\"origin_mismatch\""),
            "unexpected reply: {error}"
        );

        // The task's own drop (which runs `InflightGuard`) can land a moment
        // after the channel send above on a real multi-thread scheduler.
        for _ in 0..50 {
            if ctx.inflight_lock().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            ctx.inflight_lock().is_empty(),
            "a fast-failing fetch must not leak its inflight entry"
        );
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
