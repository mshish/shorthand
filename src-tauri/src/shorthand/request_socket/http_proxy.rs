//! `http.fetch` / `http.abort`: proxies one provider HTTP call through the
//! credential store. `plan_request` is the pure decision — parse the URL,
//! bind it to the slot's origin, strip any client-supplied auth header, and
//! inject the slot's secret — so every branch is covered without a socket.
//! `run_fetch` is the only caller that then does network I/O, and streams the
//! response back as base64 `http.body` events under the 64 MiB cap from the
//! wire contract. Design: PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md
//! (Buzz workspace).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::llm_client::sanitized_url;
use crate::shorthand::credentials::{CredentialSlot, LlmProvider};

use super::protocol::{error_line, ok_line, ErrorCode, HttpFetchParams};
use super::server::ConnectionContext;

/// Request body cap from the wire contract, checked after base64 decoding
/// (the cap is on the real bytes a provider will see, not the wire encoding).
const MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Response body cap from the wire contract. Enforced as a running total over
/// the bytes actually received, never from a `Content-Length` header — an
/// upstream can send any value there regardless of what it actually streams.
const MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Per-request timeout from the wire contract, covering both the initial
/// send and every subsequent chunk: a upstream that answers headers and then
/// stalls mid-body must not hold this request open forever. `pub(crate)`:
/// `ws_relay.rs` (A6) reuses it to bound `ws.open`'s handshake the same way,
/// since that call holds one of the connection's 32 in-flight permits (see
/// `handle_connection`'s read loop) for as long as it runs, and an upstream
/// that never completes the WebSocket upgrade must not be able to hold that
/// permit — and eventually, one per hung `ws.open`, the whole 32-permit
/// budget — forever.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Headers whose presence would let a client see or influence authentication
/// that is this proxy's decision, not the client's — the four the wire
/// contract names. Stripped from the client's request before this proxy
/// injects the slot's own auth (the wire contract's rule), and reused by
/// `response_headers_json` to strip the same names from the upstream's
/// response before it is echoed back, in case a response tried to reflect
/// one (review finding Minor-11).
const STRIPPED_AUTH_HEADERS: [&str; 4] = [
    "authorization",
    "x-api-key",
    "cookie",
    "proxy-authorization",
];

/// Framing and routing headers a client must not be allowed to set on the
/// outgoing request. Deciding these is the proxy's job, not the client's:
///
/// - `host` controls which virtual host a shared front end, CDN or reverse
///   proxy *routes* the request to. Hyper's client only sets `Host` when the
///   header is absent, so a client-supplied one is sent verbatim even though
///   the TCP/TLS peer is still the slot's origin — on such a front end, the
///   injected secret would be delivered to a host the slot never authorised.
///   That is exactly the origin binding this proxy exists to enforce.
/// - `content-length` / `transfer-encoding` disagreeing with the body this
///   proxy actually sends is the classic request-smuggling primitive against
///   the upstream; deciding framing here means never relying on hyper's
///   conflict handling for it.
/// - `connection`, `upgrade`, `te`, `trailer`, `expect` and `keep-alive` are
///   hop-by-hop, meaningful only between this proxy and the upstream it picks
///   — a client has no business setting any of them.
///
/// This is a superset of the wire contract's own four-header list; see the
/// "Auth injection rules" paragraph of the wire contract for why (review
/// finding Important-3).
const STRIPPED_FRAMING_HEADERS: [&str; 9] = [
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "te",
    "trailer",
    "expect",
    "keep-alive",
];

/// The outcome of planning one `http.fetch` call: a URL, method and header
/// set that are safe to send exactly as they are.
pub struct PlannedRequest {
    pub url: url::Url,
    pub method: reqwest::Method,
    pub headers: HeaderMap,
}

