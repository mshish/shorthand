//! The request socket's wire types: the envelope every line arrives in, the
//! parsed `Request` it decodes to, and the three line-builders every reply
//! goes through. See shared-constraints-and-wire-contract.md (Buzz
//! workspace, PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md) for
//! the full wire contract this mirrors.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::shorthand::credentials::CredentialSlot;

/// Bumped whenever a change here is not backward-compatible for an already
/// running client (core, the Obsidian plugin). Carried in `hello` so a
/// client can refuse to talk to a server it does not understand.
pub const REQUEST_PROTOCOL_VERSION: u32 = 1;

/// Advertised in `hello` so a client can tell which method families this
/// build answers at all, independent of whether an individual method is
/// implemented yet (see `dispatch` in server.rs for that finer distinction).
pub const CAPABILITIES: &[&str] = &["credential", "http-fetch", "ws-relay"];

/// One NDJSON line's envelope, common to every request. `params` is decoded
/// again, per method, once `method` says which shape to expect.
#[derive(Debug, Deserialize)]
pub struct Envelope {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// The wire's error codes (shared-constraints-and-wire-contract.md); `Serialize`
/// renders each as the exact snake_case token clients match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    UnknownMethod,
    OriginMismatch,
    CredentialMissing,
    CredentialUnavailable,
    TooLarge,
    Timeout,
    Upstream,
    Limit,
}

