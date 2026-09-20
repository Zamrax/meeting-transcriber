use std::sync::mpsc;

use crate::audio::devices;
use crate::config::Config;
use crate::openrouter::catalog::{self, Catalog};
use crate::openrouter::client::{ClientConfig, OpenRouterClient};
use crate::schema::MeetingAnalysis;

use super::recorder_panel::{self, RecorderState};
use super::results_panel::{self, ResultsState};
use super::settings::{self, SettingsState};
use super::theme::{self, AppColors};

/// Replacement written in place of a redacted secret.
const REDACTION: &str = "***";

const CREDENTIAL_PATTERNS: &[&str] = &["key=", "Bearer ", "Authorization:", "sk-or-"];

pub struct MeetingTranscriberApp {
    config: Config,
    recorder: RecorderState,
    results: ResultsState,
    settings: SettingsState,
    analysis_rx: Option<mpsc::Receiver<Result<MeetingAnalysis, String>>>,
    progress_rx: Option<mpsc::Receiver<String>>,
    catalog: Catalog,
    catalog_rx: Option<mpsc::Receiver<Catalog>>,
    analysis_status: String,
    is_analyzing: bool,
    analysis_error: String,
}

impl MeetingTranscriberApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_dark_theme(&cc.egui_ctx);
        let config = Config::load();
        let settings = SettingsState::from_config(&config);

        // Start from the cached list so the dropdowns are populated instantly,
        // then refresh in the background. Neither step blocks startup.
        let catalog = catalog::load_cached().unwrap_or_else(catalog::builtin);
        let catalog_rx = Some(spawn_catalog_refresh());

        Self {
            config,
            recorder: RecorderState::new(),
            results: ResultsState::new(),
            settings,
            analysis_rx: None,
            progress_rx: None,
            catalog,
            catalog_rx,
            analysis_status: String::new(),
            is_analyzing: false,
            analysis_error: String::new(),
        }
    }

    fn start_analysis(&mut self, wav_bytes: Vec<u8>) {
        self.analysis_error.clear();

        if self.config.openrouter_api_key.is_empty() {
            self.analysis_error = "OpenRouter API key not set. Open Settings to configure.".into();
            return;
        }

        let (tx, rx) = mpsc::channel();
        let (progress_tx, progress_rx) = mpsc::channel();
        self.analysis_rx = Some(rx);
        self.progress_rx = Some(progress_rx);
        self.analysis_status = "Preparing audio...".into();
        self.is_analyzing = true;
        self.recorder.is_analyzing = true;

        let api_key = self.config.openrouter_api_key.clone();
        let transcription_model = self.config.transcription_model.clone();
        let analysis_model = self.config.analysis_model.clone();
        // Dedicated speech-to-text models use a different endpoint, so the
        // catalog decides which path the client takes.
        let transcription_is_asr = self
            .catalog
            .models
            .iter()
            .find(|m| m.id == transcription_model)
            .is_some_and(|m| m.transcription_only);
        let diarization = self.config.diarization;
        let participants = self.config.participant_names();

        std::thread::spawn(move || {
            let result = match OpenRouterClient::new(ClientConfig {
                api_key: &api_key,
                transcription_model: &transcription_model,
                analysis_model: &analysis_model,
                transcription_is_asr,
                diarization,
            }) {
                Ok(client) => {
                    let names = if participants.is_empty() {
                        None
                    } else {
                        Some(participants.as_slice())
                    };
                    let progress = |status: &str| {
                        let _ = progress_tx.send(status.to_string());
                    };
                    client.analyze_audio(wav_bytes, names, &progress)
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
        });
    }

    /// Swap in a freshly fetched model list when the background request lands.
    fn poll_catalog(&mut self) {
        let Some(rx) = &self.catalog_rx else {
            return;
        };
        if let Ok(catalog) = rx.try_recv() {
            self.catalog = catalog;
            self.catalog_rx = None;
        }
    }

    fn poll_analysis(&mut self) {
        if let Some(rx) = &self.progress_rx {
            while let Ok(status) = rx.try_recv() {
                self.analysis_status = status;
            }
        }

        if let Some(rx) = &self.analysis_rx {
            if let Ok(result) = rx.try_recv() {
                self.is_analyzing = false;
                self.recorder.is_analyzing = false;
                self.analysis_rx = None;
                self.progress_rx = None;
                self.analysis_status.clear();

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
        self.poll_catalog();
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
                        let status = if self.analysis_status.is_empty() {
                            "Analyzing audio..."
                        } else {
                            &self.analysis_status
                        };
                        ui.label(
                            egui::RichText::new(status)
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
                                let status = if self.analysis_status.is_empty() {
                                    "Transcribing and analyzing your meeting..."
                                } else {
                                    &self.analysis_status
                                };
                                ui.label(
                                    egui::RichText::new(status)
                                        .color(AppColors::TEXT_SECONDARY)
                                        .size(14.0),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new("Long recordings are split into parts; this may take several minutes.")
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
            if let Some(saved) = settings::draw_settings(ctx, &mut self.settings, &self.catalog) {
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

/// Byte length of the whitespace run at the start of `text`.
/// Fetch the model list off the UI thread, caching it for the next launch.
/// A failure is logged and left to the cached or built-in list.
fn spawn_catalog_refresh() -> mpsc::Receiver<Catalog> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || match catalog::fetch() {
        Ok(catalog) => {
            if let Err(e) = catalog::save_cache(&catalog) {
                log::warn!("Failed to cache model list: {e}");
            }
            let _ = tx.send(catalog);
        }
        Err(e) => log::warn!("Failed to refresh model list: {e}"),
    });
    rx
}

fn leading_whitespace(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

fn scrub_credentials(message: &str) -> String {
    let mut result = message.to_string();
    for pattern in CREDENTIAL_PATTERNS {
        // Build a new string each pass to avoid index-shift issues with replace_range.
        // The rewritten text still contains `pattern`, so the search has to resume
        // past the redaction rather than restart — otherwise it matches forever.
        let mut search_from = 0;
        while let Some(offset) = result[search_from..].find(pattern) {
            let pos = search_from + offset;
            let after_pattern = pos + pattern.len();
            // `Authorization:` and friends put a space before the value, so skip
            // leading whitespace — otherwise the value scan ends before it starts.
            let value_start = after_pattern + leading_whitespace(&result[after_pattern..]);
            let end = result[value_start..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '&')
                .map(|i| value_start + i)
                .unwrap_or(result.len());
            result = format!("{}{REDACTION}{}", &result[..value_start], &result[end..]);
            search_from = value_start + REDACTION.len();
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
    fn test_scrub_credentials_terminates_without_delimiter() {
        // The redacted text still contains the pattern, so a naive rescan loops forever.
        let scrubbed = scrub_credentials("request failed with key=abc123");
        assert!(!scrubbed.contains("abc123"));
        assert_eq!(scrubbed.matches("***").count(), 1);
    }

    #[test]
    fn test_scrub_credentials_is_idempotent() {
        let once = scrub_credentials("Bearer tok3n and key=v4lue&x=1");
        assert_eq!(scrub_credentials(&once), once);
    }

    #[test]
    fn test_scrub_no_credentials() {
        let msg = "Normal error message";
        let scrubbed = scrub_credentials(msg);
        assert_eq!(scrubbed, msg);
    }
}
