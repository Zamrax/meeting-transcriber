use crate::config::Config;
use crate::openrouter::catalog::{self, Catalog, ModelInfo};

use super::theme::AppColors;

/// Which of the two model pickers a row belongs to.
#[derive(Copy, Clone, PartialEq)]
enum Role {
    Transcription,
    Analysis,
}

impl Role {
    /// Cost estimate for this role, since the two phases bill differently:
    /// transcription pays for audio tokens, analysis for transcript tokens.
    fn cost_per_hour(self, model: &ModelInfo) -> Option<f64> {
        match self {
            Role::Transcription => model.transcription_cost_per_hour(),
            Role::Analysis => model.analysis_cost_per_hour(),
        }
    }
}

/// Combined estimate for an hour of audio, when both models are priced.
fn total_cost_per_hour(
    transcription_model: &str,
    analysis_model: &str,
    catalog: &Catalog,
) -> Option<f64> {
    let find = |id: &str| catalog.models.iter().find(|m| m.id == id);
    let transcription = find(transcription_model)?.transcription_cost_per_hour()?;
    let analysis = find(analysis_model)?.analysis_cost_per_hour()?;
    Some(transcription + analysis)
}

pub struct SettingsState {
    pub open: bool,
    pub api_key: String,
    pub transcription_model: String,
    pub analysis_model: String,
    pub diarization: bool,
    pub participants: String,
    pub obsidian_vault_path: String,
    pub notion_token: String,
    pub notion_parent_page_id: String,
}

impl SettingsState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            open: false,
            api_key: config.openrouter_api_key.clone(),
            transcription_model: config.transcription_model.clone(),
            analysis_model: config.analysis_model.clone(),
            diarization: config.diarization,
            participants: config.participants.clone(),
            obsidian_vault_path: config.obsidian_vault_path.clone(),
            notion_token: config.notion_token.clone(),
            notion_parent_page_id: config.notion_parent_page_id.clone(),
        }
    }

    pub fn apply_to_config(&self, config: &mut Config) {
        config.openrouter_api_key = self.api_key.clone();
        config.transcription_model = self.transcription_model.trim().to_string();
        config.analysis_model = self.analysis_model.trim().to_string();
        config.diarization = self.diarization;
        config.participants = self.participants.clone();
        config.obsidian_vault_path = self.obsidian_vault_path.clone();
        config.notion_token = self.notion_token.clone();
        config.notion_parent_page_id = self.notion_parent_page_id.clone();
    }
}

