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
    FollowEvent, Speaker, ERR_DISABLED, ERR_FOLLOWER_LIMIT, ERR_SERIALIZATION_FAILED,
    FOLLOW_PROTOCOL_VERSION,
};
pub use server::FollowStreamServer;

pub fn hub(app: &tauri::AppHandle) -> Option<Arc<FollowStreamHub>> {
    app.try_state::<Arc<FollowStreamHub>>()
        .map(|state| Arc::clone(&state))
}
