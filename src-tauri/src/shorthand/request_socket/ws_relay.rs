//! `ws.*`: relays the ACP network transport's WebSocket connection through
//! the credential store, the same way `http_proxy.rs` relays `http.fetch`.
//! `plan_request` (http_proxy.rs) supplies the origin-bound, secret-injected
//! headers for the handshake — `ws.open` is always a `GET`, and a
//! `notes-acp` slot's origin can itself be `ws`/`wss` (see that function's
//! scheme check). This module owns the connection once the handshake
//! completes: `run_ws_open` connects and spawns `run_reader`, which forwards
//! upstream frames as events for as long as the stream stays open;
//! `run_ws_send`/`run_ws_close` write to it from the client's side. Design:
//! PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md (Buzz workspace).

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use serde::Serialize;
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex as AsyncMutex, OwnedSemaphorePermit},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::{ClientRequestBuilder, IntoClientRequest},
        http::Uri,
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message,
    },
    MaybeTlsStream, WebSocketStream,
};

use super::http_proxy::{plan_error_message, plan_request, PlannedRequest, REQUEST_TIMEOUT};
use super::protocol::{error_line, ok_line, ErrorCode, WsOpenParams};
use super::server::ConnectionContext;

type UpstreamSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type UpstreamSink = SplitSink<UpstreamSocket, Message>;
type UpstreamSource = SplitStream<UpstreamSocket>;

/// One open `ws.open` stream, held in `ConnectionContext::streams`.
pub(crate) struct WsHandle {
    /// Cancels the reader task: called both by `run_ws_close` (immediately,
    /// once the client asks to close) and by `handle_connection`'s
    /// connection-drop cleanup (which drains every remaining entry).
    /// `run_ws_close` does not wait for the upstream's own Close reply
    /// before aborting — see that function's doc comment for why leaving the
    /// reader running until the peer answers is not safe to do here.
    pub(crate) abort: tokio::task::AbortHandle,
    /// Shared with the reader task (which needs it only to reply to a
    /// binary frame with a Close, per `run_reader`); `ws.send`/`ws.close`
    /// are otherwise the only writers. The `Mutex` just serialises those
    /// against each other and against that one reader-side write — nothing
    /// here sends concurrently on purpose.
    sink: Arc<AsyncMutex<UpstreamSink>>,
    /// RAII-only: releases this connection's `stream_capacity` permit when
    /// the entry is removed, whether by `ws.close`, by the reader noticing
    /// the stream ended, or by the connection-drop drain. Never read.
    _permit: OwnedSemaphorePermit,
}

