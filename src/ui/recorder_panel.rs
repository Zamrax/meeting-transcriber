use egui::Color32;
use std::time::Instant;

use crate::audio::capture::RecordingHandle;
use crate::audio::devices;

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
    pub devices: Vec<(String, bool)>,
    pub selected_device: usize,
    /// Microphone devices (used for SystemAndMic mode)
    pub mic_devices: Vec<(String, bool)>,
    pub selected_mic: usize,
    pub is_recording: bool,
    pub is_analyzing: bool,
    pub start_time: Option<Instant>,
    pub pulse_on: bool,
    pub status_text: String,
    pub error_text: String,
    recording_handle: Option<RecordingHandle>,
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
            recording_handle: None,
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
                    .map(|(name, dev)| (name, dev.is_loopback))
                    .collect();
            }
            AudioMode::Microphone => {
                self.devices = devices::list_microphone_devices()
                    .into_iter()
                    .map(|(name, _)| (name, false))
                    .collect();
            }
        }
        self.selected_device = 0;

        // Always populate mic list for SystemAndMic mode
        if self.mode == AudioMode::SystemAndMic {
            self.mic_devices = devices::list_microphone_devices()
                .into_iter()
                .map(|(name, _)| (name, false))
                .collect();
            self.selected_mic = 0;
        }
    }

    pub fn start_recording(&mut self) {
        self.error_text.clear();
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
            let (lb_name, _) = &self.devices[self.selected_device];
            let (mic_name, _) = &self.mic_devices[self.selected_mic];
            crate::audio::capture::start_recording_dual(lb_name, mic_name)
        } else {
            let (name, is_loopback) = &self.devices[self.selected_device];
            crate::audio::capture::start_recording(name, *is_loopback)
        };

        match result {
            Ok(handle) => {
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
        if let Some(handle) = self.recording_handle.take() {
            self.status_text = "Processing audio...".into();
            match handle.stop() {
                Ok(wav_bytes) => {
                    if wav_bytes.len() > 44 {
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
                        .map(|(n, _)| n.as_str())
                        .unwrap_or("No devices found");
                    egui::ComboBox::from_id_salt("device_select")
                        .selected_text(selected_name)
                        .width(ui.available_width() - 20.0)
                        .show_ui(ui, |ui| {
                            for (idx, (name, _)) in state.devices.iter().enumerate() {
                                ui.selectable_value(&mut state.selected_device, idx, name);
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
                            .map(|(n, _)| n.as_str())
                            .unwrap_or("No microphones found");
                        egui::ComboBox::from_id_salt("mic_select")
                            .selected_text(selected_mic)
                            .width(ui.available_width() - 20.0)
                            .show_ui(ui, |ui| {
                                for (idx, (name, _)) in state.mic_devices.iter().enumerate() {
                                    ui.selectable_value(&mut state.selected_mic, idx, name);
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
                ui.horizontal(|ui| {
                    let pulse_color = if state.pulse_on {
                        AppColors::PULSE_ON
                    } else {
                        AppColors::PULSE_OFF
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 5.0, pulse_color);

                    ui.label(
                        egui::RichText::new(&state.status_text)
                            .color(AppColors::RED)
                            .size(13.0),
                    );
                });

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