/// A successfully parsed request, one variant per wire method. `HttpFetch`
/// and `WsOpen` carry a named params struct (rather than inline fields) per
/// task-A4-carryovers.md, because A5 and A6 consume those structs by name.
///
/// `Debug` is hand-written, not derived: `credentials.rs`'s module doc
/// promises "nothing here can put a secret in a log line, an error, or a
/// settings payload", and a derived `Debug` would print `CredentialSet`'s
/// plaintext `secret` the moment anything ever logged a `Request`.
pub enum Request {
    CredentialSet {
        slot: CredentialSlot,
        secret: String,
    },
    CredentialClear {
        slot: CredentialSlot,
    },
    CredentialStatus {
        slots: Vec<StatusSlot>,
    },
    HttpFetch(HttpFetchParams),
    HttpAbort {
        request: String,
    },
    WsOpen(WsOpenParams),
    WsSend {
        stream: String,
        data: String,
    },
    WsClose {
        stream: String,
        code: u16,
        reason: String,
    },
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::CredentialSet { slot, secret: _ } => f
                .debug_struct("CredentialSet")
                .field("slot", slot)
                .field("secret", &"[REDACTED]")
                .finish(),
            Request::CredentialClear { slot } => f
                .debug_struct("CredentialClear")
                .field("slot", slot)
                .finish(),
            Request::CredentialStatus { slots } => f
                .debug_struct("CredentialStatus")
                .field("slots", slots)
                .finish(),
            Request::HttpFetch(params) => f.debug_tuple("HttpFetch").field(params).finish(),
            Request::HttpAbort { request } => f
                .debug_struct("HttpAbort")
                .field("request", request)
                .finish(),
            Request::WsOpen(params) => f.debug_tuple("WsOpen").field(params).finish(),
            Request::WsSend { stream, data } => f
                .debug_struct("WsSend")
                .field("stream", stream)
                .field("data", data)
                .finish(),
            Request::WsClose {
                stream,
                code,
                reason,
            } => f
                .debug_struct("WsClose")
                .field("stream", stream)
                .field("code", code)
                .field("reason", reason)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpFetchParams {
    pub slot: CredentialSlot,
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// One element of a `credential.status` request. `raw` is the slot exactly
/// as the client sent it — echoed back verbatim in the response, per the
/// wire contract, so a client comparing the echo against what it sent (to
/// correlate entries, since responses are otherwise positional) sees its own
/// bytes back, not a server-side rewrite. `canonical` is the same slot
/// parsed and canonicalised, used only to look the secret up and to detect
/// duplicate slots within one request.
#[derive(Debug)]
pub struct StatusSlot {
    pub raw: serde_json::Value,
    pub canonical: CredentialSlot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsOpenParams {
    pub slot: CredentialSlot,
    pub url: String,
    #[serde(default)]
    pub protocols: Vec<String>,
}

#[derive(Deserialize)]
struct CredentialSetParams {
    slot: CredentialSlot,
    secret: String,
}

#[derive(Deserialize)]
struct CredentialClearParams {
    slot: CredentialSlot,
}

#[derive(Deserialize)]
struct CredentialStatusParams {
    /// Kept as raw JSON, not `Vec<CredentialSlot>`: `credential.status` must
    /// echo each slot back exactly as received (see `StatusSlot`), and the
    /// canonical form serde produces on the way into `CredentialSlot` is not
    /// reversible back to the client's original bytes.
    slots: Vec<serde_json::Value>,
}

/// Cap on `credential.status`'s `slots` array. Generous for any real caller
/// (core and the plugin ask about a handful of provider/vault slots at a
/// time) while bounding the O(n) canonicalisation and O(n) dedup work one
/// request can force the server to do.
const MAX_STATUS_SLOTS: usize = 64;

#[derive(Deserialize)]
struct HttpAbortParams {
    request: String,
}

#[derive(Deserialize)]
struct WsSendParams {
    stream: String,
    data: String,
}

#[derive(Deserialize)]
struct WsCloseParams {
    stream: String,
    code: u16,
    reason: String,
}

/// The app's own post-processing key is never proxied and never arrives
/// from a client: `CredentialSlot` cannot be narrowed to just the wire
/// variants at the type level (serde has no way to exclude one variant from
/// deserializing), so every method that carries a slot rejects this one
/// explicitly instead.
fn reject_post_process(slot: &CredentialSlot) -> Result<(), ErrorCode> {
    match slot {
        CredentialSlot::PostProcess { .. } => Err(ErrorCode::BadRequest),
        CredentialSlot::NotesLlm { .. } | CredentialSlot::NotesAcp { .. } => Ok(()),
    }
}

const MAX_ID_LEN: usize = 128;

/// Parses one NDJSON line into its id and request. The error carries an id
/// too, whenever one is available to trust: once the envelope has
/// deserialised and `id` has passed validation, that id is real — the
/// client's own id, later used to correlate replies — regardless of whether
/// `method`/`params` then turn out to be invalid. A client that never gets
/// an id back for a request it sent has no way to tell its request apart
/// from one that was silently dropped, and (per the wire contract) waits out
/// the full per-request timeout instead of failing fast. Only a line that
/// never produced a usable id in the first place — unparseable JSON, or an
/// empty/oversized `id` field — answers with `""`, because there is nothing
/// in it worth echoing back as if it correlated to anything.
pub fn parse_line(line: &str) -> Result<(String, Request), (String, ErrorCode)> {
    let envelope: Envelope =
        serde_json::from_str(line).map_err(|_| (String::new(), ErrorCode::BadRequest))?;
    if envelope.id.is_empty() || envelope.id.chars().count() > MAX_ID_LEN {
        return Err((String::new(), ErrorCode::BadRequest));
    }
    match parse_request(&envelope.method, envelope.params) {
        Ok(request) => Ok((envelope.id, request)),
        Err(code) => Err((envelope.id, code)),
    }
}

fn parse_request(method: &str, params: serde_json::Value) -> Result<Request, ErrorCode> {
    match method {
        "credential.set" => {
            let params: CredentialSetParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            reject_post_process(&params.slot)?;
            Ok(Request::CredentialSet {
                slot: params.slot,
                secret: params.secret,
            })
        }
        "credential.clear" => {
            let params: CredentialClearParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            reject_post_process(&params.slot)?;
            Ok(Request::CredentialClear { slot: params.slot })
        }
        "credential.status" => {
            let params: CredentialStatusParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            if params.slots.len() > MAX_STATUS_SLOTS {
                return Err(ErrorCode::BadRequest);
            }
            let mut seen_services = HashSet::with_capacity(params.slots.len());
            let mut slots = Vec::with_capacity(params.slots.len());
            for raw in params.slots {
                let canonical: CredentialSlot =
                    serde_json::from_value(raw.clone()).map_err(|_| ErrorCode::BadRequest)?;
                reject_post_process(&canonical)?;
                // Two slots that canonicalise to the same keyring entry (a
                // differently-cased origin, say) would otherwise get two
                // status entries for what is really one secret — and the
                // client has no way to know which one to trust.
                if !seen_services.insert(canonical.service()) {
                    return Err(ErrorCode::BadRequest);
                }
                slots.push(StatusSlot { raw, canonical });
            }
            Ok(Request::CredentialStatus { slots })
        }
        "http.fetch" => {
            let params: HttpFetchParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            reject_post_process(&params.slot)?;
            Ok(Request::HttpFetch(params))
        }
        "http.abort" => {
            let params: HttpAbortParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            Ok(Request::HttpAbort {
                request: params.request,
            })
        }
        "ws.open" => {
            let params: WsOpenParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            reject_post_process(&params.slot)?;
            Ok(Request::WsOpen(params))
        }
        "ws.send" => {
            let params: WsSendParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            Ok(Request::WsSend {
                stream: params.stream,
                data: params.data,
            })
        }
        "ws.close" => {
            let params: WsCloseParams =
                serde_json::from_value(params).map_err(|_| ErrorCode::BadRequest)?;
            Ok(Request::WsClose {
                stream: params.stream,
                code: params.code,
                reason: params.reason,
            })
        }
        _ => Err(ErrorCode::UnknownMethod),
    }
}

#[derive(Serialize)]
struct HelloLine<'a> {
    t: &'static str,
    protocol: u32,
    version: &'a str,
    capabilities: &'static [&'static str],
}

/// The server's mandatory first line on every connection.
pub fn hello_line(version: &str) -> String {
    let line = HelloLine {
        t: "hello",
        protocol: REQUEST_PROTOCOL_VERSION,
        version,
        capabilities: CAPABILITIES,
    };
    format!(
        "{}\n",
        serde_json::to_string(&line).expect("hello line always serializes")
    )
}

#[derive(Serialize)]
struct OkLine<'a> {
    id: &'a str,
    ok: bool,
    result: serde_json::Value,
}