/// Handles `ws.open`: resolves the slot's secret, plans the handshake
/// request via `plan_request` (reused from `http_proxy.rs`), connects
/// upstream, and — once connected — assigns the stream a server-generated id
/// and spawns `run_reader` to relay upstream frames as events for as long as
/// the stream stays open. Returns this request's own single reply line; the
/// reader task's events go out over `tx` independently afterwards.
pub(crate) async fn run_ws_open(
    id: &str,
    ctx: Arc<ConnectionContext>,
    tx: mpsc::Sender<String>,
    params: WsOpenParams,
) -> String {
    // A non-blocking `try_acquire`, not a queued `acquire`: a connection
    // already at the limit gets `limit` immediately, the same way the 17th
    // connection is turned away outright in `accept_loop` rather than made
    // to wait. Taking the permit here, before any credential lookup or
    // network I/O, also closes the race a check-then-insert against
    // `streams.len()` would leave open: two `ws.open` calls racing the 8th
    // slot cannot both win, because there is only ever one 8th permit to
    // acquire.
    let permit = match Arc::clone(&ctx.stream_capacity).try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return error_line(
                id,
                ErrorCode::Limit,
                "maximum number of open ws streams reached",
            )
        }
    };

    let store = Arc::clone(&ctx.store);
    let slot_for_lookup = params.slot.clone();
    let secret = match tokio::task::spawn_blocking(move || store.get(&slot_for_lookup)).await {
        Ok(Ok(secret)) => secret,
        Ok(Err(error)) => {
            return error_line(id, ErrorCode::CredentialUnavailable, &error.to_string())
        }
        Err(join_error) => {
            // Same reasoning as http_proxy::run_fetch's identical branch: a
            // `JoinError`'s `Display` can carry a panic payload, so it is
            // logged, not sent.
            log::error!(
                "credential lookup for ws.open did not complete on the blocking pool: {join_error}"
            );
            return error_line(
                id,
                ErrorCode::CredentialUnavailable,
                "the credential store did not answer",
            );
        }
    };

    let planned = match plan_request(
        &params.slot,
        secret.as_deref(),
        &params.url,
        "GET",
        &HashMap::new(),
    ) {
        Ok(planned) => planned,
        Err(code) => return error_line(id, code, plan_error_message(code)),
    };

    let request = match build_handshake_request(&planned, &params.protocols) {
        Ok(request) => request,
        Err(()) => {
            return error_line(
                id,
                ErrorCode::BadRequest,
                "the ws url or subprotocols could not be represented in a handshake request",
            )
        }
    };

    // `run_ws_open` holds one of the connection's 32 in-flight permits for
    // as long as this call runs (the permit was acquired by
    // `handle_connection`'s read loop before spawning the task this
    // function runs in) — an upstream that never completes the WebSocket
    // upgrade must not be able to hold that permit forever, or enough hung
    // handshakes eventually exhaust the whole budget and stall the read
    // loop for every other request on the connection, `ws.*` included. Reuse
    // `http_proxy`'s own per-request bound rather than inventing a second
    // number the wire contract does not mention.
    let (socket, _response) =
        match tokio::time::timeout(REQUEST_TIMEOUT, connect_async(request)).await {
            Ok(Ok(connected)) => connected,
            Ok(Err(error)) => {
                return error_line(
                    id,
                    ErrorCode::Upstream,
                    &format!("ws connect failed: {error}"),
                )
            }
            Err(_) => return error_line(id, ErrorCode::Timeout, "the ws handshake timed out"),
        };

    let stream_id = ctx.next_stream_id();
    let (sink, source) = socket.split();
    let sink = Arc::new(AsyncMutex::new(sink));

    let reader_ctx = Arc::clone(&ctx);
    let reader_sink = Arc::clone(&sink);
    let reader_stream_id = stream_id.clone();
    let reader_tx = tx.clone();
    let reader_handle = tokio::spawn(async move {
        run_reader(reader_ctx, reader_stream_id, source, reader_sink, reader_tx).await;
    });

    ctx.streams_lock().insert(
        stream_id.clone(),
        WsHandle {
            abort: reader_handle.abort_handle(),
            sink,
            _permit: permit,
        },
    );

    ok_line(id, serde_json::json!({ "stream": stream_id }))
}

