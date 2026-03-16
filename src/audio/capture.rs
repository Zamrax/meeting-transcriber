use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use super::wav::{assemble_wav, mix_mono_streams};

/// Recording state shared between the recording thread and the caller.
pub struct RecordingHandle {
    stop_tx: mpsc::Sender<()>,
    result_rx: mpsc::Receiver<Result<Vec<u8>, String>>,
    /// Live sample counter — incremented by the audio callback, readable from the UI thread.
    pub sample_count: Arc<AtomicU64>,
}

impl RecordingHandle {
    /// Stop recording and return the assembled WAV bytes.
    pub fn stop(self) -> Result<Vec<u8>, String> {
        let _ = self.stop_tx.send(());
        self.result_rx
            .recv()
            .map_err(|e| format!("Recording thread disconnected: {e}"))?
    }
}

/// Start recording from a single named device.
pub fn start_recording(
    device_name: &str,
    is_loopback: bool,
    is_input_device: bool,
) -> Result<RecordingHandle, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

    let device_name = device_name.to_string();
    let sample_count = Arc::new(AtomicU64::new(0));
    let sample_count_clone = sample_count.clone();

    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let device = find_device(&device_name, is_loopback, is_input_device)?;
            let config = get_device_config(&device, is_loopback, is_input_device)?;

            let sample_rate = config.sample_rate().0;
            let channels = config.channels();
            let sample_format = config.sample_format();

            log::info!(
                "Recording device '{}': rate={}, channels={}, format={:?}, loopback={}, input_device={}",
                device_name, sample_rate, channels, sample_format, is_loopback, is_input_device
            );

            // Pre-allocate for ~90 min of callbacks (~20 ms each ≈ 270,000 chunks).
            let chunks: Arc<Mutex<Vec<Vec<u8>>>> =
                Arc::new(Mutex::new(Vec::with_capacity(300_000)));
            let stream_config: StreamConfig = config.into();

            let stream = build_input_stream(
                &device,
                &stream_config,
                sample_format,
                chunks.clone(),
                sample_count_clone,
            )?;

            stream
                .play()
                .map_err(|e| format!("Failed to start stream: {e}"))?;

            let _ = startup_tx.send(Ok(()));
            let _ = stop_rx.recv();
            drop(stream);

            let collected = chunks.lock()
                .map_err(|e| format!("Audio buffer lock poisoned: {e}"))?
                .clone();
            let total_samples: usize = collected.iter().map(|c| c.len()).sum::<usize>() / 2;
            log::info!("Recording ended for '{}': {} total samples", device_name, total_samples);
            let wav = assemble_wav(&collected, sample_rate, channels);
            let _ = result_tx.send(Ok(wav));
            Ok(())
        };

        if let Err(e) = run() {
            let _ = startup_tx.send(Err(e.clone()));
            let _ = result_tx.send(Err(e));
        }
    });

    match startup_rx
        .recv()
        .map_err(|e| format!("Recording thread crashed: {e}"))?
    {
        Ok(()) => Ok(RecordingHandle { stop_tx, result_rx, sample_count }),
        Err(e) => Err(e),
    }
}

