//! Fork-only tray items that start and stop a capture per mode.
//!
//! Idle, the tray offers one Start item per mode, labelled with that mode's
//! shortcut. While a capture runs it offers Stop (finish and save) next to
//! upstream's Cancel (discard). Every item goes through the coordinator's
//! explicit start/stop path, the one `--start-transcription` and
//! `--start-assisted-notes` use, so a click is decided against the
//! coordinator's own `Stage` and can never flip a capture the wrong way. See
//! `TranscriptionCoordinator::send_tray_capture` for the one way a tray click
//! differs from the CLI flag.
//!
//! `tray.rs` touches this module at three points: the snapshot it diffs
//! ([`TrayModes`]), and the two menu layouts ([`start_items`],
//! [`stop_item`]). `lib.rs` routes clicks to [`handle_menu_event`].

use crate::settings::{AppSettings, KeyboardImplementation};
use crate::shorthand::capture_command::ExplicitOp;
use crate::shorthand::mode::{self, Mode};
use crate::tray_i18n::TrayStrings;
use crate::TranscriptionCoordinator;
use log::warn;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Manager};

const START_PREFIX: &str = "mode_start:";
const STOP_ID: &str = "mode_stop";

/// The order the Start items appear in.
const MODES: [Mode; 3] = [Mode::Meeting, Mode::AssistedNotes, Mode::Dictation];

/// One Start item: whether it can be clicked, and the shortcut shown after
/// its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartItem {
    pub mode: Mode,
    pub enabled: bool,
    pub shortcut: Option<String>,
}

/// Everything the Start/Stop items depend on. Part of the tray's `MenuInputs`,
/// so a rebound shortcut or a mode switched on or off rebuilds the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayModes {
    pub items: Vec<StartItem>,
}

/// The binding a mode's Start item starts, and whose shortcut it shows.
fn binding_id(mode: Mode) -> &'static str {
    match mode {
        Mode::Meeting => "transcribe",
        Mode::AssistedNotes => "assisted_notes",
        Mode::Dictation => "dictate",
    }
}

fn wire_id(mode: Mode) -> &'static str {
    match mode {
        Mode::Meeting => "meeting",
        Mode::AssistedNotes => "assisted_notes",
        Mode::Dictation => "dictation",
    }
}

/// Whether the mode can start at all. Assisted notes and dictation each have
/// their own switch in Settings; with it off, their shortcut is not even
/// registered, so the item is shown greyed out rather than refused on click.
fn mode_enabled(settings: &AppSettings, mode: Mode) -> bool {
    match mode {
        Mode::Meeting => true,
        Mode::AssistedNotes => settings.assisted_notes.enabled,
        Mode::Dictation => settings.dictation.enabled,
    }
}

/// Reads the unresolved settings on purpose: which modes are on and what
/// their shortcuts are is shared state, not something the last capture's mode
/// changes.
pub fn compute(settings: &AppSettings) -> TrayModes {
    let shortcuts_active = settings.keyboard_implementation != KeyboardImplementation::None;
    let items = MODES
        .into_iter()
        .map(|mode| {
            let enabled = mode_enabled(settings, mode);
            let shortcut = (enabled && shortcuts_active)
                .then(|| settings.bindings.get(binding_id(mode)))
                .flatten()
                .map(|binding| format_binding(&binding.current_binding))
                .filter(|label| !label.is_empty());
            StartItem {
                mode,
                enabled,
                shortcut,
            }
        })
        .collect();
    TrayModes { items }
}

/// A stored binding (`ctrl+alt+n`, `option_left+space`) as a menu shows one:
/// `Ctrl+Alt+N`, `Left Option+Space`.
pub fn format_binding(binding: &str) -> String {
    binding
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(format_key)
        .collect::<Vec<_>>()
        .join("+")
}

