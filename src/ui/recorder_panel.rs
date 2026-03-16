use egui::Color32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::audio::capture::RecordingHandle;
use crate::audio::devices::{self, AudioDevice};

use super::theme::{self, AppColors};

const MAX_SECONDS: u64 = 90 * 60;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioMode {
    SystemAudio,
    Microphone,
    SystemAndMic,
}

pub struct RecorderState {
    pub mode: AudioMode,
    pub devices: Vec<AudioDevice>,
    pub selected_device: usize,
    /// Microphone devices (used for SystemAndMic mode)
    pub mic_devices: Vec<AudioDevice>,
    pub selected_mic: usize,
    pub is_recording: bool,
    pub is_analyzing: bool,
    pub start_time: Option<Instant>,
    pub pulse_on: bool,
    pub status_text: String,
    pub error_text: String,
    /// Warning text (non-fatal, e.g. silence detected)
    pub warning_text: String,
    recording_handle: Option<RecordingHandle>,
    /// Live sample counter from the audio callback — shared with the recording thread.
    live_sample_count: Arc<AtomicU64>,
    pub last_wav_bytes: Vec<u8>,
    /// Filename stem from analysis (e.g. "2026-03-15 Sprint Planning")
    pub last_filename_stem: String,
}

impl RecorderState {
    pub fn new() -> Self {
        let mut state = Self {
            mode: AudioMode::SystemAndMic,
            devices: Vec::new(),
            selected_device: 0,
            mic_devices: Vec::new(),
            selected_mic: 0,
            is_recording: false,
            is_analyzing: false,
            start_time: None,
            pulse_on: false,
            status_text: "Ready to record".into(),
            error_text: String::new(),
            warning_text: String::new(),
            recording_handle: None,
            live_sample_count: Arc::new(AtomicU64::new(0)),
            last_wav_bytes: Vec::new(),
            last_filename_stem: String::new(),
        };
        state.refresh_devices();
        state
    }

    pub fn refresh_devices(&mut self) {
        match self.mode {
            AudioMode::SystemAudio | AudioMode::SystemAndMic => {
                self.devices = devices::list_loopback_devices()
                    .into_iter()
                    .map(|(_, dev)| dev)
                    .collect();
            }
            AudioMode::Microphone => {
                self.devices = devices::list_microphone_devices()
                    .into_iter()
                    .map(|(_, dev)| dev)
                    .collect();
            }
        }
        self.selected_device = 0;

        // Always populate mic list for SystemAndMic mode
        if self.mode == AudioMode::SystemAndMic {
            self.mic_devices = devices::list_microphone_devices()
                .into_iter()
                .map(|(_, dev)| dev)
                .collect();
            self.selected_mic = 0;
        }
    }

    pub fn start_recording(&mut self) {
        self.error_text.clear();
        self.warning_text.clear();
        if self.devices.is_empty() {
            self.error_text = "No audio devices available".into();
            return;
        }

        let result = if self.mode == AudioMode::SystemAndMic {
            // Dual capture: system audio + microphone
            if self.mic_devices.is_empty() {
                self.error_text = "No microphone devices available for combined capture".into();
                return;
            }
            let lb_dev = &self.devices[self.selected_device];
            let mic_dev = &self.mic_devices[self.selected_mic];
            crate::audio::capture::start_recording_dual(
                &lb_dev.name, lb_dev.is_input_device, &mic_dev.name,
            )
        } else {
            let dev = &self.devices[self.selected_device];
            crate::audio::capture::start_recording(
                &dev.name, dev.is_loopback, dev.is_input_device,
            )
        };

        match result {
            Ok(handle) => {
                self.live_sample_count = handle.sample_count.clone();
                self.recording_handle = Some(handle);
                self.is_recording = true;
                self.start_time = Some(Instant::now());
                self.status_text = "Recording in progress...".into();
                self.pulse_on = true;
            }
            Err(e) => {
                self.error_text = e;
            }
        }
    }

