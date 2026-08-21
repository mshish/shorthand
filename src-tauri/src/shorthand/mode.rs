//! The fork-only "active mode" cell. `TranscribeAction::start` calls
//! `set_active` once per capture; every per-mode resolver in
//! `shorthand::dictation` reads it back via `active`. See "The active-mode
//! cell" in the design doc for why this is a process-wide cell rather than a
//! parameter threaded through `clipboard::paste`, `overlay::show_overlay_state`,
//! and `actions.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Meeting,
    Dictation,
}

/// Only one capture runs at a time — `AudioRecordingManager` tracks a single
/// `is_recording` flag, and `TranscriptionCoordinator`'s `Stage` state machine
/// serialises transcribe bindings — so a single process-wide flag is safe:
/// there is never more than one capture this cell could ambiguously describe.
static ACTIVE_MODE_IS_DICTATION: AtomicBool = AtomicBool::new(false);

/// "dictate" and "dictate_with_post_process" are dictation; every other
/// binding id (including ones this module doesn't know about) is meeting.
pub fn mode_for_binding(binding_id: &str) -> Mode {
    match binding_id {
        "dictate" | "dictate_with_post_process" => Mode::Dictation,
        _ => Mode::Meeting,
    }
}

/// Records the mode of the capture that is starting. Called once, from
/// `TranscribeAction::start`. Never cleared: "the mode of the most recently
/// started capture" is always the right answer for work belonging to that
/// capture, including async work that outlives the recording itself. A
/// cleared cell would introduce a race an uncleared one does not have.
pub fn set_active(_app: &AppHandle, binding_id: &str) {
    let is_dictation = mode_for_binding(binding_id) == Mode::Dictation;
    ACTIVE_MODE_IS_DICTATION.store(is_dictation, Ordering::Release);
}

/// The mode of the most recently started capture. Defaults to `Meeting`, so
/// any code path reached before the first capture behaves exactly as it did
/// before this module existed.
pub fn active(_app: &AppHandle) -> Mode {
    if ACTIVE_MODE_IS_DICTATION.load(Ordering::Acquire) {
        Mode::Dictation
    } else {
        Mode::Meeting
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_for_binding_maps_dictation_ids_and_defaults_everything_else_to_meeting() {
        assert_eq!(mode_for_binding("dictate"), Mode::Dictation);
        assert_eq!(
            mode_for_binding("dictate_with_post_process"),
            Mode::Dictation
        );
        assert_eq!(mode_for_binding("transcribe"), Mode::Meeting);
        assert_eq!(
            mode_for_binding("transcribe_with_post_process"),
            Mode::Meeting
        );
        assert_eq!(mode_for_binding("cancel"), Mode::Meeting);
        assert_eq!(mode_for_binding("unknown"), Mode::Meeting);
    }
}
