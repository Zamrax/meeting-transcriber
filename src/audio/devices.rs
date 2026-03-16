use cpal::traits::{DeviceTrait, HostTrait};

/// A discovered audio device with its index info.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,
    pub is_loopback: bool,
    /// Whether this device is enumerated as an input device (true for macOS BlackHole, Linux monitors).
    /// Windows WASAPI loopback devices are output devices (false).
    pub is_input_device: bool,
    pub host_id: String,
}

/// List available microphone (input) devices.
pub fn list_microphone_devices() -> Vec<(String, AudioDevice)> {
    let mut devices = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // Prefer WASAPI host on Windows
        if let Some(host_id) = cpal::available_hosts()
            .into_iter()
            .find(|h| h.name().to_lowercase().contains("wasapi"))
        {
            if let Ok(host) = cpal::host_from_id(host_id) {
                if let Ok(input_devices) = host.input_devices() {
                    for device in input_devices {
                        if let Ok(name) = device.name() {
                            devices.push((
                                name.clone(),
                                AudioDevice {
                                    name: name.clone(),
                                    is_loopback: false,
                                    is_input_device: true,
                                    host_id: host_id.name().to_string(),
                                },
                            ));
                        }
                    }
                }
            }
        }
        if !devices.is_empty() {
            return devices;
        }
    }

    // Default host fallback (macOS CoreAudio, Linux ALSA/PulseAudio)
    let host = cpal::default_host();
    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(name) = device.name() {
                // Skip loopback/monitor devices
                let name_lower = name.to_lowercase();
                if name_lower.contains("monitor") || name_lower.contains("loopback") {
                    continue;
                }
                devices.push((
                    name.clone(),
                    AudioDevice {
                        name: name.clone(),
                        is_loopback: false,
                        is_input_device: true,
                        host_id: "default".to_string(),
                    },
                ));
            }
        }
    }

    devices
}

/// List available loopback/system audio devices for the current platform.
pub fn list_loopback_devices() -> Vec<(String, AudioDevice)> {
    let mut devices = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // On Windows, WASAPI loopback devices appear as output devices
        // that can be opened for input capture
        if let Some(host_id) = cpal::available_hosts()
            .into_iter()
            .find(|h| h.name().to_lowercase().contains("wasapi"))
        {
            if let Ok(host) = cpal::host_from_id(host_id) {
                if let Ok(output_devices) = host.output_devices() {
                    for device in output_devices {
                        if let Ok(name) = device.name() {
                            devices.push((
                                name.clone(),
                                AudioDevice {
                                    name: name.clone(),
                                    is_loopback: true,
                                    is_input_device: false,
                                    host_id: host_id.name().to_string(),
                                },
                            ));
                        }
                    }
                }
            }
        }
        return devices;
    }

    #[cfg(target_os = "macos")]
    {
        let host = cpal::default_host();
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("blackhole") {
                        devices.push((
                            name.clone(),
                            AudioDevice {
                                name: name.clone(),
                                is_loopback: true,
                                is_input_device: true,
                                host_id: "coreaudio".to_string(),
                            },
                        ));
                    }
                }
            }
        }
        if devices.is_empty() {
            log::warn!(
                "No loopback devices found on macOS. Install BlackHole (https://existential.audio/blackhole/) \
                 to capture system audio."
            );
        }
        return devices;
    }

    #[cfg(target_os = "linux")]
    {
        let host = cpal::default_host();
        if let Ok(input_devices) = host.input_devices() {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("monitor") {
                        devices.push((
                            name.clone(),
                            AudioDevice {
                                name: name.clone(),
                                is_loopback: true,
                                is_input_device: true,
                                host_id: "pulse".to_string(),
                            },
                        ));
                    }
                }
            }
        }
        return devices;
    }

    #[allow(unreachable_code)]
    devices
}

/// Get the current platform display name.
pub fn platform_display_name() -> &'static str {
    #[cfg(target_os = "windows")]
    return "Windows (WASAPI)";
    #[cfg(target_os = "macos")]
    return "macOS (CoreAudio)";
    #[cfg(target_os = "linux")]
    return "Linux (PulseAudio/ALSA)";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    return "Unknown Platform";
}
