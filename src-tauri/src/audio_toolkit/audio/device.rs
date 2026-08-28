use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;

/// The device name CPAL's `Display` impl writes, without its panic.
///
/// Every CPAL 0.18 host implements `Display for Device` as
/// `f.write_str(self.description().map_err(|_| fmt::Error)?.name())`, and a
/// `Display` that returns `Err` makes `ToString::to_string` panic. A dynamic
/// default-device handle whose endpoint has vanished, or a device whose
/// property store carries neither FriendlyName nor DeviceDesc, does exactly
/// that. The string returned here is byte-identical to `to_string()` for any
/// device that answers, so persisted `selected_microphone`,
/// `system_audio_device` and `clamshell_microphone` values keep matching.
pub fn device_display_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

#[derive(Clone)]
pub struct SystemAudioDeviceInfo {
    /// Opaque persisted identifier. On Linux this is CPAL's documented
    /// `DeviceId` serialization, never a display name.
    pub id: String,
    pub label: String,
    pub is_default: bool,
    pub device: cpal::Device,
}

pub struct CpalDeviceInfo {
    pub index: String,
    pub name: String,
    pub is_default: bool,
    pub device: cpal::Device,
}

pub fn list_input_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device_display_name(&device));

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let name = device_display_name(&device).unwrap_or_else(|| "Unknown".to_string());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

pub fn list_output_devices() -> Result<Vec<CpalDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_cpal_host();
    let default_name = host
        .default_output_device()
        .and_then(|device| device_display_name(&device));

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device_display_name(&device).unwrap_or_else(|| "Unknown".to_string());

        let is_default = Some(name.clone()) == default_name;

        out.push(CpalDeviceInfo {
            index: index.to_string(),
            name,
            is_default,
            device,
        });
    }

    Ok(out)
}

/// Lists endpoints that can actually be opened for system-audio capture.
/// Ordinary output enumeration is deliberately not reused: a PulseAudio
/// monitor source and CPAL's PipeWire `sink_default` are input streams even
/// though they represent sound sent to an output sink.
pub fn list_system_audio_devices() -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>>
{
    #[cfg(target_os = "linux")]
    {
        list_linux_system_audio_devices()
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Keep Windows's stored output-device names compatible. macOS uses the
        // same display rows until its ProcessTap-specific selector lands.
        return Ok(list_output_devices()?
            .into_iter()
            .map(|device| SystemAudioDeviceInfo {
                id: device.name.clone(),
                label: device.name,
                is_default: device.is_default,
                device: device.device,
            })
            .collect());
    }
}

#[cfg(target_os = "linux")]
pub fn resolve_linux_system_audio_device(device_id: Option<&str>) -> Option<cpal::Device> {
    use std::str::FromStr;

    // Built once and passed down. Constructing a host performs a real
    // PulseAudio/PipeWire handshake (CPAL's INIT_TIMEOUT is 2s) and this runs
    // synchronously on the hotkey path, inside `start_microphone_stream`.
    let host = crate::audio_toolkit::get_system_audio_host()?;

    if let Some(device_id) = device_id {
        let id = match cpal::DeviceId::from_str(device_id) {
            Ok(id) => id,
            Err(error) => {
                log::warn!(
                    "Saved system-audio device '{device_id}' is not a CPAL device id ({error}); continuing microphone-only"
                );
                return None;
            }
        };
        // A CPAL device id carries its host ("pulseaudio:<name>") and
        // `device_by_id` matches the whole id, so a selection saved while
        // pipewire-pulse was up never resolves on a later boot that only
        // reaches the native PipeWire host, and vice versa. Say which way it
        // went instead of failing silently, and leave the saved value alone:
        // the other server may well be back next boot.
        if id.host() != host.id() {
            log::warn!(
                "Saved system-audio device '{device_id}' belongs to the {} host, but {} is the sound server reachable now; continuing microphone-only",
                id.host(),
                host.id()
            );
            return None;
        }
        let device = host.device_by_id(&id);
        if device.is_none() {
            log::warn!(
                "Saved system-audio device '{device_id}' is not present on the {} host; continuing microphone-only",
                host.id()
            );
        }
        return device;
    }

    let mut devices = match list_linux_system_audio_devices_on(host) {
        Ok(devices) => devices,
        Err(error) => {
            log::warn!("Failed to enumerate Linux system-audio devices: {error}");
            return None;
        }
    };
    if devices.is_empty() {
        log::warn!("No Linux system-audio device is available; continuing microphone-only");
        return None;
    }
    let index = match devices.iter().position(|device| device.is_default) {
        Some(index) => index,
        None => {
            // `is_default` comes from matching the server's default sink name
            // against each monitor source's `monitor_of_sink_name`. A default
            // sink that exposes no monitor, or a server reporting no default
            // sink at all, leaves a non-empty list with nothing marked default.
            // The first monitor is a far better answer than no system audio.
            log::warn!(
                "No system-audio device matches the default sink; falling back to '{}'",
                devices[0].label
            );
            0
        }
    };
    Some(devices.swap_remove(index).device)
}

