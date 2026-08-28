use cpal::traits::HostTrait;
#[cfg(target_os = "linux")]
use cpal::traits::DeviceTrait;

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
    let default_name = host.default_input_device().map(|device| device.to_string());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.input_devices()?.enumerate() {
        let name = device.to_string();

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
        .map(|device| device.to_string());

    let mut out = Vec::<CpalDeviceInfo>::new();

    for (index, device) in host.output_devices()?.enumerate() {
        let name = device.to_string();

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
        return list_linux_system_audio_devices();
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

    let host = crate::audio_toolkit::get_system_audio_host()?;
    if let Some(device_id) = device_id {
        let id = cpal::DeviceId::from_str(device_id).ok()?;
        return host.device_by_id(&id);
    }

    list_linux_system_audio_devices()
        .ok()?
        .into_iter()
        .find(|device| device.is_default)
        .map(|device| device.device)
}

#[cfg(target_os = "linux")]
fn list_linux_system_audio_devices(
) -> Result<Vec<SystemAudioDeviceInfo>, Box<dyn std::error::Error>> {
    let host = crate::audio_toolkit::get_system_audio_host()
        .ok_or("No supported Linux sound server is available")?;

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
                label: device.to_string(),
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
        label: device.to_string(),
        is_default: true,
        device,
    }])
}
