//! Shared "confirm delete" modal.
//!
//! List views don't delete immediately: clicking 🗑 stores the target in
//! `AppState.pending_delete`. This modal then asks for confirmation and performs
//! the delete (or cancels). One implementation serves every table.

use crate::app::AppState;

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some((id, name)) = app.pending_delete.clone() else {
        return;
    };

    let mut confirm = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("ยืนยันการลบ")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(format!("ต้องการลบ \"{name}\" ใช่หรือไม่?"));
            ui.label(
                egui::RichText::new(
                    "ข้อมูลคะแนน / ติดตามผล / สถานะที่เกี่ยวข้องจะถูกลบด้วย และกู้คืนไม่ได้",
                )
                .small()
                .weak(),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("🗑 ลบ").color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(0xD3, 0x2F, 0x2F)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        match app.db.delete_contact(id) {
            Ok(()) => app.set_status(format!("ลบ \"{name}\" เรียบร้อย")),
            Err(e) => app.set_error(e),
        }
        app.pending_delete = None;
    } else if cancel || !open {
        app.pending_delete = None;
    }
}
