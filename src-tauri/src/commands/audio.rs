use crate::audio_feedback;
use crate::audio_toolkit::audio::{
    list_input_devices, list_output_devices, list_system_audio_devices, AudioRecorder,
};
use crate::managers::audio::{AudioRecordingManager, MicrophoneMode};
use crate::managers::model::ModelManager;
use crate::managers::transcription::{
    StreamSource, SystemAudioTranscription, TranscriptionManager,
};
use crate::settings::{get_settings, write_settings, AppSettings};
use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    RegKey, HKEY,
};

#[derive(Serialize, Type)]
pub struct CustomSounds {
    start: bool,
    stop: bool,
}

fn custom_sound_exists(app: &AppHandle, sound_type: &str) -> bool {
    crate::portable::resolve_app_data(app, &format!("custom_{}.wav", sound_type))
        .is_ok_and(|path| path.exists())
}

#[tauri::command]
#[specta::specta]
pub fn check_custom_sounds(app: AppHandle) -> CustomSounds {
    CustomSounds {
        start: custom_sound_exists(&app, "start"),
        stop: custom_sound_exists(&app, "stop"),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AudioDevice {
    pub index: String,
    pub name: String,
    pub is_default: bool,
}

/// A capture-capable system-audio endpoint. Unlike `AudioDevice`, `id` is an
/// opaque persisted value: Linux uses CPAL's stable DeviceId serialization,
/// while legacy Windows settings retain their existing display-name values.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SystemAudioDevice {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAccess {
    Allowed,
    Denied,
    Unknown,
}

/// Whether this machine can expose a system-audio loopback device at all.
///
/// This is deliberately separate from whether a particular capture attempt
/// succeeded. A reachable host is enough to render the controls; the recorder
/// continues to report its actual loopback-open outcome independently.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SystemAudioAvailability {
    Available,
    UnavailableNoSoundServer,
    PermissionDenied,
}

/// Pure host-probe decision logic, kept separate from CPAL construction so it
/// is testable without a running sound server.
fn availability_from_host_probe(loopback_host_reachable: bool) -> SystemAudioAvailability {
    if loopback_host_reachable {
        SystemAudioAvailability::Available
    } else {
        SystemAudioAvailability::UnavailableNoSoundServer
    }
}

/// The two mode-owned controls deliberately use separate commands. Generic
/// Dictation settings changes remain pure persistence, while this narrow path
/// can safely coordinate the shared recorder and its system transcription
/// lane.
#[derive(Clone, Copy)]
enum SystemAudioSettingScope {
    Meetings,
    Dictation,
}

fn set_system_audio_enabled_for_scope(
    settings: &mut AppSettings,
    scope: SystemAudioSettingScope,
    enabled: bool,
) {
    match scope {
        SystemAudioSettingScope::Meetings => settings.system_audio_enabled = enabled,
        SystemAudioSettingScope::Dictation => settings.dictation.system_audio_enabled = enabled,
    }
}

/// Whether the shared system-audio lane has to exist at all.
///
/// Meetings and Dictation each own an enabled flag over one physical lane, so
/// it is needed as soon as either scope wants it. Dictation's flag only counts
/// while Dictation itself is enabled, because `dictation::apply_mode` falls
/// back to the Meetings values otherwise.
pub fn system_audio_lane_required(settings: &AppSettings) -> bool {
    settings.system_audio_enabled
        || (settings.dictation.enabled && settings.dictation.system_audio_enabled)
}

/// Releases the system-audio lane's transcription manager and the model it is
/// holding. Without this a disable leaves a second model resident for the rest
/// of the session.
fn release_system_audio_transcription(app: &AppHandle) {
    let previous = app
        .state::<SystemAudioTranscription>()
        .0
        .lock()
        .unwrap()
        .take();
    if let Some(manager) = previous {
        if let Err(error) = manager.unload_model() {
            warn!("Failed to unload the system audio model: {error}");
        }
    }
}