/// Start recording from BOTH a loopback device and a microphone simultaneously.
/// The two streams are mixed into a single mono WAV.
pub fn start_recording_dual(
    loopback_name: &str,
    loopback_is_input: bool,
    mic_name: &str,
) -> Result<RecordingHandle, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

    let loopback_name = loopback_name.to_string();
    let mic_name = mic_name.to_string();
    let sample_count = Arc::new(AtomicU64::new(0));
    let sample_count_clone = sample_count.clone();

    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            // Open loopback device
            let lb_device = find_device(&loopback_name, true, loopback_is_input)?;
            let lb_config = get_device_config(&lb_device, true, loopback_is_input)?;
            let lb_rate = lb_config.sample_rate().0;
            let lb_channels = lb_config.channels();
            let lb_format = lb_config.sample_format();
            let lb_stream_config: StreamConfig = lb_config.into();

            // Open microphone device (always an input device)
            let mic_device = find_device(&mic_name, false, true)?;
            let mic_config = get_device_config(&mic_device, false, true)?;
            let mic_rate = mic_config.sample_rate().0;
            let mic_channels = mic_config.channels();
            let mic_format = mic_config.sample_format();
            let mic_stream_config: StreamConfig = mic_config.into();

            log::info!(
                "Dual recording - loopback '{}': rate={}, ch={}, fmt={:?}, input_dev={}",
                loopback_name, lb_rate, lb_channels, lb_format, loopback_is_input
            );
            log::info!(
                "Dual recording - mic '{}': rate={}, ch={}, fmt={:?}",
                mic_name, mic_rate, mic_channels, mic_format
            );

            let lb_chunks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
            let mic_chunks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));

            let lb_sample_count = sample_count_clone.clone();
            let mic_sample_count = sample_count_clone;
            let lb_stream = build_input_stream(
                &lb_device,
                &lb_stream_config,
                lb_format,
                lb_chunks.clone(),
                lb_sample_count,
            )?;
            let mic_stream = build_input_stream(
                &mic_device,
                &mic_stream_config,
                mic_format,
                mic_chunks.clone(),
                mic_sample_count,
            )?;

            lb_stream
                .play()
                .map_err(|e| format!("Failed to start loopback stream: {e}"))?;
            mic_stream
                .play()
                .map_err(|e| format!("Failed to start mic stream: {e}"))?;

            let _ = startup_tx.send(Ok(()));
            let _ = stop_rx.recv();

            drop(lb_stream);
            drop(mic_stream);

            // Assemble each stream to 16kHz mono, then mix
            let lb_collected = lb_chunks.lock()
                .map_err(|e| format!("Loopback audio buffer lock poisoned: {e}"))?
                .clone();
            let mic_collected = mic_chunks.lock()
                .map_err(|e| format!("Mic audio buffer lock poisoned: {e}"))?
                .clone();

            let lb_total: usize = lb_collected.iter().map(|c| c.len()).sum::<usize>() / 2;
            let mic_total: usize = mic_collected.iter().map(|c| c.len()).sum::<usize>() / 2;
            log::info!("Dual recording ended - loopback: {} samples, mic: {} samples", lb_total, mic_total);

            let lb_wav_pcm = assemble_to_mono_pcm(&lb_collected, lb_rate, lb_channels);
            let mic_wav_pcm = assemble_to_mono_pcm(&mic_collected, mic_rate, mic_channels);

            let mixed = mix_mono_streams(&lb_wav_pcm, &mic_wav_pcm);

            // Wrap mixed PCM in a WAV container
            let wav = super::wav::write_wav_public(&mixed, super::TARGET_SAMPLE_RATE, 1);
            let _ = result_tx.send(Ok(wav));
            Ok(())
        };

        if let Err(e) = run() {
            let _ = startup_tx.send(Err(e.clone()));
            let _ = result_tx.send(Err(e));
        }
    });

    match startup_rx
        .recv()
        .map_err(|e| format!("Recording thread crashed: {e}"))?
    {
        Ok(()) => Ok(RecordingHandle { stop_tx, result_rx, sample_count }),
        Err(e) => Err(e),
    }
}

/// Assemble raw chunks to 16kHz mono PCM bytes (no WAV header).
fn assemble_to_mono_pcm(chunks: &[Vec<u8>], sample_rate: u32, channels: u16) -> Vec<u8> {
    use super::wav::{resample, stereo_to_mono};

    let total_len: usize = chunks.iter().map(|c| c.len()).sum();
    let mut raw = Vec::with_capacity(total_len);
    for chunk in chunks {
        raw.extend_from_slice(chunk);
    }
    if raw.is_empty() {
        return vec![];
    }
    let mono = stereo_to_mono(&raw, channels);
    resample(&mono, sample_rate, super::TARGET_SAMPLE_RATE)
}

