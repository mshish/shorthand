//! The fork-only "active mode" cell. `TranscribeAction::start` calls
//! `set_active` once per capture; every per-mode resolver in
//! `shorthand::dictation` reads it back via `active`. See "The active-mode
//! cell" in the design doc for why this is a process-wide cell rather than a
//! parameter threaded through `clipboard::paste`, `overlay::show_overlay_state`,
//! and `actions.rs`.

use std::sync::atomic::{AtomicU8, Ordering};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Meeting,
    Dictation,
    AssistedNotes,
}

impl Mode {
    /// The cell's stored representation. Written out rather than derived from
    /// a `#[repr(u8)]` cast, so reordering the variants can never silently
    /// reinterpret a value already sitting in the cell.
    const fn as_repr(self) -> u8 {
        match self {
            Mode::Meeting => 0,
            Mode::Dictation => 1,
            Mode::AssistedNotes => 2,
        }
    }

    /// Unknown values fall back to `Meeting`, for the same reason `active`
    /// does: meeting behaviour is what every code path did before this module
    /// existed.
    const fn from_repr(value: u8) -> Mode {
        match value {
            1 => Mode::Dictation,
            2 => Mode::AssistedNotes,
            _ => Mode::Meeting,
        }
    }
}

/// Only one capture runs at a time — `AudioRecordingManager` tracks a single
/// `is_recording` flag, and `TranscriptionCoordinator`'s `Stage` state machine
/// serialises transcribe bindings — so a single process-wide cell is safe:
/// there is never more than one capture this cell could ambiguously describe.
static ACTIVE_MODE: AtomicU8 = AtomicU8::new(Mode::Meeting.as_repr());

/// "dictate*" ids are dictation, "assisted_notes*" ids are assisted notes;
/// every other binding id (including ones this module doesn't know about) is
/// meeting.
pub fn mode_for_binding(binding_id: &str) -> Mode {
    match binding_id {
        "dictate" | "dictate_with_post_process" => Mode::Dictation,
        "assisted_notes" | "assisted_notes_with_post_process" => Mode::AssistedNotes,
        _ => Mode::Meeting,
    }
}

/// Records the mode of the capture that is starting. Called once, from
/// `TranscribeAction::start`. Never cleared: "the mode of the most recently
/// started capture" is always the right answer for work belonging to that
/// capture, including async work that outlives the recording itself. A
/// cleared cell would introduce a race an uncleared one does not have.
pub fn set_active(_app: &AppHandle, binding_id: &str) {
    ACTIVE_MODE.store(mode_for_binding(binding_id).as_repr(), Ordering::Release);
}

/// The mode of the most recently started capture. Defaults to `Meeting`, so
/// any code path reached before the first capture behaves exactly as it did
/// before this module existed.
pub fn active(_app: &AppHandle) -> Mode {
    Mode::from_repr(ACTIVE_MODE.load(Ordering::Acquire))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_for_binding_maps_each_modes_ids_and_defaults_everything_else_to_meeting() {
        assert_eq!(mode_for_binding("dictate"), Mode::Dictation);
        assert_eq!(
            mode_for_binding("dictate_with_post_process"),
            Mode::Dictation
        );
        assert_eq!(mode_for_binding("assisted_notes"), Mode::AssistedNotes);
        assert_eq!(
            mode_for_binding("assisted_notes_with_post_process"),
            Mode::AssistedNotes
        );
        assert_eq!(mode_for_binding("transcribe"), Mode::Meeting);
        assert_eq!(
            mode_for_binding("transcribe_with_post_process"),
            Mode::Meeting
        );
        assert_eq!(mode_for_binding("cancel"), Mode::Meeting);
        assert_eq!(mode_for_binding("unknown"), Mode::Meeting);
    }

    #[test]
    fn mode_repr_round_trips_every_variant() {
        for mode in [Mode::Meeting, Mode::Dictation, Mode::AssistedNotes] {
            assert_eq!(Mode::from_repr(mode.as_repr()), mode);
        }
        assert_eq!(Mode::from_repr(200), Mode::Meeting);
    }
}
