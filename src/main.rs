// Hide the console window on Windows release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod export;
mod gemini;
mod schema;
mod ui;

/// Load the app icon from the embedded ICO file.
/// Returns None if the icon cannot be loaded (non-fatal).
fn load_icon() -> Option<egui::IconData> {
    let ico_bytes = include_bytes!("../meeting-transcriber.ico");
    let image = image::load_from_memory(ico_bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn main() -> eframe::Result<()> {
    env_logger::init();
    let _ = dotenvy::dotenv();

    let icon = load_icon();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Meeting Transcriber")
        .with_inner_size([1100.0, 800.0])
        .with_min_inner_size([960.0, 700.0]);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Meeting Transcriber",
        options,
        Box::new(|cc| Ok(Box::new(ui::app::MeetingTranscriberApp::new(cc)))),
    )
}
