//! egui views and shared UI helpers.
//!
//! Each view is a free function `render(app, ui)` taking `&mut AppState` so the
//! main loop can dispatch on the active [`View`] without borrow gymnastics.

pub mod customer_list;
pub mod dashboard;
pub mod downline_tree;
pub mod followup;
pub mod forms;
pub mod prospect_list;

/// CCS brand accent (teal, #00BCD4).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xBC, 0xD4);

/// Which screen is currently shown in the central panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Prospects,
    Customers,
    FollowUp,
    Network,
    Settings,
}

/// A rounded "metric" card used on the dashboard.
pub fn metric_card(ui: &mut egui::Ui, title: &str, value: &str, accent: egui::Color32) {
    egui::Frame::group(ui.style())
        .rounding(8.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).size(13.0).weak());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(value)
                        .size(30.0)
                        .strong()
                        .color(accent),
                );
            });
        });
}

/// Colour a score relative to its "high" threshold: green at/above, amber from
/// half-way, otherwise muted.
pub fn score_color(total: u8, high: u8) -> egui::Color32 {
    if total >= high {
        egui::Color32::from_rgb(0x4C, 0xAF, 0x50)
    } else if (total as u16) * 2 >= high as u16 {
        egui::Color32::from_rgb(0xFF, 0xB3, 0x00)
    } else {
        egui::Color32::GRAY
    }
}
