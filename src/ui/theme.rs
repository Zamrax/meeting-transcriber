use egui::{Color32, CornerRadius, FontId, Stroke, Style, TextStyle, Visuals};

pub struct AppColors;

impl AppColors {
    // Accent colors
    pub const GREEN: Color32 = Color32::from_rgb(76, 175, 80);
    pub const GREEN_DARK: Color32 = Color32::from_rgb(56, 142, 60);
    pub const RED: Color32 = Color32::from_rgb(239, 83, 80);
    pub const RED_DARK: Color32 = Color32::from_rgb(198, 40, 40);
    pub const BLUE: Color32 = Color32::from_rgb(66, 165, 245);
    pub const BLUE_DARK: Color32 = Color32::from_rgb(30, 136, 229);
    pub const AMBER: Color32 = Color32::from_rgb(255, 183, 77);

    // Surface colors
    pub const BG_BASE: Color32 = Color32::from_rgb(18, 18, 24);
    pub const BG_SURFACE: Color32 = Color32::from_rgb(28, 28, 36);
    pub const BG_ELEVATED: Color32 = Color32::from_rgb(38, 38, 48);
    pub const BG_CARD: Color32 = Color32::from_rgb(32, 32, 42);
    pub const BG_INPUT: Color32 = Color32::from_rgb(22, 22, 30);

    // Text colors
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(240, 240, 245);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 148, 168);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(100, 100, 120);

    // Borders
    pub const BORDER: Color32 = Color32::from_rgb(55, 55, 70);
    pub const BORDER_LIGHT: Color32 = Color32::from_rgb(70, 70, 88);

    // Recording indicator
    pub const PULSE_ON: Color32 = Color32::from_rgb(239, 83, 80);
    pub const PULSE_OFF: Color32 = Color32::from_rgb(80, 80, 100);
}

/// Apply a polished dark theme.
pub fn apply_dark_theme(ctx: &egui::Context) {
    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    // Main surfaces
    visuals.panel_fill = AppColors::BG_BASE;
    visuals.window_fill = AppColors::BG_ELEVATED;
    visuals.window_stroke = Stroke::new(1.0, AppColors::BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);

    // Widget styles
    visuals.widgets.noninteractive.bg_fill = AppColors::BG_CARD;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, AppColors::TEXT_PRIMARY);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, AppColors::BORDER);

    visuals.widgets.inactive.bg_fill = AppColors::BG_ELEVATED;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, AppColors::TEXT_PRIMARY);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.5, AppColors::BORDER);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(50, 50, 65);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, AppColors::BORDER_LIGHT);

    visuals.widgets.active.bg_fill = Color32::from_rgb(60, 60, 78);
    visuals.widgets.active.corner_radius = CornerRadius::same(6);

    visuals.selection.bg_fill = AppColors::BLUE.linear_multiply(0.25);
    visuals.selection.stroke = Stroke::new(1.0, AppColors::BLUE);

    // Separators
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(0.5, AppColors::BORDER);

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);

    // Typography
    let mut text_styles = std::collections::BTreeMap::new();
    text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
    text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
    text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
    text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
    text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
    style.text_styles = text_styles;

    ctx.set_style(style);
}

/// Draw a styled section card.
pub fn section_frame(ui: &mut egui::Ui, title: &str, add_body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(AppColors::BG_SURFACE)
        .stroke(Stroke::new(1.0, AppColors::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(16.0)
                    .strong()
                    .color(AppColors::TEXT_PRIMARY),
            );
            ui.add_space(10.0);
            add_body(ui);
        });
}

/// Create a styled primary button (colored fill).
pub fn primary_button(text: &str, color: Color32) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(Color32::WHITE)
            .strong()
            .size(14.0),
    )
    .fill(color)
    .corner_radius(CornerRadius::same(6))
    .min_size(egui::vec2(0.0, 34.0))
}

/// Create a styled secondary/outline button.
pub fn secondary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(text)
            .color(AppColors::TEXT_PRIMARY)
            .size(13.0),
    )
    .fill(AppColors::BG_ELEVATED)
    .stroke(Stroke::new(1.0, AppColors::BORDER_LIGHT))
    .corner_radius(CornerRadius::same(6))
    .min_size(egui::vec2(0.0, 30.0))
}
