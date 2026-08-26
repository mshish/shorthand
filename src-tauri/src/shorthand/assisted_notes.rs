//! Assisted-notes-mode settings: "meeting, but solo". A follower process
//! fills the note live, exactly as it does for a meeting, but the capture is
//! never pasted into the focused window and never captures system audio. See
//! docs/superpowers/plans/2026-08-26-assisted-notes-mode.md.
//!
//! The per-mode *resolver* (`apply_mode`, `resolve_settings`,
//! `resolve_push_to_talk`) deliberately stays in `dictation.rs` rather than
//! moving to a neutrally-named module here: `crate::shorthand::dictation::resolve_settings`
//! is called from seven sites in upstream-owned files, and renaming the
//! module would touch every one of them for no behavioural gain.

use crate::settings::{ClipboardHandling, OverlayStyle};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Assisted notes' own copy of settings meeting mode also has, so enabling or
/// configuring assisted notes never touches a meeting-mode value. Mirrors the
/// shape of `dictation::DictationSettings`.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct AssistedNotesSettings {
    pub enabled: bool,
    pub push_to_talk: bool,
    pub clipboard_handling: ClipboardHandling,
    pub append_trailing_space: bool,
    pub overlay_style: OverlayStyle,
    pub save_recordings: bool,
    pub save_transcripts: bool,
    pub post_process_enabled: bool,
    pub post_process_selected_prompt_id: Option<String>,
    /// Whether this mode's transcript is published to `--follow-stream`
    /// followers. The defining similarity to a meeting: a follower process
    /// filling a note is the entire reason this mode exists.
    pub follow_stream_enabled: bool,
    /// Which post-processing provider this mode uses.
    pub post_process_provider_id: String,
    /// The model, when this mode wants one other than the provider's shared
    /// choice. `None` falls back to `AppSettings::post_process_models`, so a
    /// user who never sets it per mode sees no change in behaviour.
    pub post_process_model: Option<String>,
}

impl Default for AssistedNotesSettings {
    fn default() -> Self {
        Self {
            // Enabling registers two global shortcuts, which can collide with
            // another app. Fork-only features ship off (`AGENTS.md` § "Give
            // fork-only features a boundary").
            enabled: false,
            // A note-taking session runs as long as the thinking does, and
            // nobody holds a key for that. Meeting's reasoning applies
            // unchanged.
            push_to_talk: false,
            // Per-mode despite the mode never pasting: `clipboard::paste()`
            // runs its tail regardless of paste method, and the
            // `CopyToClipboard` branch writes the transcript to the clipboard
            // even under `PasteMethod::None`. Omitting this field would let
            // Meeting's value silently govern an Assisted Notes capture.
            clipboard_handling: ClipboardHandling::default(),
            // Live for the same reason: the appended text is what
            // `write_text_to_clipboard` receives.
            append_trailing_space: false,
            // The compact pill. The Live panel would sit on top of the note
            // being filled in — the exact window the user is watching.
            overlay_style: OverlayStyle::Minimal,
            // Dictation keeps its own audio and text by default. Meeting's
            // top-level defaults stay off because a meeting recording can
            // include other participants.
            save_recordings: true,
            save_transcripts: true,
            // Cleanup needs a configured provider and API key. Nothing that
            // calls a remote endpoint can ship on.
            post_process_enabled: false,
            post_process_selected_prompt_id: None,
            // The defining similarity to a meeting: a follower process filling
            // a note is the entire reason this mode exists.
            follow_stream_enabled: true,
            post_process_provider_id: crate::settings::default_post_process_provider_id(),
            post_process_model: None,
        }
    }
}