/// Plans one `http.fetch` call. Pure and synchronous: validates the URL,
/// checks it against the slot's origin, strips any client-supplied
/// `authorization`/`x-api-key`/`cookie`/`proxy-authorization`, and injects the
/// slot's secret per the wire contract's auth-injection rule. `run_fetch` is
/// the only caller that turns the result into network I/O, so every
/// rejection here happens before any request leaves the process.
pub fn plan_request(
    slot: &CredentialSlot,
    secret: Option<&str>,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
) -> Result<PlannedRequest, ErrorCode> {
    let parsed = url::Url::parse(url).map_err(|_| ErrorCode::BadRequest)?;
    // Plain `http` is accepted for any host, not only loopback. The owner
    // decided this on 2026-09-12: whether a secret may travel in cleartext is
    // the user's call, because some corporate users route provider calls
    // through a local proxy that is unencrypted but does not sit on
    // localhost. The origin the slot was configured with is still the only
    // place the secret can go; the scheme check only rules out non-HTTP,
    // non-WebSocket URLs. `ws`/`wss` are accepted too: `ws_relay.rs` (A6)
    // reuses this same function for `ws.open`, whose slot (`notes-acp`) can
    // legitimately have a `ws://`/`wss://` origin (see
    // `CredentialSlot::origin` and `canonical_origin`), and the origin
    // comparison below already handles those schemes correctly because
    // `url::Url` treats `ws`/`wss` as special schemes with the same
    // tuple-origin algorithm as `http`/`https`.
    if !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
        return Err(ErrorCode::BadRequest);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ErrorCode::BadRequest);
    }

    // `PostProcess` never reaches here: protocol.rs rejects that slot before
    // an `HttpFetchParams` is even constructed, so every slot this function
    // sees carries an origin to bind the request to.
    let slot_origin = slot.origin().ok_or(ErrorCode::BadRequest)?;
    if parsed.origin().ascii_serialization() != slot_origin {
        return Err(ErrorCode::OriginMismatch);
    }

    let method =
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| ErrorCode::BadRequest)?;

    let mut header_map = HeaderMap::new();
    for (name, value) in headers {
        if STRIPPED_AUTH_HEADERS
            .iter()
            .chain(STRIPPED_FRAMING_HEADERS.iter())
            .any(|stripped| name.eq_ignore_ascii_case(stripped))
        {
            continue;
        }
        let (Ok(header_name), Ok(header_value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) else {
            // A header this proxy cannot represent is dropped rather than
            // failing the whole request over one malformed client header.
            continue;
        };
        header_map.insert(header_name, header_value);
    }

    match secret {
        Some(secret) => {
            let value = HeaderValue::from_str(&auth_value(slot, secret))
                .map_err(|_| ErrorCode::BadRequest)?;
            header_map.insert(auth_header_name(slot), value);
        }
        None if slot_allows_no_secret(slot) => {}
        None => return Err(ErrorCode::CredentialMissing),
    }

    Ok(PlannedRequest {
        url: parsed,
        method,
        headers: header_map,
    })
}

fn slot_allows_no_secret(slot: &CredentialSlot) -> bool {
    matches!(slot, CredentialSlot::NotesLlm { provider, .. } if provider.allows_no_secret())
}

/// The wire contract's auth-injection rule: `notes-llm` + `anthropic` gets
/// `x-api-key`, every other slot gets `authorization: Bearer …`.
fn auth_header_name(slot: &CredentialSlot) -> HeaderName {
    match slot {
        CredentialSlot::NotesLlm {
            provider: LlmProvider::Anthropic,
            ..
        } => HeaderName::from_static("x-api-key"),
        _ => AUTHORIZATION,
    }
}

fn auth_value(slot: &CredentialSlot, secret: &str) -> String {
    match slot {
        CredentialSlot::NotesLlm {
            provider: LlmProvider::Anthropic,
            ..
        } => secret.to_string(),
        _ => format!("Bearer {secret}"),
    }
}

/// Removes this request's abort handle from `ctx.inflight` whichever way
/// `run_fetch` ends — normal completion, an early error return, or the task
/// being cancelled by `handle_abort`'s own `AbortHandle::abort()` (tokio drops
/// a cancelled task's locals normally, so this still runs). Without this, a
/// finished or aborted request's id would linger in the map, and a later,
/// unrelated `http.abort` racing a reused id could abort the wrong request.
struct InflightGuard<'a> {
    ctx: &'a ConnectionContext,
    id: &'a str,
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.ctx.inflight_lock().remove(self.id);
    }
}