/// Builds the client handshake request `connect_async` sends: the planned
/// URL's mandatory WebSocket upgrade headers (`Uri::into_client_request`,
/// which `ClientRequestBuilder` delegates to, generates `Host`,
/// `Connection`, `Upgrade`, `Sec-WebSocket-Version` and a fresh
/// `Sec-WebSocket-Key`), the client's requested subprotocols, and then
/// `plan_request`'s own headers layered on top — the injected
/// `authorization` (or `x-api-key`) included. No conversion is needed for
/// that last step: `PlannedRequest.headers` is `reqwest::header::HeaderMap`,
/// which is the exact same `http` crate type as `tungstenite`'s own request
/// headers (reqwest and tungstenite both depend on `http 1.x`, and Cargo
/// unifies the two to one copy of it in this binary).
fn build_handshake_request(
    planned: &PlannedRequest,
    protocols: &[String],
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, ()> {
    let uri: Uri = planned.url.as_str().parse().map_err(|_| ())?;
    let mut builder = ClientRequestBuilder::new(uri);
    for protocol in protocols {
        builder = builder.with_sub_protocol(protocol.clone());
    }
    let mut request = builder.into_client_request().map_err(|_| ())?;
    for (name, value) in &planned.headers {
        request.headers_mut().insert(name.clone(), value.clone());
    }
    Ok(request)
}

/// Handles `ws.send`: writes one text frame to the named stream. `data` is
/// sent as-is as the frame's text content — the wire contract does not
/// describe it as JSON-encoded on top of the frame, so whatever the caller
/// already serialised (an ACP JSON-RPC message, typically) travels
/// unmodified.
pub(crate) async fn run_ws_send(
    id: &str,
    ctx: &ConnectionContext,
    stream: String,
    data: String,
) -> String {
    let sink = {
        let streams = ctx.streams_lock();
        match streams.get(&stream) {
            Some(handle) => Arc::clone(&handle.sink),
            None => {
                return error_line(
                    id,
                    ErrorCode::BadRequest,
                    "unknown or already-closed ws stream",
                )
            }
        }
    };
    // Bound to a variable rather than chained inline: a temporary
    // `MutexGuard` produced at the tail of a block outlives the `match`
    // expression that borrows from it, which the borrow checker rejects.
    // Binding it lets the guard drop (at the end of this statement) before
    // the result is matched on.
    let result = sink.lock().await.send(Message::text(data)).await;
    match result {
        Ok(()) => ok_line(id, serde_json::json!({})),
        Err(error) => error_line(id, ErrorCode::Upstream, &format!("ws send failed: {error}")),
    }
}

/// Handles `ws.close`: sends a Close frame upstream carrying the client's
/// code and reason, removes the stream's entry, and reports `ws.closed`
/// immediately with that same code and reason — the client already knows
/// what it asked to close with, so this does not wait to see whether (or
/// how) the upstream answers. That wait is exactly what this avoids: the
/// reader task is aborted here rather than left running to observe the
/// upstream's own Close reply, because nothing bounds how long a peer can
/// take to answer one (or whether it ever does), and this call's own
/// `stream_capacity` permit is released the moment `streams.remove` runs
/// below. A client that opens and closes streams in a loop against a peer
/// that never echoes Close would otherwise accumulate one forever-blocked
/// reader task per cycle, unbounded by the 8-stream limit that is supposed
/// to cap exactly this kind of resource use.
pub(crate) async fn run_ws_close(
    id: &str,
    ctx: &ConnectionContext,
    tx: &mpsc::Sender<String>,
    stream: String,
    code: u16,
    reason: String,
) -> String {
    let Some(handle) = ctx.streams_lock().remove(&stream) else {
        return error_line(
            id,
            ErrorCode::BadRequest,
            "unknown or already-closed ws stream",
        );
    };
    let close = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.clone().into(),
    };
    let _ = handle
        .sink
        .lock()
        .await
        .send(Message::Close(Some(close)))
        .await;
    handle.abort.abort();
    let _ = tx.send(closed_event(&stream, code, &reason)).await;
    ok_line(id, serde_json::json!({}))
}

/// Relays one upstream WebSocket connection's incoming frames as events on
/// `out` until the stream ends, one way or another: an upstream Close frame
/// or plain EOF becomes `ws.closed`; a binary frame (unsupported per the
/// wire contract) or a transport error becomes `ws.error` — and, for the
/// binary case, this proxy then closes its own side and reports `ws.closed`
/// too, so the client does not have to infer a stream's fate from an error
/// event alone. Removes the stream's entry from `ctx.streams` itself in
/// every case; if `run_ws_close` already removed it first, these `remove`
/// calls are harmless no-ops.
async fn run_reader(
    ctx: Arc<ConnectionContext>,
    stream_id: String,
    mut source: UpstreamSource,
    sink: Arc<AsyncMutex<UpstreamSink>>,
    out: mpsc::Sender<String>,
) {
    loop {
        let message = match source.next().await {
            Some(Ok(message)) => message,
            Some(Err(error)) => {
                ctx.streams_lock().remove(&stream_id);
                let _ = out
                    .send(error_event(&stream_id, &format!("ws error: {error}")))
                    .await;
                return;
            }
            None => {
                // The upstream ended the TCP connection without ever
                // sending a Close frame. RFC 6455 §7.1.5 calls this "No
                // Status Rcvd" (code 1005) — the same convention browsers
                // use for `onclose` here — since the wire contract has no
                // separate event for a close with nothing to report.
                ctx.streams_lock().remove(&stream_id);
                let _ = out
                    .send(closed_event(&stream_id, CloseCode::Status.into(), ""))
                    .await;
                return;
            }
        };

        match message {
            Message::Text(text) => {
                if out
                    .send(message_event(&stream_id, text.as_str()))
                    .await
                    .is_err()
                {
                    return; // The connection is gone; nothing left to relay to.
                }
            }
            Message::Binary(_) => {
                let _ = out
                    .send(error_event(&stream_id, "binary frames are not supported"))
                    .await;
                let close = CloseFrame {
                    code: CloseCode::Unsupported,
                    reason: "binary frames are not supported".into(),
                };
                let _ = sink
                    .lock()
                    .await
                    .send(Message::Close(Some(close.clone())))
                    .await;
                ctx.streams_lock().remove(&stream_id);
                let _ = out
                    .send(closed_event(
                        &stream_id,
                        close.code.into(),
                        close.reason.as_str(),
                    ))
                    .await;
                return;
            }
            Message::Close(frame) => {
                ctx.streams_lock().remove(&stream_id);
                let (code, reason) = match frame {
                    Some(frame) => (u16::from(frame.code), frame.reason.to_string()),
                    None => (CloseCode::Status.into(), String::new()),
                };
                let _ = out.send(closed_event(&stream_id, code, &reason)).await;
                return;
            }
            // A Ping's automatic Pong reply is queued by tungstenite's own
            // protocol state machine (see `WebSocket::read_message_frame`'s
            // handling of `OpCtl::Ping`), not by this loop; `Frame` is never
            // produced by `.next()` in the first place (see `Message::Frame`
            // 's own doc comment). Neither is an event this relay forwards.
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        }
    }
}

