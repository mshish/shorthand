mod client;
mod hub;
mod name;
mod protocol;
mod server;

use std::sync::Arc;

use tauri::Manager;

pub use client::run_client;
pub use hub::{
    FollowStreamHub, Follower, SubscribeError, MAX_BUFFERED_BYTES, MAX_FOLLOWERS, MAX_QUEUED_EVENTS,
};
pub use name::{socket_name, socket_name_owned};
pub use protocol::{
    CapturePhase, FollowEvent, FollowMode, RefusalReason, Speaker, StartFailureCode, ERR_DISABLED,
    ERR_FOLLOWER_LIMIT, ERR_SERIALIZATION_FAILED, FOLLOW_PROTOCOL_VERSION,
};
pub use server::FollowStreamServer;

// Fork-only: the request socket (src-tauri/src/shorthand/request_socket/)
// mirrors this module's listener lifecycle, protected DACL and peer check
// rather than duplicating them. These re-exports are its only coupling to
// follow-stream.
pub(crate) use name::current_identity;
pub(crate) use server::create_listener;
#[cfg(unix)]
pub(crate) use server::peer_is_current_user;

pub fn hub(app: &tauri::AppHandle) -> Option<Arc<FollowStreamHub>> {
    app.try_state::<Arc<FollowStreamHub>>()
        .map(|state| Arc::clone(&state))
}
