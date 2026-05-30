//! Prospect list (Sponsor List): searchable, score-sorted table with inline
//! sponsor-flow step and add/edit/delete/advance actions.

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::models::enums::{ContactType, SponsorStep};
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
    let mut rows = app.handle(r, Vec::new());
    if rows.is_empty() {
        ui.weak("— ไม่มีข้อมูลผู้มุ่งหวัง —");
        return;
    }

    let mut sort = app.prospect_sort;
    match sort.col {
        0 => rows.sort_by(|a, b| {
            a.contact
                .display_name()
                .to_lowercase()
                .cmp(&b.contact.display_name().to_lowercase())
        }),
        1 => rows.sort_by(|a, b| a.contact.phone.cmp(&b.contact.phone)),
        2 => rows.sort_by(|a, b| a.score_total.cmp(&b.score_total)),
        3 => rows.sort_by(|a, b| a.current_step.as_int().cmp(&b.current_step.as_int())),
        _ => {}
    }
    if !sort.ascending {
        rows.reverse();
    }

    let mut edit_id: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;
    let mut advance_id: Option<i64> = None;
    let mut step_change: Option<(i64, SponsorStep)> = None;
    let mut sort_clicked: Option<usize> = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(160.0)) // ชื่อ
        .column(Column::auto().at_least(110.0)) // เบอร์โทร
        .column(Column::auto()) // คะแนน
        .column(Column::remainder().at_least(200.0)) // ขั้นตอน
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            let cols: [(&str, Option<usize>); 5] = [
                ("ชื่อ", Some(0)),
                ("เบอร์โทร", Some(1)),
                ("คะแนน", Some(2)),
                ("ขั้นตอน (Sponsor Flow)", Some(3)),
                ("จัดการ", None),
            ];
            for (label, col) in cols {
                header.col(|ui| match col {
                    Some(c) => {
                        let txt = format!("{label}{}", sort.arrow(c));
                        if ui
                            .add(egui::Button::new(egui::RichText::new(txt).strong()).frame(false))
                            .clicked()
                        {
                            sort_clicked = Some(c);
                        }
                    }
                    None => {
                        ui.strong(label);
                    }
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(row.contact.display_name());
                    });
                    tr.col(|ui| {
                        ui.label(row.contact.phone.clone().unwrap_or_default());
                    });
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.score_total.to_string())
                                .color(ui::score_color(row.score_total, 20))
                                .strong(),
                        );
                    });
                    tr.col(|ui| {
                        let mut chosen = row.current_step;
                        egui::ComboBox::from_id_source(("flowstep", row.contact.id))
                            .selected_text(format!("{} · {}", chosen.short(), chosen.label_th()))
                            .show_ui(ui, |ui| {
                                for s in SponsorStep::ALL {
                                    ui.selectable_value(
                                        &mut chosen,
                                        s,
                                        format!("{} · {}", s.short(), s.label_th()),
                                    );
                                }
                            });
                        if chosen != row.current_step {
                            step_change = Some((row.contact.id, chosen));
                        }
                    });
                    tr.col(|ui| {
                        if ui.small_button("▶").on_hover_text("ขั้นต่อไป").clicked() {
                            advance_id = Some(row.contact.id);
                        }
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_id = Some(row.contact.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.contact.id, row.contact.display_name()));
                        }
                    });
                });
            }
        });

    if let Some(c) = sort_clicked {
        sort.toggle(c);
        app.prospect_sort = sort;
    }
    if let Some(id) = advance_id {
        advance_step(app, id);
    }
    if let Some((id, step)) = step_change {
        match app.db.set_sponsor_step_direct(id, step) {
            Ok(()) => app.set_status(format!("ตั้งขั้นตอนเป็น {} · {}", step.short(), step.label_th())),
            Err(e) => app.set_error(e),
        }
    }
    if let Some(id) = edit_id {
        forms::open_edit(app, id);
    }
    if let Some(req) = delete_req {
        app.pending_delete = Some(req);
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
