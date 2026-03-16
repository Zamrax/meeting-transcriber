use egui::Color32;

use crate::config::Config;
use crate::export::markdown;
use crate::export::obsidian;
use crate::schema::MeetingAnalysis;

use super::theme::{self, AppColors};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResultTab {
    Summary,
    ActionItems,
    Responsibilities,
    Transcript,
}

pub struct ResultsState {
    pub analysis: Option<MeetingAnalysis>,
    pub active_tab: ResultTab,
    pub export_status: String,
    pub export_error: String,
}

impl ResultsState {
    pub fn new() -> Self {
        Self {
            analysis: None,
            active_tab: ResultTab::Summary,
            export_status: String::new(),
            export_error: String::new(),
        }
    }

    pub fn set_analysis(&mut self, analysis: MeetingAnalysis) {
        self.analysis = Some(analysis);
        self.active_tab = ResultTab::Summary;
        self.export_status.clear();
        self.export_error.clear();
    }
}

/// Draw the results panel.
pub fn draw_results_panel(ui: &mut egui::Ui, state: &mut ResultsState, config: &Config) {
    let Some(analysis) = &state.analysis else {
        // Empty state
        theme::section_frame(ui, "Results", |ui| {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("No results yet")
                        .size(16.0)
                        .color(AppColors::TEXT_MUTED),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Record a meeting and it will be automatically\ntranscribed and analyzed.",
                    )
                    .size(13.0)
                    .color(AppColors::TEXT_MUTED),
                );
            });
            ui.add_space(20.0);
        });
        return;
    };

    let analysis = analysis.clone();

    theme::section_frame(ui, &analysis.meeting_title, |ui| {
        // Date subtitle
        ui.label(
            egui::RichText::new(&format!("Date: {}", analysis.meeting_date))
                .color(AppColors::TEXT_SECONDARY)
                .size(13.0),
        );

        ui.add_space(8.0);

        // Tab bar with custom styling
        ui.horizontal(|ui| {
            tab_button(ui, "Summary", &mut state.active_tab, ResultTab::Summary);
            tab_button(
                ui,
                "Action Items",
                &mut state.active_tab,
                ResultTab::ActionItems,
            );
            tab_button(
                ui,
                "Responsibilities",
                &mut state.active_tab,
                ResultTab::Responsibilities,
            );
            tab_button(
                ui,
                "Transcript",
                &mut state.active_tab,
                ResultTab::Transcript,
            );
        });

        ui.add_space(2.0);
        ui.separator();
        ui.add_space(4.0);

        // Tab content area
        egui::Frame::new()
            .fill(AppColors::BG_CARD)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, |ui| match state.active_tab {
                        ResultTab::Summary => {
                            ui.label(
                                egui::RichText::new(&analysis.summary)
                                    .size(14.0)
                                    .color(AppColors::TEXT_PRIMARY),
                            );
                        }
                        ResultTab::ActionItems => {
                            draw_action_items_table(ui, &analysis);
                        }
                        ResultTab::Responsibilities => {
                            draw_responsibilities(ui, &analysis);
                        }
                        ResultTab::Transcript => {
                            ui.label(
                                egui::RichText::new(&analysis.transcript)
                                    .monospace()
                                    .size(12.5)
                                    .color(AppColors::TEXT_PRIMARY),
                            );
                        }
                    });
            });

        ui.add_space(12.0);

        // Export bar
        let export_btn_size = egui::vec2(150.0, 32.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Export")
                    .color(AppColors::TEXT_SECONDARY)
                    .size(13.0),
            );
            ui.add_space(8.0);

            if ui
                .add(
                    theme::primary_button("Download .md", AppColors::BLUE)
                        .min_size(export_btn_size),
                )
                .clicked()
            {
                export_markdown_dialog(state, &analysis);
            }

            let obs_enabled = !config.obsidian_vault_path.is_empty();
            if ui
                .add_enabled(
                    obs_enabled,
                    theme::secondary_button("Save to Obsidian").min_size(export_btn_size),
                )
                .clicked()
            {
                export_obsidian(state, &analysis, &config.obsidian_vault_path);
            }

            #[cfg(feature = "notion")]
            {
                let notion_enabled =
                    !config.notion_token.is_empty() && !config.notion_parent_page_id.is_empty();
                if ui
                    .add_enabled(
                        notion_enabled,
                        theme::secondary_button("Push to Notion").min_size(export_btn_size),
                    )
                    .clicked()
                {
                    export_notion(state, &analysis, config);
                }
            }
        });

        // Export feedback
        if !state.export_status.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(&state.export_status)
                    .color(AppColors::GREEN)
                    .size(12.0),
            );
        }
        if !state.export_error.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(&state.export_error)
                    .color(AppColors::RED)
                    .size(12.0),
            );
        }
    });
}

