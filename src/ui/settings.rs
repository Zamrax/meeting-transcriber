use crate::config::Config;

use super::theme::AppColors;

pub const MODELS: &[&str] = &[
    "gemini-3.1-flash-lite-preview",
    "gemini-flash-latest",
    "gemini-2.5-flash",
    "gemini-2.0-flash",
    "gemini-1.5-flash",
    "gemini-1.5-pro",
];

pub struct SettingsState {
    pub open: bool,
    pub api_key: String,
    pub model: String,
    pub participants: String,
    pub obsidian_vault_path: String,
    pub notion_token: String,
    pub notion_parent_page_id: String,
}

impl SettingsState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            open: false,
            api_key: config.gemini_api_key.clone(),
            model: config.gemini_model.clone(),
            participants: config.participants.clone(),
            obsidian_vault_path: config.obsidian_vault_path.clone(),
            notion_token: config.notion_token.clone(),
            notion_parent_page_id: config.notion_parent_page_id.clone(),
        }
    }

    pub fn apply_to_config(&self, config: &mut Config) {
        config.gemini_api_key = self.api_key.clone();
        config.gemini_model = self.model.clone();
        config.participants = self.participants.clone();
        config.obsidian_vault_path = self.obsidian_vault_path.clone();
        config.notion_token = self.notion_token.clone();
        config.notion_parent_page_id = self.notion_parent_page_id.clone();
    }
}

/// Draw the settings dialog. Returns Some(true) if saved, Some(false) if cancelled.
pub fn draw_settings(ctx: &egui::Context, state: &mut SettingsState) -> Option<bool> {
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

            // Gemini API
            settings_section(ui, "Gemini API", |ui| {
                egui::Grid::new("settings_gemini")
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

                        settings_label(ui, "Model", label_width);
                        egui::ComboBox::from_id_salt("model_select")
                            .selected_text(&state.model)
                            .width(field_width)
                            .show_ui(ui, |ui| {
                                for &model in MODELS {
                                    ui.selectable_value(
                                        &mut state.model,
                                        model.to_string(),
                                        model,
                                    );
                                }
                            });
                        ui.end_row();
                    });
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
