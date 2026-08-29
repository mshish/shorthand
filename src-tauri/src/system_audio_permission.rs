//! Reads the macOS system-audio recording permission (`kTCCServiceAudioCapture`).
//!
//! macOS exposes Core Audio Process Taps without a public permission query, and
//! a tap without permission succeeds but delivers all-zero samples. This
//! private SPI is deliberately isolated here and fails open: if it is absent,
//! callers receive `NotGranted` and can still make the capture attempt that
//! raises macOS's consent dialog.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioPermission {
    Granted,
    NotGranted,
}

/// Maps TCC's private preflight result without guessing at undocumented
/// non-zero values.
fn from_preflight_status(status: i32) -> SystemAudioPermission {
    if status == 0 {
        SystemAudioPermission::Granted
    } else {
        SystemAudioPermission::NotGranted
    }
}

#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
use libloading::Library;
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
use objc2_core_foundation::CFString;
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
use std::{ffi::c_void, sync::OnceLock};

#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
const TCC_FRAMEWORK_PATH: &str = "/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
const SERVICE_AUDIO_CAPTURE: &str = "kTCCServiceAudioCapture";

/// `int TCCAccessPreflight(CFStringRef service, CFDictionaryRef options)`.
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
type PreflightFn = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;

#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
static TCC_LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
static TCC_ACCESS_PREFLIGHT: OnceLock<Option<PreflightFn>> = OnceLock::new();

#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
fn tcc_library() -> Option<&'static Library> {
    TCC_LIBRARY
        .get_or_init(|| {
            // SAFETY: The path is Apple's TCC framework. The library remains in
            // `TCC_LIBRARY` for the rest of the process, outliving its symbols.
            match unsafe { Library::new(TCC_FRAMEWORK_PATH) } {
                Ok(library) => Some(library),
                Err(error) => {
                    log::warn!("Failed to load TCC preflight SPI: {error}");
                    None
                }
            }
        })
        .as_ref()
}

#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
fn tcc_access_preflight() -> Option<PreflightFn> {
    *TCC_ACCESS_PREFLIGHT.get_or_init(|| {
        let library = tcc_library()?;
        // SAFETY: The symbol name is NUL-terminated and has TCC's documented
        // C ABI. The copied function pointer remains valid because the library
        // is retained in `TCC_LIBRARY` for the rest of the process.
        match unsafe { library.get::<PreflightFn>(b"TCCAccessPreflight\0") } {
            Ok(symbol) => Some(*symbol),
            Err(error) => {
                log::warn!("Failed to load TCCAccessPreflight: {error}");
                None
            }
        }
    })
}

/// Reads the current process-local permission state without displaying UI.
#[cfg(all(target_os = "macos", feature = "macos-tcc-spi"))]
pub fn preflight() -> SystemAudioPermission {
    let Some(preflight) = tcc_access_preflight() else {
        return SystemAudioPermission::NotGranted;
    };

    // `from_str` produces the +1 retain used for this call and releases it on
    // drop; no manual `CFRelease` is needed. It is infallible in
    // objc2-core-foundation 0.3.2.
    let service = CFString::from_str(SERVICE_AUDIO_CAPTURE);
    // SAFETY: `service` is valid for the duration of the call and TCC accepts
    // a null options dictionary.
    let status = unsafe { preflight(&*service as *const _ as *const c_void, std::ptr::null()) };
    from_preflight_status(status)
}

/// Feature-off builds deliberately omit all private-SPI loading.
#[cfg(not(all(target_os = "macos", feature = "macos-tcc-spi")))]
pub fn preflight() -> SystemAudioPermission {
    SystemAudioPermission::NotGranted
}

#[cfg(test)]
mod system_audio_permission_tests {
    use super::*;

    #[test]
    fn zero_is_granted() {
        assert_eq!(from_preflight_status(0), SystemAudioPermission::Granted);
    }

    #[test]
    fn every_other_status_is_not_granted() {
        // Only `0 == granted` is agreed across the reference implementations.
        // Some macOS builds return the same code for denied and not determined,
        // so no other value may be read as a denial.
        for status in [1, 2, 3, -1, i32::MAX, i32::MIN] {
            assert_eq!(
                from_preflight_status(status),
                SystemAudioPermission::NotGranted,
                "status {status} should be not-granted"
            );
        }
    }
}
