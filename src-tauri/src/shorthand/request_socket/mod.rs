//! Fork-only: the request socket is the NDJSON-framed local IPC channel core
//! and the Obsidian plugin use to reach credentials, an HTTP proxy, and a
//! WebSocket relay without either ever holding a provider secret themselves.
//! Design: PLANS/SHORTHAND_APP_CREDENTIALS_IMPLEMENTATION_PLAN.md (Buzz
//! workspace). Modelled directly on `follow_stream/`: `server.rs` mirrors
//! that module's listener lifecycle and reuses its listener-creation
//! helpers; `discovery.rs` is this module's own addition, since (unlike
//! follow-stream, which only this app's own CLI ever attaches to) an
//! external process needs a file to learn the live socket path from.

mod discovery;
mod http_proxy;
mod protocol;
mod server;
mod ws_relay;

pub use discovery::{config_directory, remove_discovery, write_discovery};
pub use protocol::{
    error_line, hello_line, ok_line, parse_line, Envelope, ErrorCode, HttpFetchParams, Request,
    StatusSlot, WsOpenParams, CAPABILITIES, REQUEST_PROTOCOL_VERSION,
};
pub use server::RequestSocketServer;
