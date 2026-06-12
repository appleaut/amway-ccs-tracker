//! Amway CCS Prospect & Downline Tracker — Windows desktop app.
//!
//! Entry point: configure the native window and hand control to [`app::AppState`].

// Hide the console window in release builds (keep it in debug for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backup;
mod db;
mod error;
mod models;
mod promo;
mod ui;
mod utils;

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([960.0, 640.0])
        .with_title("Amway CCS Tracker");
    // Best-effort window icon (title bar / taskbar); never fail startup over it.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icons/app.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Amway CCS Tracker",
        options,
        Box::new(|cc| Ok(Box::new(app::AppState::new(cc)?) as Box<dyn eframe::App>)),
    )
}