/// Draw the settings dialog. Returns Some(true) if saved, Some(false) if cancelled.
pub fn draw_settings(
    ctx: &egui::Context,
    state: &mut SettingsState,
    catalog: &Catalog,
) -> Option<bool> {
    let mut result = None;

    egui::Window::new("Settings")
        .open(&mut state.open)
        .resizable(false)
        .fixed_size([520.0, 0.0])
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.add_space(4.0);

            let label_width = 75.0;
            // Fixed field width: window(520) - window_margin(2*16) - frame_margin(2*12) - label - grid_spacing
            let field_width = 520.0 - 32.0 - 24.0 - label_width - 10.0;

            // OpenRouter API
            settings_section(ui, "OpenRouter API", |ui| {
                egui::Grid::new("settings_openrouter")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        settings_label(ui, "API Key", label_width);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.api_key)
                                .password(true)
                                .desired_width(field_width),
                        );
                        ui.end_row();
                    });
            });

            // Models — one reads the audio, one writes the notes.
            settings_section(ui, "Models", |ui| {
                egui::Grid::new("settings_models")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        model_picker(
                            ui,
                            "Transcribe",
                            label_width,
                            field_width,
                            &mut state.transcription_model,
                            &catalog.transcription_models(),
                            Role::Transcription,
                        );
                        model_picker(
                            ui,
                            "Analyze",
                            label_width,
                            field_width,
                            &mut state.analysis_model,
                            &catalog.analysis_models(),
                            Role::Analysis,
                        );
                    });

                ui.add_space(6.0);

                // Speaker labels, where the chosen model can produce them.
                let supported = catalog::supports_diarization(&state.transcription_model);
                ui.add_enabled_ui(supported, |ui| {
                    ui.checkbox(&mut state.diarization, "Label speakers (diarization)");
                });
                let hint = if !supported {
                    "This model returns plain text — no speaker labels. Pick a MAI-Transcribe model to label who said what."
                } else if state.diarization {
                    "Transcript will be split into Speaker 1, Speaker 2, … turns."
                } else {
                    "This model can label who said what — tick the box to turn it on."
                };
                ui.label(
                    egui::RichText::new(hint)
                        .color(if supported && !state.diarization {
                            AppColors::BLUE
                        } else {
                            AppColors::TEXT_MUTED
                        })
                        .size(11.0),
                );

                ui.add_space(6.0);
                let estimate = match total_cost_per_hour(
                    &state.transcription_model,
                    &state.analysis_model,
                    catalog,
                ) {
                    Some(cost) => format!("Estimated ${cost:.2} per hour of recording"),
                    None => "Cost estimate unavailable for this pair".into(),
                };
                ui.label(
                    egui::RichText::new(estimate)
                        .color(AppColors::TEXT_MUTED)
                        .size(11.0),
                );
                if catalog.fetched_at.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "Showing the built-in list — the live model list could not be fetched.",
                        )
                        .color(AppColors::TEXT_MUTED)
                        .size(11.0),
                    );
                }
            });

            // Participants
            settings_section(ui, "Participants", |ui| {
                egui::Grid::new("settings_participants")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        settings_label(ui, "Names", label_width);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.participants)
                                .hint_text("Alice, Bob, Charlie (optional)")
                                .desired_width(field_width),
                        );
                        ui.end_row();
                    });
            });

            // Obsidian Export
            settings_section(ui, "Obsidian Export", |ui| {
                egui::Grid::new("settings_obsidian")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        settings_label(ui, "Vault Path", label_width);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut state.obsidian_vault_path)
                                    .desired_width(field_width - 80.0),
                            );
                            if ui
                                .add(super::theme::secondary_button("Browse"))
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    state.obsidian_vault_path =
                                        path.to_string_lossy().to_string();
                                }
                            }
                        });
                        ui.end_row();
                    });
            });

            // Notion Export
            #[cfg(feature = "notion")]
            settings_section(ui, "Notion Export", |ui| {
                egui::Grid::new("settings_notion")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        settings_label(ui, "Token", label_width);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.notion_token)
                                .password(true)
                                .desired_width(field_width),
                        );
                        ui.end_row();

                        settings_label(ui, "Page ID", label_width);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.notion_parent_page_id)
                                .desired_width(field_width),
                        );
                        ui.end_row();
                    });
            });

            ui.add_space(12.0);

            // Action buttons
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(super::theme::primary_button("Save", AppColors::BLUE))
                        .clicked()
                    {
                        result = Some(true);
                    }
                    if ui
                        .add(super::theme::secondary_button("Cancel"))
                        .clicked()
                    {
                        result = Some(false);
                    }
                });
            });
        });

    result
}

/// One model row: an editable slug plus a catalog dropdown that fills it in.
///
/// The text field is authoritative so any slug can be typed, including models
/// the catalog does not list.
fn model_picker(
    ui: &mut egui::Ui,
    label: &str,
    label_width: f32,
    field_width: f32,
    selected: &mut String,
    models: &[&ModelInfo],
    role: Role,
) {
    settings_label(ui, label, label_width);
    ui.vertical(|ui| {
        ui.add(
            egui::TextEdit::singleline(selected)
                .hint_text("provider/model-slug")
                .desired_width(field_width),
        );

        egui::ComboBox::from_id_salt(format!("model_select_{label}"))
            .selected_text(summary_for(selected, models, role))
            .width(field_width)
            .show_ui(ui, |ui| {
                for model in models {
                    let row = format!(
                        "{}  ·  {}  ·  {}",
                        model.id,
                        model.context_label(),
                        model.cost_label(role.cost_per_hour(model))
                    );
                    ui.selectable_value(selected, model.id.clone(), row);
                }
            });
    });
    ui.end_row();
}

/// Text shown on the closed dropdown: the model's cost, or a hint that the
/// typed slug is not one the catalog knows about.
fn summary_for(selected: &str, models: &[&ModelInfo], role: Role) -> String {
    match models.iter().find(|m| m.id == selected) {
        Some(model) => format!(
            "{}  ·  {}",
            model.context_label(),
            model.cost_label(role.cost_per_hour(model))
        ),
        None if selected.trim().is_empty() => "Choose a model".into(),
        None => "Custom model — not in catalog".into(),
    }
}

fn settings_section(ui: &mut egui::Ui, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(title)
            .size(13.0)
            .strong()
            .color(AppColors::TEXT_SECONDARY),
    );
    ui.add_space(2.0);
    egui::Frame::new()
        .fill(AppColors::BG_CARD)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
    ui.add_space(4.0);
}

fn settings_label(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.add_sized(
        [width, 20.0],
        egui::Label::new(
            egui::RichText::new(text)
                .color(AppColors::TEXT_SECONDARY)
                .size(13.0),
        ),
    );
}