fn tab_button(ui: &mut egui::Ui, text: &str, current: &mut ResultTab, tab: ResultTab) {
    let is_active = *current == tab;
    let text_color = if is_active {
        AppColors::BLUE
    } else {
        AppColors::TEXT_SECONDARY
    };

    let btn = egui::Button::new(egui::RichText::new(text).color(text_color).size(13.0))
        .fill(Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(4));

    let response = ui.add(btn);
    if response.clicked() {
        *current = tab;
    }

    // Underline indicator for active tab
    if is_active {
        let rect = response.rect;
        ui.painter().line_segment(
            [
                egui::pos2(rect.left() + 4.0, rect.bottom()),
                egui::pos2(rect.right() - 4.0, rect.bottom()),
            ],
            egui::Stroke::new(2.0, AppColors::BLUE),
        );
    }
}

fn draw_action_items_table(ui: &mut egui::Ui, analysis: &MeetingAnalysis) {
    if analysis.action_items.is_empty() {
        ui.label(
            egui::RichText::new("No action items found.")
                .color(AppColors::TEXT_MUTED)
                .size(13.0),
        );
        return;
    }

    let available_width = ui.available_width();
    // Reserve space for Owner (~100) + spacing (20) + Deadline (~80) + spacing (20)
    let task_col_width = (available_width - 220.0).max(200.0);

    egui::Grid::new("action_items_grid")
        .striped(true)
        .min_col_width(80.0)
        .spacing([20.0, 8.0])
        .show(ui, |ui| {
            // Header
            ui.label(
                egui::RichText::new("Owner")
                    .strong()
                    .size(12.0)
                    .color(AppColors::TEXT_SECONDARY),
            );
            ui.label(
                egui::RichText::new("Task")
                    .strong()
                    .size(12.0)
                    .color(AppColors::TEXT_SECONDARY),
            );
            ui.label(
                egui::RichText::new("Deadline")
                    .strong()
                    .size(12.0)
                    .color(AppColors::TEXT_SECONDARY),
            );
            ui.end_row();

            for item in &analysis.action_items {
                ui.label(
                    egui::RichText::new(&item.owner)
                        .strong()
                        .color(AppColors::TEXT_PRIMARY),
                );
                // Wrap task text within the available column width
                ui.allocate_ui(egui::vec2(task_col_width, 0.0), |ui| {
                    ui.label(
                        egui::RichText::new(&item.description)
                            .color(AppColors::TEXT_PRIMARY),
                    );
                });
                let deadline_text = item.deadline.as_deref().unwrap_or("\u{2014}");
                let deadline_color = if item.deadline.is_some() {
                    AppColors::AMBER
                } else {
                    AppColors::TEXT_MUTED
                };
                ui.label(egui::RichText::new(deadline_text).color(deadline_color));
                ui.end_row();
            }
        });
}

fn draw_responsibilities(ui: &mut egui::Ui, analysis: &MeetingAnalysis) {
    if analysis.responsibilities.is_empty() {
        ui.label(
            egui::RichText::new("No responsibilities found.")
                .color(AppColors::TEXT_MUTED)
                .size(13.0),
        );
        return;
    }

    let mut names: Vec<&String> = analysis.responsibilities.keys().collect();
    names.sort();

    for name in names {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(name.as_str())
                .strong()
                .size(14.0)
                .color(AppColors::BLUE),
        );
        if let Some(items) = analysis.responsibilities.get(name) {
            for item in items {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("\u{2022}")
                            .color(AppColors::TEXT_MUTED)
                            .size(13.0),
                    );
                    ui.label(
                        egui::RichText::new(item)
                            .color(AppColors::TEXT_PRIMARY)
                            .size(13.0),
                    );
                });
            }
        }
        ui.add_space(4.0);
    }
}

fn export_markdown_dialog(state: &mut ResultsState, analysis: &MeetingAnalysis) {
    let filename = markdown::get_filename(analysis);
    if let Some(path) = rfd::FileDialog::new()
        .set_file_name(&filename)
        .add_filter("Markdown", &["md"])
        .save_file()
    {
        let content = markdown::to_markdown(analysis);
        match std::fs::write(&path, content.as_bytes()) {
            Ok(()) => {
                state.export_status = format!("Saved: {}", path.display());
                state.export_error.clear();
            }
            Err(e) => {
                state.export_error = format!("Failed to save: {e}");
                state.export_status.clear();
            }
        }
    }
}

fn export_obsidian(state: &mut ResultsState, analysis: &MeetingAnalysis, vault_path: &str) {
    match obsidian::export_to_obsidian(analysis, vault_path) {
        Ok(path) => {
            state.export_status = format!("Saved to Obsidian: {path}");
            state.export_error.clear();
        }
        Err(e) => {
            state.export_error = format!("Obsidian export failed: {e}");
            state.export_status.clear();
        }
    }
}

#[cfg(feature = "notion")]
fn export_notion(state: &mut ResultsState, analysis: &MeetingAnalysis, config: &Config) {
    use crate::export::notion;
    match notion::export_to_notion(analysis, &config.notion_token, &config.notion_parent_page_id) {
        Ok(url) => {
            state.export_status = format!("Pushed to Notion: {url}");
            state.export_error.clear();
        }
        Err(e) => {
            state.export_error = format!("Notion export failed: {e}");
            state.export_status.clear();
        }
    }
}