fn format_key(part: &str) -> String {
    if let Some(name) = part.strip_suffix("_left") {
        return format!("Left {}", format_key(name));
    }
    if let Some(name) = part.strip_suffix("_right") {
        return format!("Right {}", format_key(name));
    }
    match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => "Ctrl".to_string(),
        "cmd" | "command" => "Cmd".to_string(),
        "esc" | "escape" => "Esc".to_string(),
        "pageup" => "PgUp".to_string(),
        "pagedown" => "PgDn".to_string(),
        _ => {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

fn label(text: &str, shortcut: Option<&str>) -> String {
    match shortcut {
        Some(shortcut) => format!("{text} ({shortcut})"),
        None => text.to_string(),
    }
}

/// The idle menu's Start items, in [`MODES`] order.
pub fn start_items(
    app: &AppHandle,
    strings: &TrayStrings,
    modes: &TrayModes,
) -> tauri::Result<Vec<MenuItem<tauri::Wry>>> {
    modes
        .items
        .iter()
        .map(|item| {
            let text = match item.mode {
                Mode::Meeting => &strings.start_meeting,
                Mode::AssistedNotes => &strings.start_assisted_notes,
                Mode::Dictation => &strings.start_dictation,
            };
            MenuItem::with_id(
                app,
                format!("{START_PREFIX}{}", wire_id(item.mode)),
                label(text, item.shortcut.as_deref()),
                item.enabled,
                None::<&str>,
            )
        })
        .collect()
}

/// The busy menu's Stop item, shown above upstream's Cancel.
pub fn stop_item(app: &AppHandle, strings: &TrayStrings) -> tauri::Result<MenuItem<tauri::Wry>> {
    MenuItem::with_id(app, STOP_ID, &strings.stop, true, None::<&str>)
}

fn mode_for_wire_id(id: &str) -> Option<Mode> {
    MODES.into_iter().find(|mode| wire_id(*mode) == id)
}

/// Handles a click on one of this module's items. Returns `false` for any
/// other id, so the caller's own match handles it.
pub fn handle_menu_event(app: &AppHandle, id: &str) -> bool {
    let (op, mode) = if let Some(rest) = id.strip_prefix(START_PREFIX) {
        let Some(mode) = mode_for_wire_id(rest) else {
            return false;
        };
        (ExplicitOp::Start, mode)
    } else if id == STOP_ID {
        // A stop names the mode it stops; the capture running is the most
        // recently started one. If that has already ended, the coordinator
        // answers with a no-op.
        (ExplicitOp::Stop, mode::active(app))
    } else {
        return false;
    };
    match app.try_state::<TranscriptionCoordinator>() {
        Some(coordinator) => coordinator.send_tray_capture(op, binding_id(mode)),
        None => warn!("TranscriptionCoordinator not initialized"),
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::get_default_settings;

    #[test]
    fn bindings_format_the_way_a_menu_shows_them() {
        assert_eq!(format_binding("ctrl+alt+m"), "Ctrl+Alt+M");
        assert_eq!(format_binding("option_left+space"), "Left Option+Space");
        assert_eq!(format_binding("ctrl+shift+space"), "Ctrl+Shift+Space");
        assert_eq!(format_binding("command+f13"), "Cmd+F13");
        assert_eq!(format_binding(""), "");
    }

    #[test]
    fn labels_omit_the_suffix_without_a_shortcut() {
        assert_eq!(
            label("Start meeting", Some("Ctrl+Alt+M")),
            "Start meeting (Ctrl+Alt+M)"
        );
        assert_eq!(label("Start meeting", None), "Start meeting");
    }

    #[test]
    fn a_mode_switched_off_is_greyed_out_and_shows_no_shortcut() {
        let mut settings = get_default_settings();
        settings.assisted_notes.enabled = false;
        settings.dictation.enabled = true;
        let modes = compute(&settings);
        let item = |mode| modes.items.iter().find(|i| i.mode == mode).unwrap();

        assert!(item(Mode::Meeting).enabled);
        assert!(item(Mode::Meeting).shortcut.is_some());
        assert!(!item(Mode::AssistedNotes).enabled);
        assert_eq!(item(Mode::AssistedNotes).shortcut, None);
        assert!(item(Mode::Dictation).enabled);
        assert!(item(Mode::Dictation).shortcut.is_some());
    }

    #[test]
    fn rebinding_a_shortcut_changes_the_snapshot() {
        let mut settings = get_default_settings();
        let before = compute(&settings);
        settings
            .bindings
            .get_mut("transcribe")
            .unwrap()
            .current_binding = "ctrl+alt+m".to_string();
        let after = compute(&settings);
        assert_ne!(before, after);
        assert_eq!(after.items[0].shortcut.as_deref(), Some("Ctrl+Alt+M"));
    }

    #[test]
    fn no_shortcuts_are_shown_when_built_in_shortcuts_are_off() {
        let mut settings = get_default_settings();
        settings.keyboard_implementation = KeyboardImplementation::None;
        assert!(compute(&settings)
            .items
            .iter()
            .all(|i| i.shortcut.is_none()));
    }

    #[test]
    fn menu_ids_round_trip_to_their_mode() {
        for mode in MODES {
            assert_eq!(mode_for_wire_id(wire_id(mode)), Some(mode));
        }
        assert_eq!(mode_for_wire_id("nope"), None);
    }
}