    pub fn stop_recording(&mut self) -> Option<Vec<u8>> {
        self.is_recording = false;
        self.pulse_on = false;
        self.warning_text.clear();
        if let Some(handle) = self.recording_handle.take() {
            self.status_text = "Processing audio...".into();
            match handle.stop() {
                Ok(wav_bytes) => {
                    if wav_bytes.len() > 44 {
                        if is_silent_wav(&wav_bytes) {
                            log::warn!("Recording appears silent. Check audio routing.");
                            self.warning_text = if cfg!(target_os = "macos") {
                                "Recording is silent. On macOS, BlackHole requires setup:\n\
                                 1. Open Audio MIDI Setup\n\
                                 2. Create a Multi-Output Device (+ button)\n\
                                 3. Check both your speakers AND BlackHole\n\
                                 4. Set the Multi-Output Device as system output".into()
                            } else {
                                "Recording appears silent. Check audio routing.".into()
                            };
                        }

                        self.last_wav_bytes = wav_bytes.clone();
                        self.status_text = "Recording complete - analyzing...".into();
                        return Some(wav_bytes);
                    }
                    self.error_text = "No audio was captured".into();
                    self.status_text = "Ready to record".into();
                }
                Err(e) => {
                    self.error_text = format!("Recording error: {e}");
                    self.status_text = "Ready to record".into();
                }
            }
        }
        None
    }

    pub fn elapsed_display(&self) -> String {
        if let Some(start) = self.start_time {
            let secs = start.elapsed().as_secs();
            format!("{:02}:{:02}", secs / 60, secs % 60)
        } else {
            "00:00".into()
        }
    }

    pub fn is_over_time_limit(&self) -> bool {
        self.start_time
            .map(|s| s.elapsed().as_secs() >= MAX_SECONDS)
            .unwrap_or(false)
    }
}

/// Returns true when the WAV data appears silent (peak amplitude below threshold).
///
/// Mirrors the inline silence check in `stop_recording()`:
/// - Expects a standard 44-byte WAV header followed by i16 LE PCM samples.
/// - Returns false (not silent) if there are no PCM samples after the header.
pub fn is_silent_wav(wav_bytes: &[u8]) -> bool {
    if wav_bytes.len() <= 44 {
        return false;
    }
    let pcm_data = &wav_bytes[44..];
    let peak = pcm_data
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
        .max()
        .unwrap_or(0);
    peak < 100
}

