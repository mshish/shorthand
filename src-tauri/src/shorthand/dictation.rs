//! Dictation-mode settings and the per-mode field resolver. See
//! docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.

use super::mode::{self, Mode};
use crate::settings::{
    AppSettings, AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool,
};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Dictation's own copy of settings meeting mode also has, so enabling or
/// configuring dictation never touches a meeting-mode value. See "Per-mode
/// and shared settings" in the design doc for which fields live here versus
/// staying shared on `AppSettings`.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct DictationSettings {
    pub enabled: bool,
    pub push_to_talk: bool,
    pub paste_method: PasteMethod,
    pub clipboard_handling: ClipboardHandling,
    pub auto_submit: bool,
    pub auto_submit_key: AutoSubmitKey,
    pub append_trailing_space: bool,
    pub typing_tool: TypingTool,
    pub overlay_style: OverlayStyle,
    pub save_recordings: bool,
    pub save_transcripts: bool,
    pub post_process_enabled: bool,
    pub post_process_selected_prompt_id: Option<String>,
}

impl Default for DictationSettings {
    // `PasteMethod`'s own `#[default]` is `None` (see settings.rs) because
    // this fork delivers meeting transcripts to follower processes instead
    // of the focused window. Dictation is the opposite: pasting into the
    // focused window is the entire feature, so it must NOT inherit that
    // default — it needs Handy's original per-platform choice. Do not
    // collapse this back into a derived `Default`; that would silently
    // reintroduce `PasteMethod::None` here.
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        let paste_method = PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        let paste_method = PasteMethod::CtrlV;

        Self {
            enabled: false,
            // Meetings run an hour and are toggled; dictation is seconds and
            // is held.
            push_to_talk: true,
            paste_method,
            clipboard_handling: ClipboardHandling::default(),
            auto_submit: false,
            auto_submit_key: AutoSubmitKey::default(),
            append_trailing_space: false,
            typing_tool: TypingTool::default(),
            // The compact pill, not a live-transcript panel over the text
            // field being dictated into.
            overlay_style: OverlayStyle::Minimal,
            // Consent, not preference — stays opt-in like meeting mode's
            // equivalent toggles.
            save_recordings: false,
            save_transcripts: false,
            post_process_enabled: false,
            post_process_selected_prompt_id: None,
        }
    }
}

/// Whether push-to-talk applies to `binding_id`'s capture. Read at dispatch
/// time in `shortcut::handler::handle_shortcut_event`, before
/// `TranscribeAction::start` runs — so, unlike every other resolver in this
/// module, it cannot go through the mode cell (`mode::active` isn't updated
/// for this press yet). It derives the mode from `binding_id` directly
/// instead, the same way `mode::set_active` will a moment later.
pub fn resolve_push_to_talk(settings: &AppSettings, binding_id: &str) -> bool {
    match mode::mode_for_binding(binding_id) {
        Mode::Dictation => settings.dictation.push_to_talk,
        Mode::Meeting => settings.push_to_talk,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paste_method_is_not_none() {
        assert_ne!(DictationSettings::default().paste_method, PasteMethod::None);
    }

    #[test]
    fn resolve_push_to_talk_reads_the_matching_mode_field() {
        let mut settings = crate::settings::get_default_settings();
        settings.push_to_talk = false;
        settings.dictation.push_to_talk = true;

        assert!(!resolve_push_to_talk(&settings, "transcribe"));
        assert!(!resolve_push_to_talk(&settings, "cancel"));
        assert!(resolve_push_to_talk(&settings, "dictate"));
        assert!(resolve_push_to_talk(&settings, "dictate_with_post_process"));
    }
}
