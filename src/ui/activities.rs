//! Aggregate activity-history view: every logged interaction across all
//! contacts (prospects, customers, ABOs) in one timeline, newest first, with a
//! text search and a kind filter. Per-row actions jump to that contact's
//! activity-log modal or delete the entry.

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::ui::ACCENT_STRONG;

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ประวัติการติดต่อทั้งหมด / Activity History");
    ui.add_space(6.0);

    let kinds = app.db.list_activity_kinds().unwrap_or_default();

    ui.horizontal(|ui| {
        // The text box, button and kind dropdown have different intrinsic heights
        // and egui lines them up inconsistently in one row. Pick a shared height
        // (the button height) and build all three to it so they sit centred on the
        // same line.
        let ctrl_h =
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("ค้นหา ชื่อ / รายละเอียด")
                .desired_width(240.0)
                .min_size(egui::vec2(0.0, ctrl_h))
                .vertical_align(egui::Align::Center),
        );
        if ui.button("ล้าง").clicked() {
            app.search.clear();
        }
        ui.separator();
        ui.label("ประเภทกิจกรรม:");
        let selected = app
            .history_kind
            .clone()
            .unwrap_or_else(|| "ทั้งหมด".to_string());
        // egui::ComboBox draws through an internal frame that lands a few px below
        // the row's other controls here; combo_button allocates like the button, so
        // it centres on the line. Pair it with a popup kind picker.
        let popup_id = ui.make_persistent_id("history_kind_popup");
        let button = crate::ui::combo_button(ui, &selected, 200.0, ctrl_h);
        if button.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup_id));
        }
        egui::popup::popup_below_widget(
            ui,
            popup_id,
            &button,
            egui::popup::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(200.0);
                if ui
                    .selectable_label(app.history_kind.is_none(), "ทั้งหมด")
                    .clicked()
                {
                    app.history_kind = None;
                }
                for k in &kinds {
                    let is_sel = app.history_kind.as_deref() == Some(k.name.as_str());
                    if ui.selectable_label(is_sel, k.name.as_str()).clicked() {
                        app.history_kind = Some(k.name.clone());
                    }
                }
            },
        );
    });

    ui.add_space(8.0);

    let r = app.db.list_all_activities(&app.search);
    let mut rows = app.handle(r, Vec::new());
    if let Some(k) = &app.history_kind {
        rows.retain(|row| &row.activity.kind == k);
    }

    ui.label(
        egui::RichText::new(format!("ทั้งหมด {} รายการ", rows.len()))
            .small()
            .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ยังไม่มีประวัติการติดต่อ —");
        return;
    }

    let mut open_contact: Option<i64> = None;
    let mut delete_id: Option<i64> = None;

    // The รายละเอียด column wraps long notes, so each row must be tall enough to
    // show every wrapped line. Resolve the body font and one-line height now,
    // before the table borrows `ui`; inside the body closure we lay out each note
    // at the detail column's real width to derive that row's height.
    let base_row_h = 30.0_f32;
    let body_font = egui::TextStyle::Body.resolve(ui.style());
    let line_h = ui.text_style_height(&egui::TextStyle::Body);
    let ctx = ui.ctx().clone();

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(140.0)) // วันเวลา
        .column(Column::auto().at_least(150.0)) // ชื่อ
        .column(Column::auto().at_least(80.0)) // ประเภท
        .column(Column::auto().at_least(120.0)) // กิจกรรม
        .column(Column::remainder().at_least(160.0)) // รายละเอียด
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            for h in ["วันเวลา", "ชื่อ", "ประเภท", "กิจกรรม", "รายละเอียด", "จัดการ"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            // Detail is column 4 (วันเวลา, ชื่อ, ประเภท, กิจกรรม, รายละเอียด, จัดการ);
            // widths() gives its resolved width so a wrapped note's height matches.
            let detail_w = body.widths().get(4).copied().unwrap_or(160.0);
            let row_pad = (base_row_h - line_h).max(0.0);
            for row in &rows {
                let h = if row.activity.note.is_empty() {
                    base_row_h
                } else {
                    let galley = ctx.fonts(|f| {
                        f.layout(
                            row.activity.note.clone(),
                            body_font.clone(),
                            egui::Color32::WHITE, // colour is irrelevant; we only read the height
                            detail_w,
                        )
                    });
                    (galley.size().y + row_pad).max(base_row_h)
                };
                body.row(h, |mut tr| {
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(
                                row.activity.created_at.format("%Y-%m-%d %H:%M").to_string(),
                            )
                            .small()
                            .weak(),
                        );
                    });
                    tr.col(|ui| {
                        ui.label(&row.contact_name);
                    });
                    tr.col(|ui| {
                        let color = match row.contact_type {
                            ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
                            ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
                            ContactType::Abo => ACCENT_STRONG,
                        };
                        ui.label(
                            egui::RichText::new(row.contact_type.label_th())
                                .small()
                                .color(color),
                        );
                    });
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.activity.kind.as_str())
                                .color(ACCENT_STRONG)
                                .strong(),
                        );
                    });
                    tr.col(|ui| {
                        if row.activity.note.is_empty() {
                            ui.weak("—");
                        } else {
                            ui.add(egui::Label::new(&row.activity.note).wrap());
                        }
                    });
                    tr.col(|ui| {
                        if ui
                            .small_button("📝")
                            .on_hover_text("เปิดประวัติของรายชื่อนี้")
                            .clicked()
                        {
                            open_contact = Some(row.contact_id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบรายการนี้").clicked() {
                            delete_id = Some(row.activity.id);
                        }
                    });
                });
            }
        });

    if let Some(cid) = open_contact {
        app.activity_contact = Some(cid);
        app.activity_note.clear();
    }
    if let Some(aid) = delete_id {
        match app.db.delete_activity(aid) {
            Ok(()) => app.set_status("ลบประวัติแล้ว"),
            Err(e) => app.set_error(e),
        }
    }
}
