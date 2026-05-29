//! Prospect list (Sponsor List): searchable, score-sorted table with inline
//! sponsor-flow step and add/edit/delete/advance actions.

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::ui::forms::{self, ContactForm};
use crate::ui::{self, ACCENT};

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ผู้มุ่งหวัง / Prospects");
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("ค้นหา ชื่อ / เบอร์")
                .desired_width(240.0),
        );
        if ui.button("ล้าง").clicked() {
            app.search.clear();
        }
        ui.separator();
        if ui
            .add(egui::Button::new("➕ เพิ่มผู้มุ่งหวัง").fill(ACCENT))
            .clicked()
        {
            app.form = ContactForm::for_new_with_type(ContactType::Prospect);
        }
    });

    ui.add_space(8.0);

    let r = app.db.list_prospect_rows(&app.search);
    let rows = app.handle(r, Vec::new());
    if rows.is_empty() {
        ui.weak("— ไม่มีข้อมูลผู้มุ่งหวัง —");
        return;
    }

    let mut edit_id: Option<i64> = None;
    let mut delete_id: Option<i64> = None;
    let mut advance_id: Option<i64> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("prospect_table")
            .striped(true)
            .num_columns(5)
            .spacing([14.0, 8.0])
            .show(ui, |ui| {
                for h in ["ชื่อ", "เบอร์โทร", "คะแนน", "ขั้นตอน (Sponsor Flow)", "จัดการ"] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();

                for row in &rows {
                    ui.label(row.contact.display_name());
                    ui.label(row.contact.phone.clone().unwrap_or_default());
                    ui.label(
                        egui::RichText::new(row.score_total.to_string())
                            .color(ui::score_color(row.score_total, 20))
                            .strong(),
                    );
                    let step = row.current_step;
                    ui.label(
                        egui::RichText::new(format!("{} · {}", step.short(), step.label_th()))
                            .small(),
                    );
                    ui.horizontal(|ui| {
                        if ui.small_button("▶").on_hover_text("ขั้นต่อไป").clicked() {
                            advance_id = Some(row.contact.id);
                        }
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_id = Some(row.contact.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_id = Some(row.contact.id);
                        }
                    });
                    ui.end_row();
                }
            });
    });

    if let Some(id) = advance_id {
        advance_step(app, id);
    }
    if let Some(id) = edit_id {
        forms::open_edit(app, id);
    }
    if let Some(id) = delete_id {
        match app.db.delete_contact(id) {
            Ok(()) => app.set_status("ลบรายชื่อเรียบร้อย"),
            Err(e) => app.set_error(e),
        }
    }
}

/// Advance a prospect to the next sponsor-flow step (validated DB-side).
fn advance_step(app: &mut AppState, id: i64) {
    let flow = match app.db.get_sponsor_flow(id) {
        Ok(f) => f,
        Err(e) => {
            app.set_error(e);
            return;
        }
    };
    match flow.current_step.next() {
        Some(next) => match app.db.set_sponsor_step(id, next) {
            Ok(()) => app.set_status(format!("เลื่อนไป {} · {}", next.short(), next.label_th())),
            Err(e) => app.set_error(e),
        },
        None => app.set_status("อยู่ขั้นตอนสุดท้าย (Step 8) แล้ว"),
    }
}
