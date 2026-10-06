//! A crash marker for native calls that can abort the process without a Rust
//! panic (e.g. vulkan-1.dll's fail-fast on 2026-10-06). `begin` writes it
//! before the call and `end` removes it after; a marker still present at the
//! next launch means the process died inside that call. Holds only fixed
//! codes, so reporting it keeps TELEMETRY.md's promise.

use std::path::{Path, PathBuf};

const FILE: &str = "native-call.marker";

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

pub fn begin(dir: &Path, stage: &'static str, accel: &'static str) {
    let _ = std::fs::create_dir_all(dir);
    if let Err(e) = std::fs::write(path(dir), format!("{stage}:{accel}")) {
        log::warn!("Could not write native-call marker: {e}");
    }
}

pub fn end(dir: &Path) {
    let _ = std::fs::remove_file(path(dir));
}

/// Returns and clears a marker left by a previous run. Only codes this module
/// wrote are returned, so a hand-edited file cannot inject text into a report.
pub fn take_previous(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path(dir)).ok()?;
    end(dir);
    let valid = text
        .split_once(':')
        .is_some_and(|(s, a)| STAGES.contains(&s) && ACCELS.contains(&a));
    valid.then_some(text)
}

pub const STAGES: &[&str] = &["model_load"];
pub const ACCELS: &[&str] = &["auto", "cpu", "pinned_device"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_left_behind_is_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        begin(dir.path(), "model_load", "pinned_device");
        assert_eq!(
            take_previous(dir.path()).as_deref(),
            Some("model_load:pinned_device")
        );
        assert_eq!(take_previous(dir.path()), None);
    }

    #[test]
    fn a_completed_call_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        begin(dir.path(), "model_load", "cpu");
        end(dir.path());
        assert_eq!(take_previous(dir.path()), None);
    }

    #[test]
    fn unknown_text_is_cleared_but_never_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "C:\\Users\\someone:x").unwrap();
        assert_eq!(take_previous(dir.path()), None);
        assert!(!dir.path().join(FILE).exists());
    }
}
