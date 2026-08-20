//! Dictation-mode settings and the per-mode field resolver. See
//! docs/superpowers/specs/2026-08-20-shorthand-dictation-mode-design.md.

use super::mode::{self, Mode};
use crate::settings::{
    AppSettings, AutoSubmitKey, ClipboardHandling, OverlayStyle, PasteMethod, TypingTool,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

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

/// Pure and unit-testable. Returns `settings` unchanged for `Mode::Meeting`;
/// for `Mode::Dictation` returns a copy with the per-mode fields overridden
/// from `settings.dictation`. Because this returns a full `AppSettings`, its
/// callers (`clipboard::paste`, `overlay::show_overlay_state`, and the reads
/// in `actions.rs`) each change one line — `get_settings(x)` becomes
/// `resolve_settings(x)` — instead of taking a narrower struct that would
/// force real edits into their bodies.
pub fn apply_mode(settings: AppSettings, mode: Mode) -> AppSettings {
    match mode {
        Mode::Meeting => settings,
        Mode::Dictation => {
            let dictation = settings.dictation.clone();
            AppSettings {
                push_to_talk: dictation.push_to_talk,
                paste_method: dictation.paste_method,
                clipboard_handling: dictation.clipboard_handling,
                auto_submit: dictation.auto_submit,
                auto_submit_key: dictation.auto_submit_key,
                append_trailing_space: dictation.append_trailing_space,
                typing_tool: dictation.typing_tool,
                overlay_style: dictation.overlay_style,
                save_recordings: dictation.save_recordings,
                save_transcripts: dictation.save_transcripts,
                post_process_enabled: dictation.post_process_enabled,
                post_process_selected_prompt_id: dictation.post_process_selected_prompt_id,
                ..settings
            }
        }
    }
}

/// `apply_mode(get_settings(app), mode::active(app))` — the one call every
/// per-mode resolver in the upstream call sites makes.
pub fn resolve_settings(app: &AppHandle) -> AppSettings {
    apply_mode(crate::settings::get_settings(app), mode::active(app))
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

    #[test]
    fn apply_mode_leaves_every_field_unchanged_for_meeting() {
        let mut settings = crate::settings::get_default_settings();
        settings.push_to_talk = false;
        settings.paste_method = PasteMethod::CtrlV;
        settings.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.auto_submit = true;
        settings.auto_submit_key = AutoSubmitKey::CtrlEnter;
        settings.append_trailing_space = true;
        settings.typing_tool = TypingTool::Wtype;
        settings.overlay_style = OverlayStyle::Live;
        settings.save_recordings = true;
        settings.save_transcripts = true;
        settings.post_process_enabled = true;
        settings.post_process_selected_prompt_id = Some("meeting-prompt".to_string());

        // Deliberately different from every field above, so a leak from
        // `dictation` into the Meeting-mode result would be visible.
        settings.dictation.push_to_talk = false;
        settings.dictation.paste_method = PasteMethod::None;
        settings.dictation.clipboard_handling = ClipboardHandling::DontModify;
        settings.dictation.auto_submit = false;
        settings.dictation.auto_submit_key = AutoSubmitKey::Enter;
        settings.dictation.append_trailing_space = false;
        settings.dictation.typing_tool = TypingTool::Auto;
        settings.dictation.overlay_style = OverlayStyle::Minimal;
        settings.dictation.save_recordings = false;
        settings.dictation.save_transcripts = false;
        settings.dictation.post_process_enabled = false;
        settings.dictation.post_process_selected_prompt_id = Some("dictation-prompt".to_string());

        let result = apply_mode(settings, Mode::Meeting);

        assert!(!result.push_to_talk);
        assert_eq!(result.paste_method, PasteMethod::CtrlV);
        assert_eq!(
            result.clipboard_handling,
            ClipboardHandling::CopyToClipboard
        );
        assert!(result.auto_submit);
        assert_eq!(result.auto_submit_key, AutoSubmitKey::CtrlEnter);
        assert!(result.append_trailing_space);
        assert_eq!(result.typing_tool, TypingTool::Wtype);
        assert_eq!(result.overlay_style, OverlayStyle::Live);
        assert!(result.save_recordings);
        assert!(result.save_transcripts);
        assert!(result.post_process_enabled);
        assert_eq!(
            result.post_process_selected_prompt_id,
            Some("meeting-prompt".to_string())
        );
    }

    #[test]
    fn apply_mode_overrides_every_per_mode_field_for_dictation() {
        let mut settings = crate::settings::get_default_settings();
        settings.selected_model = "whisper-large-v3-turbo".to_string();
        settings.push_to_talk = false;
        settings.paste_method = PasteMethod::None;
        settings.clipboard_handling = ClipboardHandling::DontModify;
        settings.auto_submit = false;
        settings.auto_submit_key = AutoSubmitKey::Enter;
        settings.append_trailing_space = false;
        settings.typing_tool = TypingTool::Auto;
        settings.overlay_style = OverlayStyle::None;
        settings.save_recordings = false;
        settings.save_transcripts = false;
        settings.post_process_enabled = false;
        settings.post_process_selected_prompt_id = None;

        settings.dictation.push_to_talk = true;
        settings.dictation.paste_method = PasteMethod::CtrlV;
        settings.dictation.clipboard_handling = ClipboardHandling::CopyToClipboard;
        settings.dictation.auto_submit = true;
        settings.dictation.auto_submit_key = AutoSubmitKey::CmdEnter;
        settings.dictation.append_trailing_space = true;
        settings.dictation.typing_tool = TypingTool::Ydotool;
        settings.dictation.overlay_style = OverlayStyle::Minimal;
        settings.dictation.save_recordings = true;
        settings.dictation.save_transcripts = true;
        settings.dictation.post_process_enabled = true;
        settings.dictation.post_process_selected_prompt_id = Some("dictation-prompt".to_string());

        let result = apply_mode(settings, Mode::Dictation);

        assert!(result.push_to_talk);
        assert_eq!(result.paste_method, PasteMethod::CtrlV);
        assert_eq!(
            result.clipboard_handling,
            ClipboardHandling::CopyToClipboard
        );
        assert!(result.auto_submit);
        assert_eq!(result.auto_submit_key, AutoSubmitKey::CmdEnter);
        assert!(result.append_trailing_space);
        assert_eq!(result.typing_tool, TypingTool::Ydotool);
        assert_eq!(result.overlay_style, OverlayStyle::Minimal);
        assert!(result.save_recordings);
        assert!(result.save_transcripts);
        assert!(result.post_process_enabled);
        assert_eq!(
            result.post_process_selected_prompt_id,
            Some("dictation-prompt".to_string())
        );
        // A field `apply_mode` does not own must survive from the base settings.
        assert_eq!(result.selected_model, "whisper-large-v3-turbo");
    }
}