/// Runs one `http.fetch`: decodes the body, resolves the slot's secret, plans
/// the request, sends it, and streams the response back over `out` as an
/// `ok` line followed by `http.body`/`http.end` (or an `http.error` event if
/// something fails after the `ok` line already went out — by then `id` has
/// been used for the one reply it gets).
pub async fn run_fetch(
    ctx: Arc<ConnectionContext>,
    id: String,
    params: HttpFetchParams,
    out: mpsc::Sender<String>,
) {
    let _guard = InflightGuard { ctx: &ctx, id: &id };

    let body = match decode_body(params.body.as_deref()) {
        Ok(body) => body,
        Err((code, message)) => {
            let _ = out.send(error_line(&id, code, message)).await;
            return;
        }
    };

    // The keyring API blocks (a locked keychain or a slow Secret Service
    // round trip can stall for a while), so it runs on the blocking pool
    // rather than a runtime worker — every other credential path already does
    // this for the same reason (see server.rs's `credential_set` and
    // friends). Without it, `handle.abort()` could not interrupt this task
    // while it sat inside a synchronous call (review finding Important-4).
    let store = Arc::clone(&ctx.store);
    let slot_for_lookup = params.slot.clone();
    let secret = match tokio::task::spawn_blocking(move || store.get(&slot_for_lookup)).await {
        Ok(Ok(secret)) => secret,
        Ok(Err(error)) => {
            let _ = out
                .send(error_line(
                    &id,
                    ErrorCode::CredentialUnavailable,
                    &error.to_string(),
                ))
                .await;
            return;
        }
        Err(join_error) => {
            // A `JoinError` means the blocking task panicked or was
            // cancelled, not anything about the request itself — its
            // `Display` can include a panic payload, which may not be safe to
            // hand to a client. Logged, not sent.
            log::error!("credential lookup for http.fetch did not complete on the blocking pool: {join_error}");
            let _ = out
                .send(error_line(
                    &id,
                    ErrorCode::CredentialUnavailable,
                    "the credential store did not answer",
                ))
                .await;
            return;
        }
    };

    let planned = match plan_request(
        &params.slot,
        secret.as_deref(),
        &params.url,
        &params.method,
        &params.headers,
    ) {
        Ok(planned) => planned,
        Err(code) => {
            let _ = out
                .send(error_line(&id, code, plan_error_message(code)))
                .await;
            return;
        }
    };

    let mut request = ctx
        .http
        .request(planned.method, planned.url)
        .headers(planned.headers);
    if !body.is_empty() {
        request = request.body(body);
    }

    // One deadline for the whole request, not one restarted on every chunk:
    // the wire contract's "per-request timeout 15 min" means from the send
    // to the last byte of the body, so an upstream trickling one byte every
    // 14 minutes must not hold the request (and its semaphore permit, and
    // its connection) open indefinitely under the 64 MiB cap (review finding
    // Important-5).
    let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;

    let response = match tokio::time::timeout_at(deadline, request.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            let _ = out
                .send(error_line(
                    &id,
                    ErrorCode::Upstream,
                    &sanitized_error(&error),
                ))
                .await;
            return;
        }
        Err(_) => {
            let _ = out
                .send(error_line(
                    &id,
                    ErrorCode::Timeout,
                    "the upstream request timed out",
                ))
                .await;
            return;
        }
    };

    let status = u32::from(response.status().as_u16());
    let headers_json = response_headers_json(response.headers());
    if out
        .send(ok_line(
            &id,
            serde_json::json!({ "status": status, "headers": headers_json }),
        ))
        .await
        .is_err()
    {
        return; // The connection is already gone; nothing left to stream to.
    }

    let mut stream = response.bytes_stream();
    let mut received = 0usize;
    loop {
        let next = match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(next) => next,
            Err(_) => {
                let _ = out
                    .send(error_event(&id, "the upstream response timed out"))
                    .await;
                return;
            }
        };
        let chunk = match next {
            Some(Ok(chunk)) => chunk,
            Some(Err(error)) => {
                let _ = out.send(error_event(&id, &sanitized_error(&error))).await;
                return;
            }
            None => break,
        };

        received += chunk.len();
        if received > MAX_RESPONSE_BODY_BYTES {
            let _ = out
                .send(error_event(&id, "response body exceeds the 64 MiB limit"))
                .await;
            return;
        }

        if out
            .send(body_event(&id, &STANDARD.encode(&chunk)))
            .await
            .is_err()
        {
            return;
        }
    }

    // `handle_abort` removes this id from `ctx.inflight` before calling
    // `AbortHandle::abort()`, but abort only takes effect at this task's next
    // await point that returns `Pending` — and `out.send` on a non-full
    // channel can complete without ever yielding. So a task already past the
    // stream loop can still queue `http.end` after `handle_abort` already
    // queued the "aborted" error event for the same id. Checking membership
    // here (the only other place that removes this id is this same
    // function's own `InflightGuard`, which has not dropped yet) tells the
    // two cases apart: absent means an abort raced ahead and already spoke
    // for this id, so this task must not add its own conflicting word
    // (review finding Minor-8).
    if ctx.inflight_lock().contains_key(&id) {
        let _ = out.send(end_event(&id)).await;
    }
}