#[derive(Serialize)]
struct MessageEvent<'a> {
    t: &'static str,
    stream: &'a str,
    data: &'a str,
}

#[derive(Serialize)]
struct ClosedEvent<'a> {
    t: &'static str,
    stream: &'a str,
    code: u16,
    reason: &'a str,
}

#[derive(Serialize)]
struct WsErrorEvent<'a> {
    t: &'static str,
    stream: &'a str,
    message: &'a str,
}

fn message_event(stream: &str, data: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&MessageEvent {
            t: "ws.message",
            stream,
            data
        })
        .expect("message event always serializes")
    )
}

fn closed_event(stream: &str, code: u16, reason: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&ClosedEvent {
            t: "ws.closed",
            stream,
            code,
            reason
        })
        .expect("closed event always serializes")
    )
}

fn error_event(stream: &str, message: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&WsErrorEvent {
            t: "ws.error",
            stream,
            message
        })
        .expect("error event always serializes")
    )
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration;

    use tokio_tungstenite::tungstenite::handshake::server::{
        Request as HandshakeRequest, Response as HandshakeResponse,
    };

    use crate::shorthand::credentials::{CredentialSlot, CredentialStore, MemoryBackend};

    use super::super::server::MAX_WS_STREAMS_PER_CONNECTION;
    use super::*;

    fn store() -> Arc<CredentialStore> {
        Arc::new(CredentialStore::with_backend(Box::new(
            MemoryBackend::default(),
        )))
    }

    fn acp_slot(vault_id: &str, origin: &str) -> CredentialSlot {
        CredentialSlot::NotesAcp {
            vault_id: vault_id.to_string(),
            origin: origin.to_string(),
        }
    }

    /// A minimal in-process WebSocket echo server: text and Close frames are
    /// echoed back exactly as received (which, for Close, hands the
    /// production reader a real `CloseFrame` carrying whatever code/reason
    /// the client sent, exactly as a cooperating ACP endpoint would). Each
    /// accepted connection's `authorization` upgrade header (or its absence)
    /// is appended to the returned vector, in acceptance order.
    async fn spawn_echo_server(
        max_connections: usize,
    ) -> (SocketAddr, Arc<StdMutex<Vec<Option<String>>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen_auth = Arc::new(StdMutex::new(Vec::new()));
        let seen_auth_task = Arc::clone(&seen_auth);
        tokio::spawn(async move {
            for _ in 0..max_connections {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let captured = Arc::clone(&seen_auth_task);
                tokio::spawn(async move {
                    // The `Callback` trait's `Err` type is tungstenite's own
                    // handshake-rejection response, not something this test
                    // helper controls; it is simply never returned here
                    // (every handshake is accepted), so clippy's size
                    // warning about it does not point at anything to fix.
                    #[allow(clippy::result_large_err)]
                    let callback =
                        move |request: &HandshakeRequest, response: HandshakeResponse| {
                            let auth = request
                                .headers()
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .map(str::to_string);
                            captured.lock().unwrap().push(auth);
                            Ok(response)
                        };
                    let Ok(mut socket) =
                        tokio_tungstenite::accept_hdr_async(stream, callback).await
                    else {
                        return;
                    };
                    while let Some(Ok(message)) = socket.next().await {
                        match message {
                            Message::Close(frame) => {
                                let _ = socket.send(Message::Close(frame)).await;
                                break;
                            }
                            other => {
                                let _ = socket.send(other).await;
                            }
                        }
                    }
                });
            }
        });
        (addr, seen_auth)
    }

    /// A server that sends one binary frame immediately after the handshake,
    /// then waits for the relay's own Close reply (sent because binary
    /// frames are unsupported) and echoes it, the way a cooperating endpoint
    /// would acknowledge a close it did not initiate.
    async fn spawn_binary_frame_server() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let _ = socket.send(Message::Binary(vec![1, 2, 3].into())).await;
            while let Some(Ok(message)) = socket.next().await {
                if let Message::Close(frame) = message {
                    let _ = socket.send(Message::Close(frame)).await;
                    break;
                }
            }
        });
        addr
    }

    /// Accepts one connection and completes the handshake, then never sends
    /// anything back — including never echoing a Close frame it receives.
    /// Stands in for an unresponsive or hostile ACP endpoint.
    async fn spawn_silent_server() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            while socket.next().await.is_some() {}
        });
        addr
    }

    async fn wait_for<F: Fn() -> bool>(condition: F) {
        for _ in 0..100 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn open_send_and_close_round_trip_with_injected_auth() {
        let (addr, seen_auth) = spawn_echo_server(1).await;
        let origin = format!("ws://{addr}");
        let slot = acp_slot("v", &origin);
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);

        let params = WsOpenParams {
            slot: slot.clone(),
            url: format!("{origin}/"),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("r1", Arc::clone(&ctx), tx.clone(), params).await;
        assert_eq!(
            reply,
            "{\"id\":\"r1\",\"ok\":true,\"result\":{\"stream\":\"s1\"}}\n"
        );

        wait_for(|| !seen_auth.lock().unwrap().is_empty()).await;
        assert_eq!(
            seen_auth.lock().unwrap().as_slice(),
            [Some("Bearer tok".to_string())],
            "the app must inject the slot's own secret into the handshake, never a client-supplied one"
        );

        let send_reply = run_ws_send("r2", &ctx, "s1".to_string(), "ping".to_string()).await;
        assert_eq!(send_reply, "{\"id\":\"r2\",\"ok\":true,\"result\":{}}\n");

        let message = rx.recv().await.unwrap();
        assert_eq!(
            message,
            "{\"t\":\"ws.message\",\"stream\":\"s1\",\"data\":\"ping\"}\n"
        );

        let close_reply =
            run_ws_close("r3", &ctx, &tx, "s1".to_string(), 1000, String::new()).await;
        assert_eq!(close_reply, "{\"id\":\"r3\",\"ok\":true,\"result\":{}}\n");

        let closed = rx.recv().await.unwrap();
        assert_eq!(
            closed,
            "{\"t\":\"ws.closed\",\"stream\":\"s1\",\"code\":1000,\"reason\":\"\"}\n"
        );
    }

    #[tokio::test]
    async fn origin_mismatch_is_refused_before_any_connection_attempt() {
        let (addr, seen_auth) = spawn_echo_server(1).await;
        let slot = acp_slot("v", &format!("ws://{addr}"));
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, _rx) = mpsc::channel::<String>(64);

        let params = WsOpenParams {
            slot,
            url: "ws://evil.example/".to_string(),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("r1", ctx, tx, params).await;
        assert_eq!(
            reply,
            "{\"id\":\"r1\",\"ok\":false,\"error\":{\"code\":\"origin_mismatch\",\"message\":\"the request URL's origin does not match the credential slot's origin\"}}\n"
        );

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            seen_auth.lock().unwrap().is_empty(),
            "no connection should have been attempted"
        );
    }

    #[tokio::test]
    async fn ninth_stream_on_one_connection_is_refused_with_limit() {
        let (addr, _seen_auth) = spawn_echo_server(MAX_WS_STREAMS_PER_CONNECTION).await;
        let origin = format!("ws://{addr}");
        let slot = acp_slot("v", &origin);
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, rx) = mpsc::channel::<String>(64);

        for i in 0..MAX_WS_STREAMS_PER_CONNECTION {
            let params = WsOpenParams {
                slot: slot.clone(),
                url: format!("{origin}/"),
                protocols: Vec::new(),
            };
            let reply =
                run_ws_open(&format!("open{i}"), Arc::clone(&ctx), tx.clone(), params).await;
            assert!(
                reply.contains("\"ok\":true"),
                "stream {i} should have opened: {reply}"
            );
        }

        let params = WsOpenParams {
            slot,
            url: format!("{origin}/"),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("open9", Arc::clone(&ctx), tx.clone(), params).await;
        assert_eq!(
            reply,
            "{\"id\":\"open9\",\"ok\":false,\"error\":{\"code\":\"limit\",\"message\":\"maximum number of open ws streams reached\"}}\n"
        );
        drop(rx);
    }

    #[tokio::test]
    async fn binary_frame_from_upstream_is_reported_then_closes() {
        let addr = spawn_binary_frame_server().await;
        let origin = format!("ws://{addr}");
        let slot = acp_slot("v", &origin);
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);

        let params = WsOpenParams {
            slot,
            url: format!("{origin}/"),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("r1", Arc::clone(&ctx), tx, params).await;
        assert!(
            reply.contains("\"stream\":\"s1\""),
            "unexpected reply: {reply}"
        );

        let error = rx.recv().await.unwrap();
        assert_eq!(
            error,
            "{\"t\":\"ws.error\",\"stream\":\"s1\",\"message\":\"binary frames are not supported\"}\n"
        );

        let closed = rx.recv().await.unwrap();
        assert_eq!(
            closed,
            "{\"t\":\"ws.closed\",\"stream\":\"s1\",\"code\":1003,\"reason\":\"binary frames are not supported\"}\n"
        );
    }

    /// Pins the fix described in `run_ws_close`'s own doc comment: a peer
    /// that never echoes a Close reply must not be able to leave a reader
    /// task running forever just because the client already asked to close
    /// — repeating this cycle against such a peer would otherwise
    /// accumulate one permanently-blocked task per `ws.close`, unbounded by
    /// the 8-stream limit (which the immediate permit release makes look
    /// like it isn't even trying to cap this).
    #[tokio::test]
    async fn close_aborts_the_reader_task_even_when_the_peer_never_echoes_it() {
        let addr = spawn_silent_server().await;
        let origin = format!("ws://{addr}");
        let slot = acp_slot("v", &origin);
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);

        let params = WsOpenParams {
            slot,
            url: format!("{origin}/"),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("r1", Arc::clone(&ctx), tx.clone(), params).await;
        assert!(
            reply.contains("\"stream\":\"s1\""),
            "unexpected reply: {reply}"
        );

        let abort = ctx.streams_lock().get("s1").unwrap().abort.clone();
        assert!(!abort.is_finished());

        let close_reply =
            run_ws_close("r2", &ctx, &tx, "s1".to_string(), 1000, "bye".to_string()).await;
        assert_eq!(close_reply, "{\"id\":\"r2\",\"ok\":true,\"result\":{}}\n");

        let closed = rx.recv().await.unwrap();
        assert_eq!(
            closed,
            "{\"t\":\"ws.closed\",\"stream\":\"s1\",\"code\":1000,\"reason\":\"bye\"}\n"
        );

        wait_for(|| abort.is_finished()).await;
        assert!(
            abort.is_finished(),
            "the reader task must not be left waiting on a peer that never answers"
        );
    }

    /// Mirrors what `handle_connection`'s cleanup does when the whole
    /// connection drops: drain `streams` and abort every remaining reader
    /// task. This pins that doing so actually stops the task (rather than,
    /// say, only removing the bookkeeping entry and leaving the task
    /// running against a socket nobody reads from anymore).
    #[tokio::test]
    async fn connection_drop_aborts_every_open_stream() {
        let (addr, _seen_auth) = spawn_echo_server(1).await;
        let origin = format!("ws://{addr}");
        let slot = acp_slot("v", &origin);
        let credential_store = store();
        credential_store.set(&slot, "tok").unwrap();
        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, _rx) = mpsc::channel::<String>(64);

        let params = WsOpenParams {
            slot,
            url: format!("{origin}/"),
            protocols: Vec::new(),
        };
        let reply = run_ws_open("r1", Arc::clone(&ctx), tx, params).await;
        assert!(
            reply.contains("\"stream\":\"s1\""),
            "unexpected reply: {reply}"
        );

        let abort = ctx.streams_lock().get("s1").unwrap().abort.clone();
        assert!(!abort.is_finished());

        for (_, handle) in ctx.streams_lock().drain() {
            handle.abort.abort();
        }

        wait_for(|| abort.is_finished()).await;
        assert!(
            abort.is_finished(),
            "the reader task must be aborted when the connection drops"
        );
    }
}
