//! PulseAudio/PipeWire capture for Linux.
//!
//! cpal's only Linux host is ALSA, which enumerates PCM names (`default`,
//! `front:CARD=Gam,DEV=0`) rather than PulseAudio sources. Monitor sources —
//! the ones that carry system audio — never appear there at all, so system
//! audio cannot be captured through cpal on a PipeWire or PulseAudio desktop.
//!
//! This module talks to the sound server instead: `pactl` lists the real
//! sources, and `parec` streams one of them as raw PCM, already at the sample
//! rate and channel count the app wants.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::TARGET_SAMPLE_RATE;

/// One capture source reported by the sound server.
#[derive(Debug, Clone, PartialEq)]
pub struct PulseSource {
    /// Stable identifier, passed to `parec --device`.
    pub name: String,
    /// Human-readable name for the UI.
    pub description: String,
    /// Monitor sources carry what the machine is playing, i.e. system audio.
    pub is_monitor: bool,
}

/// Whether this machine has the tools to capture through the sound server.
pub fn is_available() -> bool {
    has_command("pactl") && has_command("parec")
}

fn has_command(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// List capture sources, microphones and monitors alike.
pub fn list_sources() -> Result<Vec<PulseSource>, String> {
    let output = Command::new("pactl")
        .args(["-f", "json", "list", "sources"])
        .output()
        .map_err(|e| format!("Failed to run pactl: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "pactl exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    parse_sources(&String::from_utf8_lossy(&output.stdout))
}

/// Parse `pactl -f json list sources` output.
pub fn parse_sources(json: &str) -> Result<Vec<PulseSource>, String> {
    let entries: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse pactl output: {e}"))?;

    let entries = entries
        .as_array()
        .ok_or("pactl output was not a list of sources")?;

    Ok(entries
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_string();
            let description = entry
                .get("description")
                .and_then(|d| d.as_str())
                .filter(|d| !d.is_empty())
                .unwrap_or(&name)
                .to_string();
            let is_monitor = name.ends_with(".monitor");
            Some(PulseSource {
                name,
                description,
                is_monitor,
            })
        })
        .collect())
}

/// A running `parec` capture.
pub struct PulseRecorder {
    child: Child,
    reader: Option<std::thread::JoinHandle<()>>,
    pcm: Arc<Mutex<Vec<u8>>>,
    source: String,
}

impl PulseRecorder {
    /// Start capturing one source as 16 kHz mono PCM.
    ///
    /// `sample_count` is incremented as audio arrives so the UI can show a
    /// live level, matching the cpal path.
    pub fn start(source: &str, sample_count: Arc<AtomicU64>) -> Result<Self, String> {
        let mut child = Command::new("parec")
            .args([
                format!("--device={source}"),
                "--format=s16le".to_string(),
                format!("--rate={TARGET_SAMPLE_RATE}"),
                "--channels=1".to_string(),
                "--raw".to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start parec: {e}"))?;

        let mut stdout = child
            .stdout
            .take()
            .ok_or("parec produced no output stream")?;

        let pcm = Arc::new(Mutex::new(Vec::with_capacity(16 * 1024 * 1024)));
        let sink = pcm.clone();

        let reader = std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        sample_count.fetch_add(n as u64 / 2, Ordering::Relaxed);
                        let Ok(mut sink) = sink.lock() else {
                            break;
                        };
                        sink.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) => {
                        log::error!("parec read failed: {e}");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            reader: Some(reader),
            pcm,
            source: source.to_string(),
        })
    }

    /// Stop the capture and return the raw 16 kHz mono PCM collected so far.
    pub fn stop(mut self) -> Result<Vec<u8>, String> {
        // Killing parec closes the pipe, which ends the reader thread.
        let _ = self.child.kill();
        let _ = self.child.wait();

        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }

        let pcm = self
            .pcm
            .lock()
            .map_err(|e| format!("Audio buffer lock poisoned: {e}"))?
            .clone();

        log::info!(
            "parec capture of '{}' ended: {} samples",
            self.source,
            pcm.len() / 2
        );
        Ok(pcm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
        {"index": 75,
         "name": "alsa_output.pci-0000_0a_00.1.hdmi-stereo.monitor",
         "description": "Monitor of Digital Stereo (HDMI)",
         "driver": "PipeWire"},
        {"index": 77,
         "name": "alsa_input.pci-0000_0c_00.4.analog-stereo",
         "description": "Built-in Audio Analog Stereo",
         "driver": "PipeWire"},
        {"index": 78,
         "name": "alsa_input.usb-Kingston_HyperX_QuadCast_S_4100-00.analog-stereo",
         "driver": "PipeWire"}
    ]"#;

    #[test]
    fn test_parse_separates_monitors_from_inputs() {
        let sources = parse_sources(SAMPLE).unwrap();
        assert_eq!(sources.len(), 3);

        assert!(sources[0].is_monitor);
        assert_eq!(sources[0].description, "Monitor of Digital Stereo (HDMI)");
        assert!(!sources[1].is_monitor);
        assert_eq!(sources[1].description, "Built-in Audio Analog Stereo");
    }

    #[test]
    fn test_parse_falls_back_to_name_without_description() {
        let sources = parse_sources(SAMPLE).unwrap();
        assert_eq!(sources[2].description, sources[2].name);
    }

    #[test]
    fn test_parse_rejects_non_list() {
        assert!(parse_sources("{}").is_err());
        assert!(parse_sources("not json").is_err());
    }

    #[test]
    fn test_parse_empty_list() {
        assert!(parse_sources("[]").unwrap().is_empty());
    }
}
