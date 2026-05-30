//! Activity history modal — view, add, and delete logged interactions
//! (สาธิตสินค้า / บอกโปรโมชั่น / พูดแผน / …) for any contact.

use crate::app::AppState;
use crate::models::enums::ActivityKind;
use crate::ui::{ACCENT, ACCENT_STRONG};

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(id) = app.activity_contact else {
        return;
    };

    let contact = match app.db.get_contact(id) {
        Ok(c) => c,
        Err(e) => {
            app.set_error(e);
            app.activity_contact = None;
            return;
        }
    };
    let activities = app.db.list_activities(id).unwrap_or_default();

    let mut open = true;
    let mut add = false;
    let mut close = false;
    let mut delete_id: Option<i64> = None;

    egui::Window::new("ประวัติการติดต่อ / Activity Log")
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(contact.display_name()).size(18.0).strong());
            ui.add_space(6.0);

            // --- add a new entry ---
            ui.horizontal(|ui| {
                ui.label("ประเภท:");
                egui::ComboBox::from_id_source("activity_kind")
                    .selected_text(app.activity_kind.label_th())
                    .show_ui(ui, |ui| {
                        for k in ActivityKind::ALL {
                            ui.selectable_value(&mut app.activity_kind, k, k.label_th());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.activity_note)
                        .hint_text("รายละเอียด (ไม่บังคับ)")
                        .desired_width(300.0),
                );
                if ui.add(egui::Button::new("➕ เพิ่ม").fill(ACCENT)).clicked() {
                    add = true;
                }
            });

            ui.add_space(6.0);
            ui.separator();

            // --- timeline ---
            if activities.is_empty() {
                ui.weak("— ยังไม่มีประวัติ —");
            } else {
                egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    for a in &activities {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    a.created_at.format("%Y-%m-%d %H:%M").to_string(),
                                )
                                .small()
                                .weak(),
                            );
                            ui.label(
                                egui::RichText::new(a.kind.label_th())
                                    .color(ACCENT_STRONG)
                                    .strong(),
                            );
                            if !a.note.is_empty() {
                                ui.label(format!("— {}", a.note));
                            }
                            if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                                delete_id = Some(a.id);
                            }
                        });
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            if ui.button("ปิด").clicked() {
                close = true;
            }
        });

    if add {
        let note = app.activity_note.trim().to_string();
        let kind = app.activity_kind;
        match app.db.add_activity(id, kind, &note) {
            Ok(_) => {
                app.activity_note.clear();
                app.set_status("บันทึกประวัติแล้ว");
            }
            Err(e) => app.set_error(e),
        }
    }
    if let Some(aid) = delete_id {
        if let Err(e) = app.db.delete_activity(aid) {
            app.set_error(e);
        }
    }
    if close || !open {
        app.activity_contact = None;
    }
}
