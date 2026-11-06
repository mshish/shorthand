//! Sizes the recording overlay window to the card it draws.
//!
//! The overlay is one transparent webview window, and a transparent area of a
//! window still takes the clicks meant for whatever is beneath it. Fixed window
//! sizes (see `overlay_dimensions`) must hold the largest card a state can
//! draw, so most states left an invisible strip around a smaller card. Instead
//! the overlay measures its card with a `ResizeObserver` and reports the size
//! through [`fit_overlay`]; the window then grows and shrinks with the card,
//! keeping its screen-edge side and horizontal centre where `show` put them.
//!
//! The fixed sizes remain as the window's first size on each show, big enough
//! that nothing clips before the webview has rendered and reported.

#[cfg(any(target_os = "windows", target_os = "linux"))]
use crate::overlay;
use crate::settings::OverlayPosition;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "linux")]
use crate::settings;

/// Upper bounds on a reported card size, in CSS pixels. The widest real card is
/// the Live panel at about 394, the tallest the two-lane panel at about 158;
/// anything past these is a layout fault, not content.
const MAX_FIT_WIDTH: f64 = 1000.0;
const MAX_FIT_HEIGHT: f64 = 600.0;

/// Rounds a reported size up to whole CSS pixels (so a fractional card is never
/// clipped by a truncated window) and refuses sizes that can't be a card.
fn clamp_fit_size(width: f64, height: f64) -> Option<(f64, f64)> {
    if !width.is_finite() || !height.is_finite() || width < 1.0 || height < 1.0 {
        return None;
    }
    Some((
        width.ceil().min(MAX_FIT_WIDTH),
        height.ceil().min(MAX_FIT_HEIGHT),
    ))
}

/// Where the card sits on screen, captured when the overlay is shown or moved:
/// the horizontal centre and the screen-edge side. Windows and macOS hold it in
/// the window's own units (physical pixels on Windows, points on macOS) so a
/// resize never goes back through the monitor under the cursor, which would
/// move the overlay to another display mid-meeting. Linux has no anchor: the
/// layer shell anchors itself, and the fallback window re-places itself.
#[cfg_attr(target_os = "linux", allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    center_x: f64,
    /// The top edge for a top overlay, the bottom edge for a bottom one.
    edge: f64,
    position: OverlayPosition,
}

/// Top-left of a `width` x `height` window held at `anchor`: the anchored edge
/// and the centre stay put, so the window grows away from the screen edge.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn origin_for(anchor: Anchor, width: f64, height: f64) -> (f64, f64) {
    let x = anchor.center_x - width / 2.0;
    let y = match anchor.position {
        OverlayPosition::Top => anchor.edge,
        OverlayPosition::Bottom => anchor.edge - height,
    };
    (x, y)
}

struct FitState {
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    anchor: Option<Anchor>,
    /// The size last requested (CSS pixels), so an unchanged report is dropped
    /// and a position change can re-apply it. Cleared on every show, because
    /// show resets the window to its fixed first size.
    wanted: Option<(f64, f64)>,
}

static STATE: Mutex<FitState> = Mutex::new(FitState {
    anchor: None,
    wanted: None,
});

fn state() -> std::sync::MutexGuard<'static, FitState> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The overlay reports its card's border-box size in CSS pixels. Window changes
/// run on the main thread, as in `overlay.rs`.
#[tauri::command]
#[specta::specta]
pub fn fit_overlay(app: AppHandle, width: f64, height: f64) {
    let Some(size) = clamp_fit_size(width, height) else {
        return;
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || fit_on_main(&handle, size));
}

fn fit_on_main(app: &AppHandle, size: (f64, f64)) {
    let Some(window) = app.get_webview_window("recording_overlay") else {
        return;
    };
    // A report that lands after the hide must not move a hidden window.
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    {
        let mut state = state();
        if state.wanted == Some(size) {
            return;
        }
        state.wanted = Some(size);
    }
    apply(app, &window, size);
}

/// Called by `overlay.rs` after it places and shows the window at its fixed
/// first size.
pub fn on_shown(window: &tauri::webview::WebviewWindow) {
    let mut state = state();
    state.wanted = None;
    state.anchor = capture_anchor(window);
}

