/// Returns the appropriate CPAL host for the current platform.
/// On Linux, uses ALSA host. On other platforms, uses the default host.
pub fn get_cpal_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    {
        cpal::host_from_id(cpal::HostId::Alsa).unwrap_or_else(|_| cpal::default_host())
    }
    #[cfg(not(target_os = "linux"))]
    {
        cpal::default_host()
    }
}

/// Returns the CPAL host for system-audio loopback capture, or `None` where
/// this phase has no loopback-capable backend.
///
/// Deliberately not `get_cpal_host()`: Linux microphone capture uses ALSA,
/// whose device list has no per-sink monitor sources. Prefer PulseAudio first:
/// that backend also works through PipeWire's standard pulse-server
/// compatibility layer and exposes the monitor-source metadata needed to
/// distinguish valid loopback endpoints. Use CPAL's direct PipeWire backend
/// only when no pulse server is available.
pub fn get_system_audio_host() -> Option<cpal::Host> {
    #[cfg(target_os = "linux")]
    {
        [cpal::HostId::PulseAudio, cpal::HostId::PipeWire]
            .into_iter()
            .find_map(|id| cpal::host_from_id(id).ok())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Some(get_cpal_host())
    }
}
