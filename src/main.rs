//! Amway CCS Prospect & Downline Tracker — Windows desktop app.
//!
//! Entry point: configure the native window and hand control to [`app::AppState`].

// Hide the console window in release builds (keep it in debug for logs).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod error;
mod models;
mod ui;
mod utils;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Amway CCS Tracker"),
        ..Default::default()
    };

    eframe::run_native(
        "Amway CCS Tracker",
        options,
        Box::new(|cc| Ok(Box::new(app::AppState::new(cc)?) as Box<dyn eframe::App>)),
    )
}