/// Build a cpal input stream that pushes i16 LE bytes into the shared chunks vec.
fn build_input_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    chunks: Arc<Mutex<Vec<Vec<u8>>>>,
    counter: Arc<AtomicU64>,
) -> Result<cpal::Stream, String> {
    let err_fn = |err: cpal::StreamError| {
        log::error!("Audio stream error: {err}");
    };

    match sample_format {
        SampleFormat::I16 => {
            let chunks_clone = chunks.clone();
            let counter_clone = counter;
            device
                .build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        counter_clone.fetch_add(data.len() as u64, Ordering::Relaxed);
                        let bytes: Vec<u8> =
                            data.iter().flat_map(|s| s.to_le_bytes()).collect();
                        match chunks_clone.lock() {
                            Ok(mut c) => c.push(bytes),
                            Err(e) => log::error!("Audio buffer mutex lock failed (i16): {e}"),
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build i16 input stream: {e}"))
        }
        SampleFormat::F32 => {
            let chunks_clone = chunks.clone();
            let counter_clone = counter;
            device
                .build_input_stream(
                    config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        counter_clone.fetch_add(data.len() as u64, Ordering::Relaxed);
                        let bytes: Vec<u8> = data
                            .iter()
                            .map(|&s| {
                                let clamped = s.clamp(-1.0, 1.0);
                                (clamped * i16::MAX as f32) as i16
                            })
                            .flat_map(|s| s.to_le_bytes())
                            .collect();
                        match chunks_clone.lock() {
                            Ok(mut c) => c.push(bytes),
                            Err(e) => log::error!("Audio buffer mutex lock failed (f32): {e}"),
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build f32 input stream: {e}"))
        }
        _ => Err(format!("Unsupported sample format: {sample_format:?}")),
    }
}

/// Find a cpal device by name.
///
/// For loopback devices, the search order depends on `is_input_device`:
/// - macOS (BlackHole) and Linux (PulseAudio monitors) are input devices
/// - Windows (WASAPI loopback) are output devices
fn find_device(name: &str, is_loopback: bool, is_input_device: bool) -> Result<cpal::Device, String> {
    let host = cpal::default_host();

    if is_loopback && !is_input_device {
        // Windows WASAPI loopback: search output devices first
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(n) = device.name() {
                    if n == name {
                        return Ok(device);
                    }
                }
            }
        }
    }

    // Search input devices (primary path for macOS/Linux loopback and all mic devices)
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(n) = device.name() {
                if n == name {
                    return Ok(device);
                }
            }
        }
    }

    // Fallback: search output devices if not found in input
    if is_loopback && is_input_device {
        if let Ok(devices) = host.output_devices() {
            for device in devices {
                if let Ok(n) = device.name() {
                    if n == name {
                        return Ok(device);
                    }
                }
            }
        }
    }

    Err(format!("Audio device not found: {name}"))
}

/// Get a supported stream configuration for the device.
///
/// For loopback devices, the config source depends on `is_input_device`:
/// - macOS (BlackHole) and Linux (PulseAudio monitors): use input config first
/// - Windows (WASAPI loopback): use output config first
fn get_device_config(
    device: &cpal::Device,
    is_loopback: bool,
    is_input_device: bool,
) -> Result<cpal::SupportedStreamConfig, String> {
    if is_loopback && !is_input_device {
        // Windows WASAPI loopback: try output config first
        if let Ok(config) = device.default_output_config() {
            return Ok(config);
        }
        if let Ok(config) = device.default_input_config() {
            return Ok(config);
        }
        Err("No supported config for loopback device".into())
    } else {
        // macOS/Linux loopback (input device) or regular mic: use input config
        if let Ok(config) = device.default_input_config() {
            return Ok(config);
        }
        // Fallback for loopback devices
        if is_loopback {
            if let Ok(config) = device.default_output_config() {
                return Ok(config);
            }
        }
        Err(format!("No supported input config for device"))
    }
}
