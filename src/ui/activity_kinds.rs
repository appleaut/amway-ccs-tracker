//! Manage activity types (CRUD): the named kinds shown in the activity-log
//! dropdown and the history filter. Stored in the `activity_kinds` table.
//! Renaming a type relabels existing activity rows; deleting one leaves past
//! activities' text intact (it just disappears from the dropdown).

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::ui::confirm::PendingDelete;
use crate::ui::ACCENT;

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ประเภทกิจกรรม / Activity Types");
    ui.label(
        egui::RichText::new("ประเภทที่ใช้บันทึกประวัติการติดต่อ — เพิ่ม / แก้ไข / ลบ ได้")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // --- add / rename form ---
    let editing = app.kind_edit;
    let mut add = false;
    let mut save = false;
    let mut cancel_edit = false;

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.kind_draft)
                .hint_text("ชื่อประเภท เช่น สาธิตสินค้า")
                .desired_width(260.0),
        );
        if editing.is_some() {
            if ui.add(egui::Button::new("💾 บันทึก").fill(ACCENT)).clicked() {
                save = true;
            }
            if ui.button("ยกเลิก").clicked() {
                cancel_edit = true;
            }
        } else if ui.add(egui::Button::new("➕ เพิ่ม").fill(ACCENT)).clicked() {
            add = true;
        }
    });

    ui.add_space(8.0);

    let r = app.db.list_activity_kinds();
    let kinds = app.handle(r, Vec::new());

    let mut edit_req: Option<(i64, String)> = None;
    let mut delete_req: Option<(i64, String)> = None;

    if kinds.is_empty() {
        ui.weak("— ยังไม่มีประเภทกิจกรรม —");
    } else {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(220.0)) // ชื่อประเภท
            .column(Column::auto()) // จัดการ
            .header(28.0, |mut header| {
                header.col(|ui| {
                    ui.strong("ชื่อประเภท");
                });
                header.col(|ui| {
                    ui.strong("จัดการ");
                });
            })
            .body(|mut body| {
                for k in &kinds {
                    body.row(30.0, |mut tr| {
                        tr.col(|ui| {
                            ui.label(k.name.as_str());
                        });
                        tr.col(|ui| {
                            if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                                edit_req = Some((k.id, k.name.clone()));
                            }
                            if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                                delete_req = Some((k.id, k.name.clone()));
                            }
                        });
                    });
                }
            });
    }

    // --- apply deferred actions ---
    if let Some((id, name)) = edit_req {
        app.kind_edit = Some(id);
        app.kind_draft = name;
    }
    if let Some((id, name)) = delete_req {
        app.pending_delete = Some(PendingDelete::ActivityKind { id, name });
    }
    if add {
        let name = app.kind_draft.trim().to_string();
        match app.db.add_activity_kind(&name) {
            Ok(_) => {
                app.kind_draft.clear();
                app.set_status("เพิ่มประเภทกิจกรรมแล้ว");
            }
            Err(e) => app.set_error(e),
        }
    }
    if save {
        if let Some(id) = app.kind_edit {
            let name = app.kind_draft.trim().to_string();
            match app.db.rename_activity_kind(id, &name) {
                Ok(()) => {
                    app.kind_edit = None;
                    app.kind_draft.clear();
                    app.set_status("บันทึกการแก้ไขแล้ว");
                }
                Err(e) => app.set_error(e),
            }
        }
    }
    if cancel_edit {
        app.kind_edit = None;
        app.kind_draft.clear();
    }
}