/// Handles `http.abort`. Aborting a request that already finished is not an
/// error — the wire contract has no way to tell "too late" apart from "gone"
/// — so this always acknowledges the abort itself, and only emits the
/// `http.error` "aborted" event when a matching in-flight request was found.
pub async fn handle_abort(
    id: &str,
    target: &str,
    ctx: &ConnectionContext,
    out: &mpsc::Sender<String>,
) {
    let handle = ctx.inflight_lock().remove(target);
    if let Some(handle) = handle {
        handle.abort();
        let _ = out.send(error_event(target, "aborted")).await;
    }
    let _ = out.send(ok_line(id, serde_json::json!({}))).await;
}

fn decode_body(encoded: Option<&str>) -> Result<Vec<u8>, (ErrorCode, &'static str)> {
    let Some(encoded) = encoded else {
        return Ok(Vec::new());
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| (ErrorCode::BadRequest, "request body is not valid base64"))?;
    if bytes.len() > MAX_REQUEST_BODY_BYTES {
        return Err((ErrorCode::TooLarge, "request body exceeds the 16 MiB limit"));
    }
    Ok(bytes)
}

/// `pub(crate)`, not private: `ws_relay.rs` (A6) reuses `plan_request` for
/// `ws.open` and needs the same error-code-to-message mapping.
pub(crate) fn plan_error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::OriginMismatch => {
            "the request URL's origin does not match the credential slot's origin"
        }
        ErrorCode::CredentialMissing => "no credential is configured for this slot",
        _ => "the request could not be validated",
    }
}

/// The response headers as a lower-case string map, minus `set-cookie` and
/// the same four names `STRIPPED_AUTH_HEADERS` strips from the request. The
/// wire contract carries no cookie jar, and echoing `set-cookie` back would
/// let a provider set state in a client that never asked for it; reflecting
/// `authorization`/`x-api-key`/`cookie`/`proxy-authorization` needs a
/// cooperating endpoint on the slot's own origin (and a response body could
/// do the same), so this is defence in depth rather than a live leak — but a
/// client that by design must never see this proxy's injected secret should
/// not be handed it back on a silver platter either (review finding
/// Minor-11).
fn response_headers_json(headers: &HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        let name_str = name.as_str();
        if name_str.eq_ignore_ascii_case("set-cookie")
            || STRIPPED_AUTH_HEADERS
                .iter()
                .any(|stripped| name_str.eq_ignore_ascii_case(stripped))
        {
            continue;
        }
        if let Ok(text) = value.to_str() {
            map.insert(
                name.as_str().to_ascii_lowercase(),
                serde_json::Value::String(text.to_string()),
            );
        }
    }
    serde_json::Value::Object(map)
}

/// Turns a reqwest error into a message safe to hand back to the client. A
/// reqwest error's own `Display` can embed the request URL verbatim,
/// including a leaked query-string token, so the URL is stripped and
/// re-added through `sanitized_url` instead — mirroring
/// `llm_client::report_reqwest_error`, which sanitises the same way for the
/// post-processing HTTP client.
fn sanitized_error(error: &reqwest::Error) -> String {
    let kind = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_decode() {
        "decode"
    } else if error.is_body() {
        "body"
    } else {
        "request"
    };
    match error.url().map(sanitized_url) {
        Some(url) => format!("upstream {kind} error (url: {url})"),
        None => format!("upstream {kind} error"),
    }
}

#[derive(Serialize)]
struct BodyEvent<'a> {
    t: &'static str,
    request: &'a str,
    data: &'a str,
}

#[derive(Serialize)]
struct EndEvent<'a> {
    t: &'static str,
    request: &'a str,
}

#[derive(Serialize)]
struct ErrorEvent<'a> {
    t: &'static str,
    request: &'a str,
    message: &'a str,
}

fn body_event(request: &str, data: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&BodyEvent {
            t: "http.body",
            request,
            data
        })
        .expect("body event always serializes")
    )
}

fn end_event(request: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&EndEvent {
            t: "http.end",
            request
        })
        .expect("end event always serializes")
    )
}