/// Called by `overlay.rs` after the overlay moves to a new position setting.
pub fn on_position_changed(window: &tauri::webview::WebviewWindow) {
    let wanted = {
        let mut state = state();
        state.anchor = capture_anchor(window);
        state.wanted
    };
    if let Some(size) = wanted {
        apply(window.app_handle(), window, size);
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_anchor(window: &tauri::webview::WebviewWindow) -> Option<Anchor> {
    let position = crate::settings::get_settings(window.app_handle()).overlay_position;
    let origin = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let (x, y, width, height) = (
        origin.x as f64,
        origin.y as f64,
        size.width as f64,
        size.height as f64,
    );
    // macOS holds logical points; Windows keeps physical pixels.
    #[cfg(target_os = "macos")]
    let (x, y, width, height) = {
        let scale = window.scale_factor().ok()?;
        (x / scale, y / scale, width / scale, height / scale)
    };
    Some(Anchor {
        center_x: x + width / 2.0,
        edge: match position {
            OverlayPosition::Top => y,
            OverlayPosition::Bottom => y + height,
        },
        position,
    })
}

#[cfg(target_os = "linux")]
fn capture_anchor(_window: &tauri::webview::WebviewWindow) -> Option<Anchor> {
    None
}

/// Windows: one `SetWindowPos` moves and sizes together, so the window never
/// shows a size from one step with the position of another. The size scales by
/// the display scale and the accessibility text scale, as the fixed sizes do.
#[cfg(target_os = "windows")]
fn apply(_app: &AppHandle, window: &tauri::webview::WebviewWindow, size: (f64, f64)) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

    let Some(anchor) = state().anchor else {
        return;
    };
    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let content_scale = scale * overlay::windows_text_scale_factor();
    // Round up: a card of 42.2 CSS px must not meet a window truncated short of it.
    let width = (size.0 * content_scale).ceil().max(1.0);
    let height = (size.1 * content_scale).ceil().max(1.0);
    let (x, y) = origin_for(anchor, width, height);
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    unsafe {
        if let Err(error) = SetWindowPos(
            hwnd,
            None,
            x.round() as i32,
            y.round() as i32,
            width as i32,
            height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        ) {
            log::error!("Failed to fit recording overlay: {error}");
        }
    }
}

/// macOS: the NSPanel takes the same logical size and position calls the show
/// path uses.
#[cfg(target_os = "macos")]
fn apply(_app: &AppHandle, window: &tauri::webview::WebviewWindow, size: (f64, f64)) {
    let Some(anchor) = state().anchor else {
        return;
    };
    let (x, y) = origin_for(anchor, size.0, size.1);
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: size.0,
        height: size.1,
    }));
    let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
}

/// Linux: a layer surface takes its size from GTK's size request and its
/// placement from its anchor and margin, so the compositor grows it away from
/// the screen edge. Without the layer shell the window is placed like the show
/// path places it.
#[cfg(target_os = "linux")]
fn apply(app: &AppHandle, window: &tauri::webview::WebviewWindow, size: (f64, f64)) {
    if overlay::LAYER_SHELL_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
        match window.gtk_window() {
            Ok(gtk_window) => overlay::configure_layer_shell_surface(
                &gtk_window,
                settings::get_settings(app).overlay_position,
                size.0,
                size.1,
            ),
            Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
        }
        return;
    }
    let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
        width: size.0,
        height: size.1,
    }));
    if let Some((x, y)) = overlay::calculate_overlay_position(app, size.0, size.1) {
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fractional_card_rounds_up_so_it_is_not_clipped() {
        assert_eq!(clamp_fit_size(172.0, 42.2), Some((172.0, 43.0)));
    }

    #[test]
    fn sizes_that_cannot_be_a_card_are_refused() {
        assert_eq!(clamp_fit_size(f64::NAN, 40.0), None);
        assert_eq!(clamp_fit_size(40.0, f64::INFINITY), None);
        assert_eq!(clamp_fit_size(0.0, 40.0), None);
        assert_eq!(clamp_fit_size(40.0, -3.0), None);
    }

    #[test]
    fn a_runaway_size_is_capped() {
        assert_eq!(
            clamp_fit_size(5000.0, 5000.0),
            Some((MAX_FIT_WIDTH, MAX_FIT_HEIGHT))
        );
    }

    #[test]
    fn a_top_overlay_grows_downward_from_its_edge() {
        let anchor = Anchor {
            center_x: 1000.0,
            edge: 4.0,
            position: OverlayPosition::Top,
        };
        assert_eq!(origin_for(anchor, 184.0, 42.0), (908.0, 4.0));
        // Taller and wider: the top edge and the centre are unchanged.
        assert_eq!(origin_for(anchor, 394.0, 158.0), (803.0, 4.0));
    }

    #[test]
    fn a_bottom_overlay_grows_upward_from_its_edge() {
        let anchor = Anchor {
            center_x: 1000.0,
            edge: 1040.0,
            position: OverlayPosition::Bottom,
        };
        let (_, short_y) = origin_for(anchor, 184.0, 42.0);
        let (x, tall_y) = origin_for(anchor, 394.0, 158.0);
        assert_eq!((short_y + 42.0, tall_y + 158.0), (1040.0, 1040.0));
        assert_eq!(x, 803.0);
        assert!(tall_y < short_y);
    }
}