fn ensure_system_audio_transcription(app: &AppHandle) -> Result<Arc<TranscriptionManager>, String> {
    let system_state = app.state::<SystemAudioTranscription>();
    let mut slot = system_state.0.lock().unwrap();
    if let Some(manager) = slot.as_ref() {
        return Ok(Arc::clone(manager));
    }

    let manager = Arc::new(
        TranscriptionManager::new(
            app,
            app.state::<Arc<ModelManager>>().inner().clone(),
            StreamSource::System,
        )
        .map_err(|error| format!("Failed to initialize system audio transcription: {error}"))?,
    );
    *slot = Some(Arc::clone(&manager));
    Ok(manager)
}

/// Constructing a PulseAudio host can wait on its server socket, so keep the
/// CPAL probe off Tauri's webview/main run loop.
#[tauri::command]
#[specta::specta]
pub async fn get_system_audio_availability(app: AppHandle) -> SystemAudioAvailability {
    // Plan C reads the macOS process-local permission observation from this
    // handle after its native TCC spike establishes a reliable classifier.
    let _ = &app;
    tokio::task::spawn_blocking(|| {
        availability_from_host_probe(crate::audio_toolkit::get_system_audio_host().is_some())
    })
    .await
    .unwrap_or(SystemAudioAvailability::UnavailableNoSoundServer)
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_system_audio_devices() -> Result<Vec<SystemAudioDevice>, String> {
    // Both the PulseAudio handshake and PipeWire registry discovery can block;
    // keep enumeration off Tauri's webview/main run loop.
    tokio::task::spawn_blocking(|| {
        list_system_audio_devices()
            .map(|devices| {
                devices
                    .into_iter()
                    .map(|device| SystemAudioDevice {
                        id: device.id,
                        label: device.label,
                        is_default: device.is_default,
                    })
                    .collect()
            })
            .map_err(|error| format!("Failed to list system-audio devices: {error}"))
    })
    .await
    .map_err(|error| format!("system-audio task join failed: {error}"))?
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct WindowsMicrophonePermissionStatus {
    pub supported: bool,
    pub overall_access: PermissionAccess,
    pub device_access: PermissionAccess,
    pub app_access: PermissionAccess,
    pub desktop_app_access: PermissionAccess,
}

#[cfg(target_os = "windows")]
fn read_registry_permission_access(root_hkey: HKEY, path: &str) -> PermissionAccess {
    let root = RegKey::predef(root_hkey);
    let Ok(key) = root.open_subkey(path) else {
        return PermissionAccess::Unknown;
    };

    let Ok(value) = key.get_value::<String, _>("Value") else {
        return PermissionAccess::Unknown;
    };

    match value.to_ascii_lowercase().as_str() {
        "allow" => PermissionAccess::Allowed,
        "deny" => PermissionAccess::Denied,
        _ => PermissionAccess::Unknown,
    }
}

#[cfg(target_os = "windows")]
fn get_windows_microphone_permission_status_impl() -> WindowsMicrophonePermissionStatus {
    const MICROPHONE_PATH: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";
    const DESKTOP_APPS_PATH: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged";

    let device_access = read_registry_permission_access(HKEY_LOCAL_MACHINE, MICROPHONE_PATH);
    let app_access = read_registry_permission_access(HKEY_CURRENT_USER, MICROPHONE_PATH);
    let desktop_app_access = read_registry_permission_access(HKEY_CURRENT_USER, DESKTOP_APPS_PATH);

    // Handy is a desktop app, so the NonPackaged key (desktop_app_access) is
    // the relevant permission scope. The UWP master key (app_access) can be
    // "deny" on systems with debloaters (e.g. O&O ShutUp10) without actually
    // blocking desktop app microphone access.
    // Either the machine-wide device policy or the desktop-app-specific policy
    // can definitively deny Handy access; the UWP app policy is only a fallback
    // when the desktop-app policy is unknown.
    let overall_access = if device_access == PermissionAccess::Denied
        || desktop_app_access == PermissionAccess::Denied
    {
        PermissionAccess::Denied
    } else if desktop_app_access == PermissionAccess::Allowed {
        PermissionAccess::Allowed
    } else if app_access == PermissionAccess::Denied {
        PermissionAccess::Denied
    } else if device_access == PermissionAccess::Allowed && app_access == PermissionAccess::Allowed
    {
        PermissionAccess::Allowed
    } else {
        PermissionAccess::Unknown
    };

    WindowsMicrophonePermissionStatus {
        supported: true,
        overall_access,
        device_access,
        app_access,
        desktop_app_access,
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_windows_microphone_permission_status() -> WindowsMicrophonePermissionStatus {
    #[cfg(target_os = "windows")]
    {
        get_windows_microphone_permission_status_impl()
    }

    #[cfg(not(target_os = "windows"))]
    {
        WindowsMicrophonePermissionStatus {
            supported: false,
            overall_access: PermissionAccess::Unknown,
            device_access: PermissionAccess::Unknown,
            app_access: PermissionAccess::Unknown,
            desktop_app_access: PermissionAccess::Unknown,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn open_microphone_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .spawn()
            .map_err(|e| format!("Failed to open Windows microphone privacy settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Opening microphone privacy settings is only supported on Windows".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn update_microphone_mode(app: AppHandle, always_on: bool) -> Result<(), String> {
    // Update settings (fast, stays inline)
    let mut settings = get_settings(&app);
    settings.always_on_microphone = always_on;
    write_settings(&app, settings);

    // Update the audio manager mode. update_mode can stop/start the cpal stream
    // (blocking CoreAudio) and takes the manager std mutexes — run it on a
    // blocking thread, NOT inline on the webview/main run loop (a slow device
    // open/close would freeze the UI).
    let rm = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    let new_mode = if always_on {
        MicrophoneMode::AlwaysOn
    } else {
        MicrophoneMode::OnDemand
    };

    tokio::task::spawn_blocking(move || rm.update_mode(new_mode))
        .await
        .map_err(|e| format!("audio task join failed: {}", e))?
        .map_err(|e| format!("Failed to update microphone mode: {}", e))
}

#[tauri::command]
#[specta::specta]
pub fn get_microphone_mode(app: AppHandle) -> Result<bool, String> {
    let settings = get_settings(&app);
    Ok(settings.always_on_microphone)
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_microphones() -> Result<Vec<AudioDevice>, String> {
    // cpal device enumeration can stall — run it off the webview/main run loop.
    tokio::task::spawn_blocking(|| {
        let devices =
            list_input_devices().map_err(|e| format!("Failed to list audio devices: {}", e))?;

        let mut result = vec![AudioDevice {
            index: "default".to_string(),
            name: "Default".to_string(),
            is_default: true,
        }];

        result.extend(devices.into_iter().map(|d| AudioDevice {
            index: d.index,
            name: d.name,
            is_default: false, // The explicit default is handled separately
        }));

        Ok::<_, String>(result)
    })
    .await
    .map_err(|e| format!("audio task join failed: {}", e))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_selected_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);

    // Update the audio manager to use the new device. update_selected_device
    // can restart the cpal stream (blocking CoreAudio) — run it on a blocking
    // thread, not inline on the webview/main run loop.
    let rm = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    tokio::task::spawn_blocking(move || rm.update_selected_device())
        .await
        .map_err(|e| format!("audio task join failed: {}", e))?
        .map_err(|e| format!("Failed to update selected device: {}", e))
}

#[tauri::command]
#[specta::specta]
pub fn get_selected_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_output_devices() -> Result<Vec<AudioDevice>, String> {
    // cpal device enumeration can stall — run it off the webview/main run loop.
    tokio::task::spawn_blocking(|| {
        let devices =
            list_output_devices().map_err(|e| format!("Failed to list output devices: {}", e))?;

        let mut result = vec![AudioDevice {
            index: "default".to_string(),
            name: "Default".to_string(),
            is_default: true,
        }];

        result.extend(devices.into_iter().map(|d| AudioDevice {
            index: d.index,
            name: d.name,
            is_default: false, // The explicit default is handled separately
        }));

        Ok::<_, String>(result)
    })
    .await
    .map_err(|e| format!("audio task join failed: {}", e))?
}

#[tauri::command]
#[specta::specta]
pub fn set_selected_output_device(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_output_device = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_selected_output_device(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .selected_output_device
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn play_test_sound(app: AppHandle, sound_type: String) {
    let sound = match sound_type.as_str() {
        "start" => audio_feedback::SoundType::Start,
        "stop" => audio_feedback::SoundType::Stop,
        _ => {
            warn!("Unknown sound type: {}", sound_type);
            return;
        }
    };
    audio_feedback::play_test_sound(&app, sound);
}

#[tauri::command]
#[specta::specta]
pub fn set_clamshell_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.clamshell_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_clamshell_microphone(app: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app);
    Ok(settings
        .clamshell_microphone
        .unwrap_or_else(|| "default".to_string()))
}

#[tauri::command]
#[specta::specta]
pub fn is_recording(app: AppHandle) -> bool {
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    audio_manager.is_recording()
}

#[tauri::command]
#[specta::specta]
pub async fn get_microphone_channels(device_name: String) -> Result<u16, String> {
    // cpal device enumeration and config queries can stall, so keep them off
    // the webview/main run loop.
    tokio::task::spawn_blocking(move || {
        use cpal::traits::HostTrait;

        let device = if device_name.eq_ignore_ascii_case("default") {
            crate::audio_toolkit::get_cpal_host().default_input_device()
        } else {
            list_input_devices()
                .map_err(|e| format!("Failed to list audio devices: {e}"))?
                .into_iter()
                .find(|device| device.name == device_name)
                .map(|device| device.device)
        };

        match device {
            Some(device) => AudioRecorder::preferred_input_channel_count(&device)
                .map_err(|e| format!("Failed to get microphone config: {e}")),
            None => Ok(1),
        }
    })
    .await
    .map_err(|e| format!("audio task join failed: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_selected_channel(app: AppHandle, channel: Option<u16>) -> Result<(), String> {
    // Restarting cpal can block, so keep it off the webview/main run loop. Apply
    // the runtime change before persisting it so a rejected active-recording
    // change does not become effective on the next launch.
    let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    tokio::task::spawn_blocking(move || manager.update_selected_channel(channel))
        .await
        .map_err(|e| format!("audio task join failed: {e}"))?
        .map_err(|e| format!("Failed to update channel selection: {e}"))?;

    let mut settings = get_settings(&app);
    settings.selected_channel = channel;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn change_system_audio_enabled_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    change_system_audio_enabled_for_scope(app, SystemAudioSettingScope::Meetings, enabled).await
}

/// Change Dictation's mode-owned system-audio setting while keeping the
/// physical capture lane aligned with whichever mode is active. This must not
/// be folded into `change_dictation_settings`: that general transaction is
/// intentionally side-effect free and must never trigger system privacy
/// prompts as a by-product of saving unrelated Dictation preferences.
#[tauri::command]
#[specta::specta]
pub async fn change_dictation_system_audio_enabled_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    change_system_audio_enabled_for_scope(app, SystemAudioSettingScope::Dictation, enabled).await
}

async fn change_system_audio_enabled_for_scope(
    app: AppHandle,
    scope: SystemAudioSettingScope,
    enabled: bool,
) -> Result<(), String> {
    let settings = get_settings(&app);
    if enabled
        && get_system_audio_availability(app.clone()).await != SystemAudioAvailability::Available
    {
        return Err("System audio capture is not available on this system".to_string());
    }
    if enabled && settings.mute_while_recording {
        return Err(
            "System audio capture cannot be enabled while mute while recording is enabled"
                .to_string(),
        );
    }
    if enabled {
        let supports_streaming = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model)
            .is_some_and(|model| model.supports_streaming);
        if !supports_streaming {
            return Err(
                "System audio capture requires a model that supports streaming".to_string(),
            );
        }
    }

    // Evaluate the candidate against the active mode before changing runtime
    // capture. This keeps a Dictation preference change from restarting a
    // currently active Meeting stream (and the inverse), while still applying
    // a change immediately when that mode owns the open stream.
    let mut candidate_settings = settings;
    set_system_audio_enabled_for_scope(&mut candidate_settings, scope, enabled);
    let resolved = crate::shorthand::dictation::apply_mode(
        candidate_settings.clone(),
        crate::shorthand::mode::active(&app),
    );
    let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    let lane_required = system_audio_lane_required(&candidate_settings);
    let stream_router = if lane_required {
        Some(ensure_system_audio_transcription(&app)?.stream_router())
    } else {
        None
    };
    let manager_for_update = Arc::clone(&manager);
    tokio::task::spawn_blocking(move || {
        manager_for_update.apply_system_audio_configuration(
            resolved.system_audio_enabled,
            resolved.system_audio_device,
            stream_router,
        )
    })
    .await
    .map_err(|error| format!("audio task join failed: {error}"))?
    .map_err(|error| format!("Failed to update system audio capture: {error}"))?;

    if !lane_required {
        release_system_audio_transcription(&app);
    }

    // Re-read immediately before writing rather than persisting the snapshot
    // taken at entry. The availability probe and the capture reconfiguration
    // above both await, and the frontend can save an unrelated setting (model
    // choice, a shortcut, the paste method) while they are in flight; writing
    // the stale snapshot back would silently revert it.
    let mut settings = get_settings(&app);
    set_system_audio_enabled_for_scope(&mut settings, scope, enabled);
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_system_audio_device(
    app: AppHandle,
    device_name: Option<String>,
) -> Result<(), String> {
    if get_system_audio_availability(app.clone()).await != SystemAudioAvailability::Available {
        return Err("System audio capture is not available on this system".to_string());
    }
    let normalized_device =
        device_name.filter(|name| !name.eq_ignore_ascii_case("default") && !name.is_empty());
    let settings = get_settings(&app);
    let mut candidate_settings = settings;
    candidate_settings.system_audio_device = normalized_device.clone();
    let resolved = crate::shorthand::dictation::apply_mode(
        candidate_settings.clone(),
        crate::shorthand::mode::active(&app),
    );
    let manager = app.state::<Arc<AudioRecordingManager>>().inner().clone();
    let stream_router = if system_audio_lane_required(&candidate_settings) {
        Some(ensure_system_audio_transcription(&app)?.stream_router())
    } else {
        None
    };
    tokio::task::spawn_blocking(move || {
        manager.apply_system_audio_configuration(
            resolved.system_audio_enabled,
            resolved.system_audio_device,
            stream_router,
        )
    })
    .await
    .map_err(|error| format!("audio task join failed: {error}"))?
    .map_err(|error| format!("Failed to update system audio device: {error}"))?;

    // Re-read immediately before writing: see the note in
    // `change_system_audio_enabled_for_scope`. This command owns only the
    // device field.
    let mut settings = get_settings(&app);
    settings.system_audio_device = normalized_device;
    write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod system_audio_availability_tests {
    use super::*;

    #[test]
    fn available_when_a_loopback_host_is_reachable() {
        assert_eq!(
            availability_from_host_probe(true),
            SystemAudioAvailability::Available
        );
    }

    #[test]
    fn unavailable_when_no_loopback_host_is_reachable() {
        assert_eq!(
            availability_from_host_probe(false),
            SystemAudioAvailability::UnavailableNoSoundServer
        );
    }
}