/// Draw the recorder panel. Returns Some(wav_bytes) when recording completes.
pub fn draw_recorder_panel(ui: &mut egui::Ui, state: &mut RecorderState) -> Option<Vec<u8>> {
    let mut wav_result = None;

    theme::section_frame(ui, "Recording", |ui| {
        let controls_enabled = !state.is_recording && !state.is_analyzing;

        // Aligned grid for Source / System / Mic rows
        let label_col_width = 55.0;
        let prev_mode = state.mode;

        egui::Grid::new("recorder_grid")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                // Row 1: Source
                ui.add_sized(
                    [label_col_width, 20.0],
                    egui::Label::new(
                        egui::RichText::new("Source")
                            .color(AppColors::TEXT_SECONDARY)
                            .size(13.0),
                    ),
                );
                ui.add_enabled_ui(controls_enabled, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut state.mode,
                            AudioMode::SystemAndMic,
                            egui::RichText::new("System + Mic").size(13.0),
                        );
                        ui.selectable_value(
                            &mut state.mode,
                            AudioMode::SystemAudio,
                            egui::RichText::new("System Audio").size(13.0),
                        );
                        ui.selectable_value(
                            &mut state.mode,
                            AudioMode::Microphone,
                            egui::RichText::new("Microphone").size(13.0),
                        );
                    });
                });
                ui.end_row();

                // Row 2: Device / System
                let device_label = match state.mode {
                    AudioMode::SystemAndMic => "System",
                    _ => "Device",
                };
                ui.add_sized(
                    [label_col_width, 20.0],
                    egui::Label::new(
                        egui::RichText::new(device_label)
                            .color(AppColors::TEXT_SECONDARY)
                            .size(13.0),
                    ),
                );
                ui.add_enabled_ui(controls_enabled, |ui| {
                    let selected_name = state
                        .devices
                        .get(state.selected_device)
                        .map(|d| d.name.as_str())
                        .unwrap_or("No devices found");
                    egui::ComboBox::from_id_salt("device_select")
                        .selected_text(selected_name)
                        .width(ui.available_width() - 20.0)
                        .show_ui(ui, |ui| {
                            for (idx, dev) in state.devices.iter().enumerate() {
                                ui.selectable_value(&mut state.selected_device, idx, &dev.name);
                            }
                        });
                });
                ui.end_row();

                // Row 3: Mic (only in combined mode)
                if state.mode == AudioMode::SystemAndMic {
                    ui.add_sized(
                        [label_col_width, 20.0],
                        egui::Label::new(
                            egui::RichText::new("Mic")
                                .color(AppColors::TEXT_SECONDARY)
                                .size(13.0),
                        ),
                    );
                    ui.add_enabled_ui(controls_enabled, |ui| {
                        let selected_mic = state
                            .mic_devices
                            .get(state.selected_mic)
                            .map(|d| d.name.as_str())
                            .unwrap_or("No microphones found");
                        egui::ComboBox::from_id_salt("mic_select")
                            .selected_text(selected_mic)
                            .width(ui.available_width() - 20.0)
                            .show_ui(ui, |ui| {
                                for (idx, dev) in state.mic_devices.iter().enumerate() {
                                    ui.selectable_value(&mut state.selected_mic, idx, &dev.name);
                                }
                            });
                    });
                    ui.end_row();
                }
            });

        if state.mode != prev_mode {
            state.refresh_devices();
        }

        ui.add_space(12.0);

        // Recording controls - big centered area
        if state.is_recording {
            // Recording active state
            ui.vertical_centered(|ui| {
                // Timer - large and prominent
                let elapsed = state.elapsed_display();
                ui.label(
                    egui::RichText::new(&elapsed)
                        .size(42.0)
                        .monospace()
                        .strong()
                        .color(AppColors::TEXT_PRIMARY),
                );

                ui.add_space(4.0);

                // Pulsing indicator + status
                let samples = state.live_sample_count.load(Ordering::Relaxed);
                let elapsed_secs = state.start_time
                    .map(|s| s.elapsed().as_secs())
                    .unwrap_or(0);
                let no_data = elapsed_secs >= 3 && samples == 0;

                ui.horizontal(|ui| {
                    let pulse_color = if no_data {
                        AppColors::RED
                    } else if state.pulse_on {
                        AppColors::PULSE_ON
                    } else {
                        AppColors::PULSE_OFF
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, pulse_color);

                    let status = if no_data {
                        "No audio data received — check device routing"
                    } else {
                        &state.status_text
                    };
                    ui.label(
                        egui::RichText::new(status)
                            .color(if no_data { AppColors::RED } else { AppColors::RED })
                            .size(13.0),
                    );
                });

                // Live sample counter
                if samples > 0 {
                    ui.label(
                        egui::RichText::new(format!("{} samples captured", samples))
                            .color(AppColors::TEXT_MUTED)
                            .size(11.0),
                    );
                }

                state.pulse_on = state
                    .start_time
                    .map(|s| s.elapsed().as_millis() / 500 % 2 == 0)
                    .unwrap_or(false);

                ui.add_space(10.0);

                // Stop button
                let stop_btn = theme::primary_button("  Stop Recording  ", AppColors::RED)
                    .min_size(egui::vec2(180.0, 40.0));
                if ui.add(stop_btn).clicked() {
                    wav_result = state.stop_recording();
                }

                if state.is_over_time_limit() {
                    wav_result = state.stop_recording();
                }

                ui.ctx().request_repaint();
            });
        } else {
            // Ready state
            ui.vertical_centered(|ui| {
                let btn_enabled = !state.is_analyzing && !state.devices.is_empty();
                let start_btn = theme::primary_button("  Start Recording  ", AppColors::GREEN)
                    .min_size(egui::vec2(180.0, 40.0));
                if ui.add_enabled(btn_enabled, start_btn).clicked() {
                    state.start_recording();
                }

                ui.add_space(6.0);

                // Status line
                ui.label(
                    egui::RichText::new(&state.status_text)
                        .color(AppColors::TEXT_MUTED)
                        .size(12.0),
                );

                // Debug save button (subtle, only when data available)
                if !state.last_wav_bytes.is_empty() {
                    ui.add_space(4.0);
                    if ui.add(theme::secondary_button("Save WAV")).clicked() {
                        let wav_name = if state.last_filename_stem.is_empty() {
                            format!(
                                "{} recording.wav",
                                chrono::Local::now().format("%Y-%m-%d %H%M")
                            )
                        } else {
                            format!("{}.wav", state.last_filename_stem)
                        };
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(&wav_name)
                            .add_filter("WAV Audio", &["wav"])
                            .save_file()
                        {
                            if let Err(e) = std::fs::write(&path, &state.last_wav_bytes) {
                                state.error_text = format!("Failed to save WAV: {e}");
                            }
                        }
                    }
                }
            });
        }

        // Warning display (non-fatal, e.g. silence detected)
        if !state.warning_text.is_empty() {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Color32::from_rgba_premultiplied(255, 183, 77, 20))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&state.warning_text)
                            .color(Color32::from_rgb(255, 183, 77))
                            .size(13.0),
                    );
                });
        }

        // Error display
        if !state.error_text.is_empty() {
            ui.add_space(8.0);
            egui::Frame::new()
                .fill(Color32::from_rgba_premultiplied(239, 83, 80, 20))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(&state.error_text)
                            .color(AppColors::RED)
                            .size(13.0),
                    );
                });
        }
    });

    wav_result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal WAV-shaped byte buffer:
    /// 44 bytes of fake header followed by the given PCM i16 samples (LE).
    fn make_wav_bytes(samples: &[i16]) -> Vec<u8> {
        let mut buf = vec![0u8; 44]; // stub header — only length matters for the test
        for s in samples {
            buf.extend_from_slice(&s.to_le_bytes());
        }
        buf
    }

    // --- Silence detection: all-zero samples ---

    #[test]
    fn test_silence_detection_all_zero_samples() {
        let samples: Vec<i16> = vec![0i16; 1000];
        let wav = make_wav_bytes(&samples);
        assert!(
            is_silent_wav(&wav),
            "All-zero PCM should be detected as silent"
        );
    }

    // --- Silence detection: low amplitude (peak = 50, threshold is 100) ---

    #[test]
    fn test_silence_detection_low_amplitude() {
        let mut samples = vec![0i16; 999];
        samples.push(50); // peak = 50, below threshold of 100
        let wav = make_wav_bytes(&samples);
        assert!(
            is_silent_wav(&wav),
            "Peak amplitude 50 should be detected as silent (threshold is 100)"
        );
    }

    #[test]
    fn test_silence_detection_peak_at_threshold_boundary() {
        // peak = 99: still silent (condition is peak < 100)
        let mut samples = vec![0i16; 999];
        samples.push(99);
        let wav = make_wav_bytes(&samples);
        assert!(
            is_silent_wav(&wav),
            "Peak amplitude 99 should still be detected as silent"
        );
    }

    // --- Silence detection: normal amplitude (peak = 5000, not silent) ---

    #[test]
    fn test_silence_detection_normal_amplitude_not_silent() {
        let mut samples = vec![0i16; 999];
        samples.push(5000); // peak = 5000, well above threshold
        let wav = make_wav_bytes(&samples);
        assert!(
            !is_silent_wav(&wav),
            "Peak amplitude 5000 should NOT be detected as silent"
        );
    }

    #[test]
    fn test_silence_detection_negative_peak_not_silent() {
        // The implementation uses unsigned_abs(), so -5000 has abs = 5000.
        let mut samples = vec![0i16; 999];
        samples.push(-5000);
        let wav = make_wav_bytes(&samples);
        assert!(
            !is_silent_wav(&wav),
            "Negative peak amplitude -5000 should NOT be detected as silent"
        );
    }

    #[test]
    fn test_silence_detection_exact_threshold_is_not_silent() {
        // peak = 100 is the first value that is NOT silent (condition peak < 100)
        let mut samples = vec![0i16; 999];
        samples.push(100);
        let wav = make_wav_bytes(&samples);
        assert!(
            !is_silent_wav(&wav),
            "Peak amplitude exactly 100 should NOT be detected as silent"
        );
    }

    // --- Edge cases ---

    #[test]
    fn test_silence_detection_wav_too_short_returns_false() {
        // A buffer shorter than or equal to 44 bytes has no PCM — not classified as silent.
        let wav = vec![0u8; 44];
        assert!(
            !is_silent_wav(&wav),
            "WAV with no PCM data (only header) should return false (not silent)"
        );
    }

    #[test]
    fn test_silence_detection_empty_buffer_returns_false() {
        assert!(
            !is_silent_wav(&[]),
            "Empty buffer should return false (not silent)"
        );
    }
}