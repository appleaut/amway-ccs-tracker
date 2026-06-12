//! egui views and shared UI helpers.
//!
//! Each view is a free function `render(app, ui)` taking `&mut AppState` so the
//! main loop can dispatch on the active [`View`] without borrow gymnastics.

pub mod abo_list;
pub mod activities;
pub mod activity_kinds;
pub mod activity_log;
pub mod advance_collect;
pub mod advances;
pub mod confirm;
pub mod customer_list;
pub mod dashboard;
pub mod downline_tree;
pub mod followup;
pub mod forms;
pub mod meeting_form;
pub mod meetings;
pub mod promo_download;
pub mod settings_backup;
pub mod prospect_list;
pub mod rank_advisor;
pub mod todo;
pub mod todo_done;
pub mod todo_schedules;

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
    Meetings,
    Todos,
    TodoSchedules,
    Advances,
    PromoDownload,
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

/// Like [`metric_card`] but the whole card is clickable; returns the click
/// response so the caller can navigate on click.
pub fn metric_card_clickable(
    ui: &mut egui::Ui,
    title: &str,
    value: &str,
    accent: egui::Color32,
) -> egui::Response {
    egui::Frame::group(ui.style())
        .rounding(8.0)
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.set_min_width(150.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).size(13.0).weak());
                ui.add_space(4.0);
                ui.label(egui::RichText::new(value).size(30.0).strong().color(accent));
            });
        })
        .response
        .interact(egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A combo box whose popup carries a search field that filters the options —
/// egui's built-in `ComboBox` has no filtering, so this fills the gap.
///
/// `selected` is the chosen id (`None` shows `none_label`, when given, as both
/// the closed text and a top entry that clears the selection). `options` are
/// `(id, label)` pairs. `filter` holds the per-combo search text (kept in app
/// state so it survives the frames the popup is open).
pub fn filter_combo(
    ui: &mut egui::Ui,
    id_source: &str,
    selected: &mut Option<i64>,
    filter: &mut String,
    none_label: Option<&str>,
    options: &[(i64, String)],
    width: f32,
) {
    let popup_id = ui.make_persistent_id(id_source);

    let selected_text = match *selected {
        Some(id) => options
            .iter()
            .find(|(oid, _)| *oid == id)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| "—".to_string()),
        None => none_label.unwrap_or("—").to_string(),
    };

    // egui's `ComboBox` closes its popup on *any* click (CloseOnClick), so a
    // search field inside it is unusable. Build the popup by hand with
    // CloseOnClickOutside and close it ourselves once a choice is made.
    let combo_h = ui.spacing().interact_size.y.max(22.0);
    let button = combo_button(ui, &selected_text, width, combo_h);
    if button.clicked() {
        filter.clear();
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    let before = *selected;
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &button,
        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(width);
            let search = ui.add(
                egui::TextEdit::singleline(filter)
                    .hint_text("พิมพ์เพื่อค้นหา…")
                    .desired_width(f32::INFINITY),
            );
            // Focus the search field on open so the user can type immediately.
            if !search.has_focus() {
                search.request_focus();
            }
            ui.add_space(2.0);

            let needle = filter.trim().to_lowercase();
            let matches =
                |label: &str| needle.is_empty() || label.to_lowercase().contains(&needle);

            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                let mut shown = 0usize;
                if let Some(label) = none_label {
                    if matches(label) {
                        ui.selectable_value(selected, None, label);
                        shown += 1;
                    }
                }
                for (id, label) in options {
                    if matches(label) {
                        ui.selectable_value(selected, Some(*id), label.as_str());
                        shown += 1;
                    }
                }
                if shown == 0 {
                    ui.weak("— ไม่พบ —");
                }
            });
        },
    );

    // A choice was made → reset the filter and close the popup.
    if *selected != before {
        filter.clear();
        ui.memory_mut(|mem| mem.close_popup());
    }
}

/// Draw a combo-box-style control (framed, left-aligned text, right ▼ arrow)
/// sized to `width` × `height`, returning its click response. It allocates via
/// `allocate_exact_size`, so it vertically centres in a row exactly like a
/// `Button` — unlike `egui::ComboBox`, which can sit a few px off the line when
/// mixed with text boxes / buttons in one row.
pub(crate) fn combo_button(ui: &mut egui::Ui, text: &str, width: f32, height: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let visuals = ui.style().interact(&resp);
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect(rect, visuals.rounding, visuals.weak_bg_fill, visuals.bg_stroke);
    let font = egui::TextStyle::Button.resolve(ui.style());
    painter.text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        text,
        font.clone(),
        visuals.text_color(),
    );
    painter.text(
        rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        "▼",
        font,
        visuals.text_color(),
    );
    resp
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
