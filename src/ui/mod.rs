//! egui views and shared UI helpers.
//!
//! Each view is a free function `render(app, ui)` taking `&mut AppState` so the
//! main loop can dispatch on the active [`View`] without borrow gymnastics.

pub mod abo_list;
pub mod activities;
pub mod activity_kinds;
pub mod activity_log;
pub mod confirm;
pub mod customer_list;
pub mod dashboard;
pub mod downline_tree;
pub mod followup;
pub mod forms;
pub mod prospect_list;
pub mod rank_advisor;

/// CCS brand accent (teal, #00BCD4). Used for widget fills / selection tint.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xBC, 0xD4);

/// Darker teal (#00838F) for accent *text* — readable on the light background.
pub const ACCENT_STRONG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x83, 0x8F);

/// Per-table sort state: which column index, and direction.
#[derive(Clone, Copy)]
pub struct SortSpec {
    pub col: usize,
    pub ascending: bool,
}

impl SortSpec {
    pub fn new(col: usize, ascending: bool) -> Self {
        SortSpec { col, ascending }
    }

    /// Sort-direction arrow to append to a header label for `col` (empty if this
    /// column is not the active sort key).
    pub fn arrow(&self, col: usize) -> &'static str {
        if self.col == col {
            if self.ascending {
                "  ▲"
            } else {
                "  ▼"
            }
        } else {
            ""
        }
    }

    /// Handle a click on `col`: flip direction if it's the active column,
    /// otherwise switch to it (ascending).
    pub fn toggle(&mut self, col: usize) {
        if self.col == col {
            self.ascending = !self.ascending;
        } else {
            self.col = col;
            self.ascending = true;
        }
    }
}

/// Which screen is currently shown in the central panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Prospects,
    Customers,
    Abos,
    FollowUp,
    Network,
    Activities,
    ActivityKinds,
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
        egui::Color32::from_rgb(0x2E, 0x7D, 0x32) // green 800
    } else if (total as u16) * 2 >= high as u16 {
        egui::Color32::from_rgb(0xB2, 0x6A, 0x00) // dark amber
    } else {
        egui::Color32::from_rgb(0x61, 0x61, 0x61) // gray 700
    }
}