fn error_event(request: &str, message: &str) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&ErrorEvent {
            t: "http.error",
            request,
            message
        })
        .expect("error event always serializes")
    )
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::shorthand::credentials::{CredentialStore, MemoryBackend};

    use super::*;

    fn llm(provider: LlmProvider, origin: &str) -> CredentialSlot {
        CredentialSlot::NotesLlm {
            provider,
            origin: origin.into(),
        }
    }

    #[test]
    fn openai_gets_bearer_and_loses_client_supplied_auth() {
        let mut headers = HashMap::from([
            (
                "authorization".to_string(),
                "Bearer placeholder".to_string(),
            ),
            ("content-type".to_string(), "application/json".to_string()),
        ]);
        let plan = plan_request(
            &llm(LlmProvider::Openai, "https://api.openai.com"),
            Some("sk-test"),
            "https://api.openai.com/v1/chat/completions",
            "POST",
            &headers,
        )
        .unwrap();
        assert_eq!(plan.headers.get("authorization").unwrap(), "Bearer sk-test");
        assert_eq!(
            plan.headers.get("content-type").unwrap(),
            "application/json"
        );
        headers.clear();
    }

    #[test]
    fn anthropic_uses_x_api_key() {
        let plan = plan_request(
            &llm(LlmProvider::Anthropic, "https://api.anthropic.com"),
            Some("sk-ant"),
            "https://api.anthropic.com/v1/messages",
            "POST",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(plan.headers.get("x-api-key").unwrap(), "sk-ant");
        assert!(plan.headers.get("authorization").is_none());
    }

    #[test]
    fn origin_mismatch_is_refused_before_any_io() {
        assert!(matches!(
            plan_request(
                &llm(LlmProvider::Openai, "https://api.openai.com"),
                Some("sk"),
                "https://evil.example/v1/chat/completions",
                "POST",
                &HashMap::new(),
            ),
            Err(ErrorCode::OriginMismatch)
        ));
        assert!(matches!(
            plan_request(
                &llm(LlmProvider::Openai, "https://api.openai.com"),
                Some("sk"),
                "https://api.openai.com:8443/v1",
                "POST",
                &HashMap::new(),
            ),
            Err(ErrorCode::OriginMismatch)
        ));
        assert!(matches!(
            plan_request(
                &llm(LlmProvider::Openai, "https://api.openai.com"),
                Some("sk"),
                "https://user:pw@api.openai.com/v1",
                "POST",
                &HashMap::new(),
            ),
            Err(ErrorCode::BadRequest)
        ));
    }

    #[test]
    fn hosted_provider_without_secret_is_credential_missing_but_local_is_fine() {
        assert!(matches!(
            plan_request(
                &llm(LlmProvider::Openai, "https://api.openai.com"),
                None,
                "https://api.openai.com/v1",
                "POST",
                &HashMap::new(),
            ),
            Err(ErrorCode::CredentialMissing)
        ));
        let plan = plan_request(
            &llm(LlmProvider::Ollama, "http://127.0.0.1:11434"),
            None,
            "http://127.0.0.1:11434/api/chat",
            "POST",
            &HashMap::new(),
        )
        .unwrap();
        assert!(plan.headers.get("authorization").is_none());
    }

    #[test]
    fn acp_slot_gets_bearer() {
        let slot = CredentialSlot::NotesAcp {
            vault_id: "v".into(),
            origin: "https://agent.example".into(),
        };
        let plan = plan_request(
            &slot,
            Some("tok"),
            "https://agent.example/acp",
            "GET",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(plan.headers.get("authorization").unwrap(), "Bearer tok");
    }

    /// `ws_relay.rs` (A6) calls `plan_request` with a `ws://`/`wss://` url for
    /// `ws.open`, since a `notes-acp` slot's origin can itself be `ws`/`wss`
    /// (the wire contract's own `SLOT` example is `"wss://agent.example"`).
    /// This pins that the scheme broadening didn't loosen the origin check
    /// itself: same-origin `wss` succeeds, cross-scheme and cross-host still
    /// fail exactly as they do for `http`/`https`.
    #[test]
    fn ws_and_wss_origins_are_accepted_and_still_origin_bound() {
        let slot = CredentialSlot::NotesAcp {
            vault_id: "v".into(),
            origin: "wss://agent.example".into(),
        };
        let plan = plan_request(
            &slot,
            Some("tok"),
            "wss://agent.example/acp",
            "GET",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(plan.headers.get("authorization").unwrap(), "Bearer tok");

        assert!(matches!(
            plan_request(
                &slot,
                Some("tok"),
                "wss://evil.example/acp",
                "GET",
                &HashMap::new(),
            ),
            Err(ErrorCode::OriginMismatch)
        ));
        // A slot whose origin is `wss` must not be satisfied by a plain `ws`
        // url to the same host: the scheme is part of the origin tuple.
        assert!(matches!(
            plan_request(
                &slot,
                Some("tok"),
                "ws://agent.example/acp",
                "GET",
                &HashMap::new(),
            ),
            Err(ErrorCode::OriginMismatch)
        ));
    }

    fn store() -> Arc<CredentialStore> {
        Arc::new(CredentialStore::with_backend(Box::new(
            MemoryBackend::default(),
        )))
    }

    /// What `serve_raw` hands back: the server's own URL, the exact bytes the
    /// connecting client sent (headers and, if any, body — review test-gap
    /// "nothing asserts what the upstream actually received"), and how many
    /// connections the listener accepted in total. A followed redirect would
    /// show up as a second accept; `accept_count` lets a test prove one never
    /// arrived instead of trusting `reqwest::redirect::Policy::none()` alone
    /// (review test-gap "the redirect test cannot observe a followed
    /// redirect").
    struct RawServer {
        url: String,
        request: std::sync::mpsc::Receiver<Vec<u8>>,
        accept_count: Arc<AtomicUsize>,
    }

    /// A one-shot raw HTTP server on loopback, modelled on
    /// `llm_client::tests::serve_one_response`: `run_fetch` is the client
    /// under test, so this needs to control status line, headers and how the
    /// body is written onto the wire more precisely than a full HTTP library
    /// would let it. Runs on a blocking OS thread (plain `std::net`) so it
    /// keeps writing on its own schedule regardless of the async runtime the
    /// test body drives `run_fetch` on.
    fn serve_raw(
        status_line: &'static str,
        extra_headers: String,
        write_body: impl FnOnce(&mut std::net::TcpStream) + Send + 'static,
    ) -> RawServer {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let accept_count = Arc::new(AtomicUsize::new(0));
        let accept_count_thread = Arc::clone(&accept_count);
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            accept_count_thread.fetch_add(1, Ordering::SeqCst);

            // Drain whatever the client sent instead of reading into a fixed
            // buffer once: a short read timeout tells "the client stopped
            // writing" apart from "there is more coming" without guessing a
            // size that a real request (a decoded body included) might
            // exceed.
            stream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                    Err(_) => break, // Timed out: the client is done writing.
                }
            }
            let _ = request_tx.send(request);

            let header =
                format!("HTTP/1.1 {status_line}\r\n{extra_headers}Connection: close\r\n\r\n");
            let _ = stream.write_all(header.as_bytes());
            write_body(&mut stream);
            drop(stream);

            // A followed redirect would arrive as a second connection; give
            // one a short window rather than assuming silence means none
            // did. This runs on its own thread and nothing joins it, so it
            // never adds latency to a test that does not read `accept_count`.
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_millis(150);
            while std::time::Instant::now() < deadline {
                if listener.accept().is_ok() {
                    accept_count_thread.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        RawServer {
            url: format!("http://{address}"),
            request: request_rx,
            accept_count,
        }
    }

    fn ollama_slot(origin: &str) -> CredentialSlot {
        CredentialSlot::NotesLlm {
            provider: LlmProvider::Ollama,
            origin: origin.into(),
        }
    }

    fn fetch_params(slot: CredentialSlot, url: &str) -> HttpFetchParams {
        HttpFetchParams {
            slot,
            url: url.to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        }
    }

    /// Mirrors what `spawn_http_fetch` does in production before it ever
    /// calls `run_fetch`: registers a placeholder abort handle for `id`. The
    /// tests here call `run_fetch` directly, bypassing `spawn_http_fetch`, so
    /// without this the normal-completion path's "is my id still registered"
    /// check (see `run_fetch`'s handling of review finding Minor-8) would
    /// always see an unregistered id and skip `http.end`, even though nothing
    /// aborted the request.
    fn register_inflight(ctx: &ConnectionContext, id: &str) {
        let placeholder = tokio::spawn(async {});
        ctx.inflight_lock()
            .insert(id.to_string(), placeholder.abort_handle());
    }

    #[tokio::test]
    async fn streams_the_body_across_events_then_ends() {
        let chunks: [&str; 3] = ["hello ", "brave ", "world"];
        let body: String = chunks.concat();
        let server = serve_raw(
            "200 OK",
            format!("Content-Length: {}\r\n", body.len()),
            move |stream| {
                for chunk in chunks {
                    let _ = stream.write_all(chunk.as_bytes());
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_millis(20));
                }
            },
        );

        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        register_inflight(&ctx, "r1");
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = fetch_params(ollama_slot(&server.url), &server.url);
        run_fetch(ctx, "r1".into(), params, tx).await;

        let ok = rx.recv().await.unwrap();
        let ok_value: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok_value["ok"], true);
        assert_eq!(ok_value["result"]["status"], 200);

        let mut collected = Vec::new();
        let mut saw_body = false;
        loop {
            let line = rx.recv().await.expect("connection ended before http.end");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            if value["t"] == "http.end" {
                break;
            }
            assert_eq!(value["t"], "http.body", "unexpected line: {line}");
            saw_body = true;
            let data = value["data"].as_str().unwrap();
            collected.extend(STANDARD.decode(data).unwrap());
        }
        assert!(saw_body);
        assert_eq!(String::from_utf8(collected).unwrap(), body);
    }

    #[tokio::test]
    async fn redirect_status_is_reported_without_being_followed() {
        let server = serve_raw(
            "302 Found",
            "Location: https://evil.example/\r\nContent-Length: 0\r\n".to_string(),
            |_stream| {},
        );

        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        register_inflight(&ctx, "r1");
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = fetch_params(ollama_slot(&server.url), &server.url);
        run_fetch(ctx, "r1".into(), params, tx).await;

        let ok = rx.recv().await.unwrap();
        let ok_value: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok_value["result"]["status"], 302);
        assert_eq!(
            ok_value["result"]["headers"]["location"], "https://evil.example/",
            "unexpected headers: {ok_value}"
        );

        let end = rx.recv().await.unwrap();
        let end_value: serde_json::Value = serde_json::from_str(&end).unwrap();
        assert_eq!(end_value["t"], "http.end", "expected http.end, got: {end}");

        // Give the server's post-response listen window (see `serve_raw`)
        // time to notice a second connection, if `reqwest` had ever started
        // following the redirect instead of just reporting its status.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            server.accept_count.load(Ordering::SeqCst),
            1,
            "the redirect must never be followed"
        );
    }

    #[tokio::test]
    async fn response_larger_than_the_cap_is_reported_too_large() {
        // The cap this proxies enforces is on bytes actually received, never
        // on a trusted `Content-Length`: this header lies (100 MiB) while the
        // handler only ever writes enough real bytes to cross the 64 MiB cap.
        const REAL_BODY_LEN: usize = 65 * 1024 * 1024;
        let server = serve_raw(
            "200 OK",
            "Content-Length: 104857600\r\n".to_string(),
            |stream| {
                let chunk = vec![b'a'; 1024 * 1024];
                for _ in 0..(REAL_BODY_LEN / chunk.len()) {
                    if stream.write_all(&chunk).is_err() {
                        break;
                    }
                }
            },
        );

        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = fetch_params(ollama_slot(&server.url), &server.url);
        // `run_fetch` streams the 65 MiB body as thousands of `http.body`
        // events into a channel with capacity 64: awaiting it to completion
        // before ever reading `rx` deadlocks the sender against a full,
        // undrained channel. Spawning it and draining `rx` concurrently, as
        // a real connection's writer task would, lets the cap actually be
        // reached.
        let fetch = tokio::spawn(run_fetch(ctx, "r1".into(), params, tx));

        let ok = rx.recv().await.unwrap();
        let ok_value: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok_value["result"]["status"], 200);

        loop {
            let line = rx
                .recv()
                .await
                .expect("connection ended before an error arrived");
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            if value["t"] == "http.error" {
                assert_eq!(value["request"], "r1");
                assert!(
                    value["message"].as_str().unwrap().contains("64 MiB"),
                    "unexpected error line: {line}"
                );
                break;
            }
        }
        fetch.await.unwrap();
    }

    /// The `plan_request` tests above only inspect an in-memory `HeaderMap` —
    /// they would still pass if `.headers(planned.headers)` were dropped
    /// before the request is actually sent. This drives a real `run_fetch`
    /// against `serve_raw` and inspects the bytes that left the process:
    /// the injected secret must be on the wire, the client's own
    /// `authorization` must not be, and the decoded body must have been sent
    /// (review test-gap "nothing asserts what the upstream actually
    /// received").
    #[tokio::test]
    async fn injected_auth_reaches_the_wire_client_auth_does_not_and_body_is_sent() {
        let server = serve_raw("200 OK", "Content-Length: 0\r\n".to_string(), |_stream| {});

        let credential_store = store();
        let slot = llm(LlmProvider::Openai, &server.url);
        credential_store.set(&slot, "sk-real").unwrap();

        let ctx = Arc::new(ConnectionContext::for_tests(credential_store, "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = HttpFetchParams {
            slot,
            url: server.url.clone(),
            method: "POST".to_string(),
            headers: HashMap::from([
                (
                    "authorization".to_string(),
                    "Bearer client-supplied".to_string(),
                ),
                ("content-type".to_string(), "application/json".to_string()),
            ]),
            body: Some(STANDARD.encode(b"{\"hello\":true}")),
        };
        run_fetch(ctx, "r1".into(), params, tx).await;

        let ok = rx.recv().await.unwrap();
        let ok_value: serde_json::Value = serde_json::from_str(&ok).unwrap();
        assert_eq!(ok_value["result"]["status"], 200);

        let raw_request = server
            .request
            .recv_timeout(Duration::from_secs(1))
            .expect("the server thread never received a request");
        let request_text = String::from_utf8_lossy(&raw_request);
        assert!(
            request_text.contains("authorization: Bearer sk-real"),
            "the injected secret never reached the wire: {request_text}"
        );
        assert!(
            !request_text.contains("client-supplied"),
            "the client-supplied authorization header reached the wire: {request_text}"
        );
        assert!(
            request_text.contains("{\"hello\":true}"),
            "the decoded body was not sent: {request_text}"
        );
    }

    #[tokio::test]
    async fn request_body_over_the_cap_is_too_large() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let oversized = STANDARD.encode(vec![b'a'; 16 * 1024 * 1024 + 1]);
        // No real network attempt is made: `decode_body`'s cap check runs
        // before `plan_request` or any I/O, so the target does not need to
        // be reachable.
        let params = HttpFetchParams {
            slot: ollama_slot("http://127.0.0.1:1"),
            url: "http://127.0.0.1:1/".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            body: Some(oversized),
        };
        run_fetch(ctx, "r1".into(), params, tx).await;

        let error = rx.recv().await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&error).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "too_large");
    }

    #[tokio::test]
    async fn non_base64_body_is_bad_request() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = HttpFetchParams {
            slot: ollama_slot("http://127.0.0.1:1"),
            url: "http://127.0.0.1:1/".to_string(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            body: Some("not valid base64 !!".to_string()),
        };
        run_fetch(ctx, "r1".into(), params, tx).await;

        let error = rx.recv().await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&error).unwrap();
        assert_eq!(value["error"]["code"], "bad_request");
    }

    #[tokio::test]
    async fn set_cookie_is_dropped_and_response_headers_are_lower_cased() {
        let server = serve_raw(
            "200 OK",
            "Content-Length: 0\r\nSet-Cookie: session=abc\r\nX-Upstream: Value\r\n".to_string(),
            |_stream| {},
        );

        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = fetch_params(ollama_slot(&server.url), &server.url);
        run_fetch(ctx, "r1".into(), params, tx).await;

        let ok = rx.recv().await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&ok).unwrap();
        let headers = value["result"]["headers"]
            .as_object()
            .expect("headers must be an object");
        assert!(
            !headers.contains_key("set-cookie") && !headers.contains_key("Set-Cookie"),
            "set-cookie leaked: {headers:?}"
        );
        assert_eq!(headers.get("x-upstream").unwrap(), "Value");
    }

    #[tokio::test]
    async fn acp_slot_without_a_secret_is_credential_missing() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let params = HttpFetchParams {
            slot: CredentialSlot::NotesAcp {
                vault_id: "v".to_string(),
                origin: "https://agent.example".to_string(),
            },
            url: "https://agent.example/acp".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
        };
        // No real network attempt: `notes-acp` never allows an unauthenticated
        // request, so `plan_request` rejects this before any I/O.
        run_fetch(ctx, "r1".into(), params, tx).await;

        let error = rx.recv().await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&error).unwrap();
        assert_eq!(value["error"]["code"], "credential_missing");
    }

    #[tokio::test]
    async fn abort_removes_the_inflight_entry_and_reports_aborted() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);

        // A never-finishing task stands in for a real in-flight fetch: only
        // `handle_abort`'s bookkeeping (map lookup, abort, event) is under
        // test here, not a real network call.
        let never_finishes = tokio::spawn(async {
            std::future::pending::<()>().await;
        });
        ctx.inflight
            .lock()
            .unwrap()
            .insert("r4".to_string(), never_finishes.abort_handle());

        handle_abort("r5", "r4", &ctx, &tx).await;

        let error = rx.recv().await.unwrap();
        assert_eq!(
            error,
            "{\"t\":\"http.error\",\"request\":\"r4\",\"message\":\"aborted\"}\n"
        );
        let ok = rx.recv().await.unwrap();
        assert_eq!(ok, "{\"id\":\"r5\",\"ok\":true,\"result\":{}}\n");
        assert!(ctx.inflight.lock().unwrap().is_empty());
        assert!(never_finishes.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn abort_of_an_unknown_request_still_acknowledges() {
        let ctx = Arc::new(ConnectionContext::for_tests(store(), "0.5.0"));
        let (tx, mut rx) = mpsc::channel::<String>(64);

        handle_abort("r5", "not-in-flight", &ctx, &tx).await;

        let ok = rx.recv().await.unwrap();
        assert_eq!(ok, "{\"id\":\"r5\",\"ok\":true,\"result\":{}}\n");
    }
}