#[cfg(target_os = "linux")]
fn list_linux_system_audio_devices(
) -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_system_audio_host()
        .ok_or("No supported Linux sound server is available")?;
    list_linux_system_audio_devices_on(host)
}

/// Takes the host by value so a caller that has already built one does not pay
/// for a second sound-server handshake.
#[cfg(target_os = "linux")]
fn list_linux_system_audio_devices_on(
    host: cpal::Host,
) -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>> {
    match host.id() {
        cpal::HostId::PulseAudio => list_pulseaudio_monitor_devices(host),
        cpal::HostId::PipeWire => list_pipewire_sink_default(host),
        other => Err(format!("Unsupported system-audio host: {other}").into()),
    }
}

#[cfg(target_os = "linux")]
fn list_pulseaudio_monitor_devices(
    host: cpal::Host,
) -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>> {
    use std::collections::HashSet;
    use std::ffi::CString;

    let client_name = CString::new(format!("shorthand-system-audio-{}", std::process::id()))?;
    let client = pulseaudio::Client::from_env(&client_name)?;
    let server = futures::executor::block_on(client.server_info())?;
    let default_sink = server
        .default_sink_name
        .as_ref()
        .map(|name| name.to_string_lossy().into_owned());
    let monitor_sources = futures::executor::block_on(client.list_sources())?
        .into_iter()
        .filter_map(|source| {
            let sink = source.monitor_of_sink_name?;
            let source_name = source.name.to_string_lossy().into_owned();
            let is_default = default_sink
                .as_ref()
                .is_some_and(|default_sink| sink.to_string_lossy().as_ref() == default_sink);
            Some((source_name, is_default))
        })
        .collect::<Vec<_>>();
    let monitor_names = monitor_sources
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();

    let mut devices = host
        .input_devices()?
        .filter_map(|device| {
            let id = device.id().ok()?;
            if !monitor_names.contains(id.id()) {
                return None;
            }
            let is_default = monitor_sources
                .iter()
                .find(|(name, _)| name == id.id())
                .is_some_and(|(_, is_default)| *is_default);
            Some(SystemAudioDeviceInfo {
                id: id.to_string(),
                label: device_display_name(&device).unwrap_or_else(|| "Unknown".to_string()),
                is_default,
                device,
            })
        })
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(devices)
}

#[cfg(target_os = "linux")]
fn list_pipewire_sink_default(
    host: cpal::Host,
) -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>> {
    // CPAL 0.18.2 deliberately exposes `sink_default` as a duplex device. Its
    // PipeWire backend sets `STREAM_CAPTURE_SINK` whenever an input stream is
    // opened on that device, which is the documented loopback path. Do not
    // infer sink eligibility from generic input/output capability: CPAL's
    // public device description intentionally hides the PipeWire media role.
    let id = cpal::DeviceId::new(cpal::HostId::PipeWire, "sink_default");
    let device = host
        .device_by_id(&id)
        .ok_or("PipeWire did not expose its default sink")?;
    Ok(vec![SystemAudioDeviceInfo {
        id: id.to_string(),
        label: device_display_name(&device).unwrap_or_else(|| "Unknown".to_string()),
        is_default: true,
        device,
    }])
}
