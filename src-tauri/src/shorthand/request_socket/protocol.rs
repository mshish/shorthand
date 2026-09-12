//! The request socket's wire types: the envelope every line arrives in, the
//! parsed `Request` it decodes to, and the three line-builders every reply
//! goes through. See shared-constraints-and-wire-contract.md (Buzz
//! workspace, PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md) for
//! the full wire contract this mirrors.

use std::collections::HashMap;

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
#[derive(Debug)]
pub enum Request {
    CredentialSet {
        slot: CredentialSlot,
        secret: String,
    },
    CredentialClear {
        slot: CredentialSlot,
    },
    CredentialStatus {
        slots: Vec<CredentialSlot>,
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
    slots: Vec<CredentialSlot>,
}

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

/// Parses one NDJSON line into its id and request. The id is deliberately
/// not returned on any error path: an envelope that failed validation
/// (unparseable JSON, an empty or oversized id, an unknown method, a
/// rejected slot) has nothing in it the caller should trust enough to echo
/// back to the client as if it correlated to their request.
pub fn parse_line(line: &str) -> Result<(String, Request), ErrorCode> {
    let envelope: Envelope = serde_json::from_str(line).map_err(|_| ErrorCode::BadRequest)?;
    if envelope.id.is_empty() || envelope.id.chars().count() > MAX_ID_LEN {
        return Err(ErrorCode::BadRequest);
    }
    let request = parse_request(&envelope.method, envelope.params)?;
    Ok((envelope.id, request))
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
            for slot in &params.slots {
                reject_post_process(slot)?;
            }
            Ok(Request::CredentialStatus {
                slots: params.slots,
            })
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

    #[test]
    fn rejects_unknown_method_and_post_process_slots() {
        assert!(matches!(
            parse_line(r#"{"id":"r1","method":"nope","params":{}}"#),
            Err(ErrorCode::UnknownMethod)
        ));
        assert!(matches!(
            parse_line(
                r#"{"id":"r1","method":"credential.set","params":{"slot":{"kind":"post-process","provider_id":"openai"},"secret":"x"}}"#
            ),
            Err(ErrorCode::BadRequest)
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

    /// Carry-over rule (task-A4-carryovers.md): the core client sends an
    /// already-lower-cased origin, so canonicalisation is normally a no-op.
    /// This pins the fallback case — the server never echoes raw client
    /// bytes, only the parsed-and-recanonicalised slot — so a malformed or
    /// non-conforming client cannot smuggle a differently-cased origin
    /// through the echo and defeat a same-origin comparison downstream.
    #[test]
    fn credential_status_slot_echoes_the_canonicalised_origin_not_the_raw_host_case() {
        let (_, req) = parse_line(r#"{"id":"r1","method":"credential.status","params":{"slots":[{"kind":"notes-llm","provider":"openai","origin":"https://API.OpenAI.com"}]}}"#).unwrap();
        let Request::CredentialStatus { slots } = req else {
            panic!("expected a CredentialStatus request");
        };
        assert_eq!(slots[0].origin(), Some("https://api.openai.com"));
    }
}
