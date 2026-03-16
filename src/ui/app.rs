use std::sync::mpsc;

use crate::audio::devices;
use crate::config::Config;
use crate::gemini::client::GeminiClient;
use crate::schema::MeetingAnalysis;

use super::recorder_panel::{self, RecorderState};
use super::results_panel::{self, ResultsState};
use super::settings::{self, SettingsState};
use super::theme::{self, AppColors};

const CREDENTIAL_PATTERNS: &[&str] = &["key=", "Bearer ", "Authorization:", "x-goog-api-key:"];

pub struct MeetingTranscriberApp {
    config: Config,
    recorder: RecorderState,
    results: ResultsState,
    settings: SettingsState,
    analysis_rx: Option<mpsc::Receiver<Result<MeetingAnalysis, String>>>,
    is_analyzing: bool,
    analysis_error: String,
}

impl MeetingTranscriberApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_dark_theme(&cc.egui_ctx);
        let config = Config::load();
        let settings = SettingsState::from_config(&config);

        Self {
            config,
            recorder: RecorderState::new(),
            results: ResultsState::new(),
            settings,
            analysis_rx: None,
            is_analyzing: false,
            analysis_error: String::new(),
        }
    }

    fn start_analysis(&mut self, wav_bytes: Vec<u8>) {
        self.analysis_error.clear();

        if self.config.gemini_api_key.is_empty() {
            self.analysis_error = "Gemini API key not set. Open Settings to configure.".into();
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.analysis_rx = Some(rx);
        self.is_analyzing = true;
        self.recorder.is_analyzing = true;

        let api_key = self.config.gemini_api_key.clone();
        let model = self.config.gemini_model.clone();
        let participants = self.config.participant_names();

        std::thread::spawn(move || {
            let result = match GeminiClient::new(&api_key, &model) {
                Ok(client) => {
                    let names = if participants.is_empty() {
                        None
                    } else {
                        Some(participants.as_slice())
                    };
                    client.analyze_audio(wav_bytes, names)
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
        });
    }

    fn poll_analysis(&mut self) {
        if let Some(rx) = &self.analysis_rx {
            if let Ok(result) = rx.try_recv() {
                self.is_analyzing = false;
                self.recorder.is_analyzing = false;
                self.analysis_rx = None;

                match result {
                    Ok(analysis) => {
                        // Store filename stem for WAV save (same as .md but without extension)
                        let md_name = crate::export::markdown::get_filename(&analysis);
                        self.recorder.last_filename_stem =
                            md_name.strip_suffix(".md").unwrap_or(&md_name).to_string();
                        self.results.set_analysis(analysis);
                    }
                    Err(e) => {
                        self.analysis_error = scrub_credentials(&e);
                    }
                }
            }
        }
    }
}

impl eframe::App for MeetingTranscriberApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_analysis();

        // Top bar
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(AppColors::BG_SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 12))
                    .stroke(egui::Stroke::new(1.0, AppColors::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Meeting Transcriber")
                            .size(20.0)
                            .strong()
                            .color(AppColors::TEXT_PRIMARY),
                    );

                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.add(theme::secondary_button("Settings")).clicked() {
                                self.settings = SettingsState::from_config(&self.config);
                                self.settings.open = true;
                            }
                        },
                    );
                });
            });

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(AppColors::BG_SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 6))
                    .stroke(egui::Stroke::new(1.0, AppColors::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(devices::platform_display_name())
                            .color(AppColors::TEXT_MUTED)
                            .size(11.0),
                    );
                    ui.separator();
                    if self.is_analyzing {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new("Analyzing audio...")
                                .color(AppColors::BLUE)
                                .size(11.0),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Ready")
                                .color(AppColors::TEXT_MUTED)
                                .size(11.0),
                        );
                    }
                });
            });

        // Main content
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(AppColors::BG_BASE)
                    .inner_margin(egui::Margin::symmetric(24, 16)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Make content fill width
                    ui.set_min_width(ui.available_width());

                    // Recorder panel
                    if let Some(wav_bytes) =
                        recorder_panel::draw_recorder_panel(ui, &mut self.recorder)
                    {
                        self.start_analysis(wav_bytes);
                    }

                    ui.add_space(12.0);

                    // Analysis error
                    if !self.analysis_error.is_empty() {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgba_premultiplied(239, 83, 80, 20))
                            .corner_radius(egui::CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&self.analysis_error)
                                        .color(AppColors::RED)
                                        .size(13.0),
                                );
                            });
                        ui.add_space(12.0);
                    }

                    // Analysis progress
                    if self.is_analyzing {
                        theme::section_frame(ui, "Analyzing", |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                ui.spinner();
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(
                                        "Transcribing and analyzing your meeting...",
                                    )
                                    .color(AppColors::TEXT_SECONDARY)
                                    .size(14.0),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("This may take several minutes for longer recordings.")
                                        .color(AppColors::TEXT_MUTED)
                                        .size(12.0),
                                );
                                ui.add_space(16.0);
                            });
                        });
                        ui.add_space(12.0);
                    }

                    // Results panel
                    results_panel::draw_results_panel(ui, &mut self.results, &self.config);
                });
            });

        // Settings dialog
        if self.settings.open {
            if let Some(saved) = settings::draw_settings(ctx, &mut self.settings) {
                if saved {
                    self.settings.apply_to_config(&mut self.config);
                    if let Err(e) = self.config.save() {
                        log::error!("Failed to save config: {e}");
                    }
                }
                self.settings.open = false;
            }
        }

        if self.is_analyzing {
            ctx.request_repaint();
        }
    }
}

fn scrub_credentials(message: &str) -> String {
    let mut result = message.to_string();
    for pattern in CREDENTIAL_PATTERNS {
        while let Some(pos) = result.find(pattern) {
            let start = pos + pattern.len();
            if let Some(end) = result[start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '&')
            {
                result.replace_range(start..start + end, "***");
            } else {
                result.replace_range(start.., "***");
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrub_credentials_api_key() {
        let msg = "Error at key=abc123def&other=stuff";
        let scrubbed = scrub_credentials(msg);
        assert!(!scrubbed.contains("abc123def"));
        assert!(scrubbed.contains("***"));
    }

    #[test]
    fn test_scrub_credentials_bearer() {
        let msg = "Error: Bearer secrettoken123 in request";
        let scrubbed = scrub_credentials(msg);
        assert!(!scrubbed.contains("secrettoken123"));
        assert!(scrubbed.contains("***"));
    }

    #[test]
    fn test_scrub_credentials_authorization() {
        let msg = "Header Authorization: mysecrettoken in request";
        let scrubbed = scrub_credentials(msg);
        assert!(!scrubbed.contains("mysecrettoken"));
        assert!(scrubbed.contains("***"));
    }

    #[test]
    fn test_scrub_credentials_multiple_occurrences() {
        let msg = "key=first&retry key=second end";
        let scrubbed = scrub_credentials(msg);
        assert!(!scrubbed.contains("first"));
        assert!(!scrubbed.contains("second"));
    }

    #[test]
    fn test_scrub_no_credentials() {
        let msg = "Normal error message";
        let scrubbed = scrub_credentials(msg);
        assert_eq!(scrubbed, msg);
    }
}