/// A successful reply to the request named `id`.
pub fn ok_line(id: &str, result: serde_json::Value) -> String {
    let line = OkLine {
        id,
        ok: true,
        result,
    };
    format!(
        "{}\n",
        serde_json::to_string(&line).expect("ok line always serializes")
    )
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    code: ErrorCode,
    message: &'a str,
}

#[derive(Serialize)]
struct ErrorLine<'a> {
    id: &'a str,
    ok: bool,
    error: ErrorBody<'a>,
}

/// A failed reply. `message` is a plain string field, never interpolated
/// into `params` or any other structure that could carry client-controlled
/// JSON back out — see `error_line_never_echoes_params`.
pub fn error_line(id: &str, code: ErrorCode, message: &str) -> String {
    let line = ErrorLine {
        id,
        ok: false,
        error: ErrorBody { code, message },
    };
    format!(
        "{}\n",
        serde_json::to_string(&line).expect("error line always serializes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_is_exact() {
        assert_eq!(hello_line("0.5.0"), "{\"t\":\"hello\",\"protocol\":1,\"version\":\"0.5.0\",\"capabilities\":[\"credential\",\"http-fetch\",\"ws-relay\"]}\n");
    }

    #[test]
    fn parses_credential_set() {
        let (id, req) = parse_line(r#"{"id":"r1","method":"credential.set","params":{"slot":{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com"},"secret":"sk-test"}}"#).unwrap();
        assert_eq!(id, "r1");
        assert!(matches!(req, Request::CredentialSet { .. }));
    }

    /// M1: a derived `Debug` on `Request` would print `CredentialSet`'s
    /// plaintext secret; the hand-written impl must not.
    #[test]
    fn credential_set_debug_redacts_the_secret() {
        let (_, req) = parse_line(r#"{"id":"r1","method":"credential.set","params":{"slot":{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com"},"secret":"sk-super-secret"}}"#).unwrap();
        let debug = format!("{req:?}");
        assert!(
            !debug.contains("sk-super-secret"),
            "Debug output leaked the secret: {debug}"
        );
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn rejects_unknown_method_and_post_process_slots() {
        assert!(matches!(
            parse_line(r#"{"id":"r1","method":"nope","params":{}}"#),
            Err((ref id, ErrorCode::UnknownMethod)) if id == "r1"
        ));
        assert!(matches!(
            parse_line(
                r#"{"id":"r1","method":"credential.set","params":{"slot":{"kind":"post-process","provider_id":"openai"},"secret":"x"}}"#
            ),
            Err((ref id, ErrorCode::BadRequest)) if id == "r1"
        ));
    }

    /// A malformed/method-valid-but-params-invalid request still carries a
    /// trustworthy id once the envelope itself parsed — see `parse_line`'s
    /// doc comment. Only unparseable JSON, or a missing/oversized `id`
    /// field, has nothing worth echoing.
    #[test]
    fn an_id_that_deserialised_is_echoed_even_when_the_request_is_rejected() {
        assert!(matches!(
            parse_line(r#"{"id":"","method":"nope","params":{}}"#),
            Err((ref id, ErrorCode::BadRequest)) if id.is_empty()
        ));
        assert!(matches!(
            parse_line("{"),
            Err((ref id, ErrorCode::BadRequest)) if id.is_empty()
        ));
    }

    #[test]
    fn error_line_never_echoes_params() {
        let line = error_line("r1", ErrorCode::BadRequest, "slot is required");
        assert_eq!(
            line,
            "{\"id\":\"r1\",\"ok\":false,\"error\":{\"code\":\"bad_request\",\"message\":\"slot is required\"}}\n"
        );
    }

    /// The core client matches `credential.status` entries back to the slots
    /// it asked about by comparing the echoed `slot` against exactly the
    /// bytes it sent (client.ts). Echoing the canonicalised form instead —
    /// lower-cased origin, normalised port — makes that comparison fail for
    /// any client that did not already canonicalise on its own, so the raw
    /// JSON has to survive untouched even though `canonical` (used only for
    /// the store lookup) differs.
    #[test]
    fn credential_status_slot_echoes_the_raw_request_not_the_canonicalised_form() {
        let (_, req) = parse_line(r#"{"id":"r1","method":"credential.status","params":{"slots":[{"kind":"notes-llm","provider":"openai","origin":"https://API.OpenAI.com"}]}}"#).unwrap();
        let Request::CredentialStatus { slots } = req else {
            panic!("expected a CredentialStatus request");
        };
        assert_eq!(
            slots[0].raw,
            serde_json::json!({"kind":"notes-llm","provider":"openai","origin":"https://API.OpenAI.com"})
        );
        assert_eq!(slots[0].canonical.origin(), Some("https://api.openai.com"));
    }

    /// Two slots the client wrote differently but that canonicalise to the
    /// same keyring entry would otherwise get two status entries for one
    /// secret, with no principled way to pick which the client should trust.
    #[test]
    fn credential_status_rejects_duplicate_canonical_slots() {
        assert!(matches!(
            parse_line(r#"{"id":"r1","method":"credential.status","params":{"slots":[{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com"},{"kind":"notes-llm","provider":"openai","origin":"https://API.OpenAI.com"}]}}"#),
            Err((ref id, ErrorCode::BadRequest)) if id == "r1"
        ));
    }

    #[test]
    fn credential_status_preserves_request_order() {
        let (_, req) = parse_line(r#"{"id":"r1","method":"credential.status","params":{"slots":[{"kind":"notes-llm","provider":"anthropic","origin":"https://api.anthropic.com"},{"kind":"notes-llm","provider":"openai","origin":"https://api.openai.com"}]}}"#).unwrap();
        let Request::CredentialStatus { slots } = req else {
            panic!("expected a CredentialStatus request");
        };
        assert_eq!(
            slots[0].canonical.origin(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(slots[1].canonical.origin(), Some("https://api.openai.com"));
    }

    #[test]
    fn credential_status_caps_the_slot_count() {
        let slots: Vec<String> = (0..65)
            .map(|i| {
                format!(
                    r#"{{"kind":"notes-llm","provider":"openai-compatible","origin":"http://host{i}.example"}}"#
                )
            })
            .collect();
        let line = format!(
            r#"{{"id":"r1","method":"credential.status","params":{{"slots":[{}]}}}}"#,
            slots.join(",")
        );
        assert!(matches!(
            parse_line(&line),
            Err((ref id, ErrorCode::BadRequest)) if id == "r1"
        ));
    }
}
