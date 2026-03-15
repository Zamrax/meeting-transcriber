use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use super::wav::{assemble_wav, mix_mono_streams};

/// Recording state shared between the recording thread and the caller.
pub struct RecordingHandle {
    stop_tx: mpsc::Sender<()>,
    result_rx: mpsc::Receiver<Result<Vec<u8>, String>>,
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
) -> Result<RecordingHandle, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

    let device_name = device_name.to_string();

    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            let device = find_device(&device_name, is_loopback)?;
            let config = get_device_config(&device, is_loopback)?;

            let sample_rate = config.sample_rate().0;
            let channels = config.channels();
            let sample_format = config.sample_format();

            let chunks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
            let stream_config: StreamConfig = config.into();

            let stream = build_input_stream(
                &device,
                &stream_config,
                sample_format,
                chunks.clone(),
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
        Ok(()) => Ok(RecordingHandle { stop_tx, result_rx }),
        Err(e) => Err(e),
    }
}

/// Start recording from BOTH a loopback device and a microphone simultaneously.
/// The two streams are mixed into a single mono WAV.
pub fn start_recording_dual(
    loopback_name: &str,
    mic_name: &str,
) -> Result<RecordingHandle, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

    let loopback_name = loopback_name.to_string();
    let mic_name = mic_name.to_string();

    std::thread::spawn(move || {
        let run = || -> Result<(), String> {
            // Open loopback device
            let lb_device = find_device(&loopback_name, true)?;
            let lb_config = get_device_config(&lb_device, true)?;
            let lb_rate = lb_config.sample_rate().0;
            let lb_channels = lb_config.channels();
            let lb_format = lb_config.sample_format();
            let lb_stream_config: StreamConfig = lb_config.into();

            // Open microphone device
            let mic_device = find_device(&mic_name, false)?;
            let mic_config = get_device_config(&mic_device, false)?;
            let mic_rate = mic_config.sample_rate().0;
            let mic_channels = mic_config.channels();
            let mic_format = mic_config.sample_format();
            let mic_stream_config: StreamConfig = mic_config.into();

            let lb_chunks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
            let mic_chunks: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));

            let lb_stream = build_input_stream(
                &lb_device,
                &lb_stream_config,
                lb_format,
                lb_chunks.clone(),
            )?;
            let mic_stream = build_input_stream(
                &mic_device,
                &mic_stream_config,
                mic_format,
                mic_chunks.clone(),
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
        Ok(()) => Ok(RecordingHandle { stop_tx, result_rx }),
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
) -> Result<cpal::Stream, String> {
    let err_fn = |err: cpal::StreamError| {
        log::error!("Audio stream error: {err}");
    };

    match sample_format {
        SampleFormat::I16 => {
            let chunks_clone = chunks.clone();
            device
                .build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let bytes: Vec<u8> =
                            data.iter().flat_map(|s| s.to_le_bytes()).collect();
                        if let Ok(mut c) = chunks_clone.lock() {
                            c.push(bytes);
                        }
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| format!("Failed to build i16 input stream: {e}"))
        }
        SampleFormat::F32 => {
            let chunks_clone = chunks.clone();
            device
                .build_input_stream(
                    config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let bytes: Vec<u8> = data
                            .iter()
                            .map(|&s| {
                                let clamped = s.clamp(-1.0, 1.0);
                                (clamped * i16::MAX as f32) as i16
                            })
                            .flat_map(|s| s.to_le_bytes())
                            .collect();
                        if let Ok(mut c) = chunks_clone.lock() {
                            c.push(bytes);
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
fn find_device(name: &str, is_loopback: bool) -> Result<cpal::Device, String> {
    let host = cpal::default_host();

    if is_loopback {
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

    if let Ok(devices) = host.input_devices() {
        for device in devices {
            if let Ok(n) = device.name() {
                if n == name {
                    return Ok(device);
                }
            }
        }
    }

    Err(format!("Audio device not found: {name}"))
}

/// Get a supported stream configuration for the device.
fn get_device_config(
    device: &cpal::Device,
    is_loopback: bool,
) -> Result<cpal::SupportedStreamConfig, String> {
    if is_loopback {
        if let Ok(config) = device.default_output_config() {
            return Ok(config);
        }
        if let Ok(config) = device.default_input_config() {
            return Ok(config);
        }
        Err("No supported config for loopback device".into())
    } else {
        device
            .default_input_config()
            .map_err(|e| format!("No supported input config: {e}"))
    }
}
