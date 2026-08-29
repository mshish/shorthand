//! Shared follow-stream listener lifetime policy.
//!
//! The socket is one process-wide resource, but three settings each want to
//! influence whether it exists: Meeting's top-level toggle (no enable switch
//! of its own — Meeting is always on), and Dictation's and Assisted Notes'
//! per-mode `follow_stream_enabled` fields, each gated behind that mode's own
//! `enabled`. Before this module existed, the listener was started and
//! stopped exclusively from Meeting's toggle (`lib.rs` at startup,
//! `change_follow_stream_enabled_setting` interactively), so a mode could
//! resolve `follow_stream_enabled: true` while turning Meeting's setting off
//! silently closed the socket under it — the UI would describe a capture the
//! transport was not actually serving. `listener_required` is the OR-of-
//! publishing-enabled-modes policy as one pure, unit-tested predicate, and
//! `reconcile` is the only place that acts on it, so the listener's existence
//! can never be traced to two call sites disagreeing about who owns it.

use std::sync::Arc;

use tauri::AppHandle;

use crate::settings::AppSettings;

use super::{FollowStreamHub, FollowStreamServer};

/// Whether the shared follow-stream listener must be running for `settings`
/// to keep its promises. Meeting has no enable switch, so its top-level
/// publication field alone decides its term. Dictation and Assisted Notes
/// only contribute when the mode itself is switched on: a publication
/// preference stored on a disabled mode can never actually publish, so it
/// must not keep the listener alive on its own.
pub fn listener_required(settings: &AppSettings) -> bool {
    settings.follow_stream_enabled
        || (settings.dictation.enabled && settings.dictation.follow_stream_enabled)
        || (settings.assisted_notes.enabled && settings.assisted_notes.follow_stream_enabled)
}

/// Starts or stops the shared listener so it matches
/// `listener_required(candidate)`, under the server's own lifecycle lock.
/// Callers pass the complete candidate `AppSettings` — the settings a command
/// is about to persist — rather than a boolean, so this is the only place the
/// OR policy above is evaluated. `FollowStreamServer::start` is idempotent
/// (a no-op when already running), so calling this from every settings
/// command that can affect the policy is safe even when the listener does
/// not actually need to change state.
pub async fn reconcile(
    app: &AppHandle,
    server: &FollowStreamServer,
    hub: Arc<FollowStreamHub>,
    candidate: &AppSettings,
) -> Result<(), String> {
    let _lifecycle_guard = server.lock_lifecycle().await;
    if listener_required(candidate) {
        server
            .start(app, hub)
            .await
            .map_err(|error| format!("Failed to start follow-stream listener: {error}"))
    } else {
        server.stop();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    fn settings_with(
        meeting: bool,
        dictation: (bool, bool),
        assisted: (bool, bool),
    ) -> AppSettings {
        let mut settings = get_default_settings();
        settings.follow_stream_enabled = meeting;
        settings.dictation.enabled = dictation.0;
        settings.dictation.follow_stream_enabled = dictation.1;
        settings.assisted_notes.enabled = assisted.0;
        settings.assisted_notes.follow_stream_enabled = assisted.1;
        settings
    }

    #[test]
    fn listener_required_is_false_when_nothing_can_publish() {
        assert!(!listener_required(&settings_with(
            false,
            (false, false),
            (false, false)
        )));
    }

    #[test]
    fn listener_required_covers_every_combination_of_the_three_publishing_terms() {
        // Meeting | Dictation (enabled && publishing) | Assisted Notes (enabled && publishing) | Required
        let cases = [
            (false, false, false, false),
            (true, false, false, true),
            (false, true, false, true),
            (false, false, true, true),
            (true, true, false, true),
            (true, false, true, true),
            (false, true, true, true),
            (true, true, true, true),
        ];

        for (meeting, dictation, assisted, expected) in cases {
            let settings = settings_with(meeting, (dictation, dictation), (assisted, assisted));
            assert_eq!(
                listener_required(&settings),
                expected,
                "meeting={meeting} dictation={dictation} assisted={assisted}"
            );
        }
    }

    #[test]
    fn a_disabled_dictation_mode_does_not_keep_the_listener_alive_on_its_own() {
        // The preference is on, but the mode itself never switched on, so it
        // can never actually publish anything.
        let settings = settings_with(false, (false, true), (false, false));
        assert!(!listener_required(&settings));
    }

    #[test]
    fn a_disabled_assisted_notes_mode_does_not_keep_the_listener_alive_on_its_own() {
        let settings = settings_with(false, (false, false), (false, true));
        assert!(!listener_required(&settings));
    }
}
