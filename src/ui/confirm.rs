//! Shared "confirm delete" modal.
//!
//! Views don't delete immediately: clicking 🗑 stores the target in
//! `AppState.pending_delete`. This modal then asks for confirmation and performs
//! the delete (or cancels). One implementation serves every table.

use crate::app::AppState;

/// What a pending delete targets — set by clicking 🗑 in a table.
#[derive(Clone)]
pub enum PendingDelete {
    Contact { id: i64, name: String },
    ActivityKind { id: i64, name: String },
    Todo { id: i64, name: String },
    Advance { id: i64, item: String },
    Meeting { id: i64, name: String },
    Activity { id: i64, label: String },
    TodoSchedule { id: i64, name: String },
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_delete.clone() else {
        return;
    };

    // Per-target name + warning detail.
    let (name, detail) = match &pending {
        PendingDelete::Contact { name, .. } => (
            name.clone(),
            "ข้อมูลคะแนน / ติดตามผล / ประวัติการติดต่อที่เกี่ยวข้องจะถูกลบด้วย และกู้คืนไม่ได้"
                .to_string(),
        ),
        PendingDelete::ActivityKind { name, .. } => {
            let used = app.db.activity_kind_usage(name).unwrap_or(0);
            let detail = if used > 0 {
                format!(
                    "มีประวัติ {used} รายการที่ใช้ประเภทนี้ — รายการเดิมจะยังคงข้อความไว้ \
                     แต่ประเภทนี้จะหายจากตัวเลือก"
                )
            } else {
                "ประเภทนี้จะถูกลบออกจากตัวเลือก".to_string()
            };
            (name.clone(), detail)
        }
        PendingDelete::Todo { name, .. } => {
            (name.clone(), "งานนี้จะถูกลบถาวร".to_string())
        }
        PendingDelete::Advance { item, .. } => {
            (item.clone(), "รายการสำรองจ่ายนี้จะถูกลบถาวร".to_string())
        }
        PendingDelete::Meeting { name, .. } => {
            (name.clone(), "งานประชุมนี้และสถานะการเข้าร่วมทั้งหมดของงานนี้จะถูกลบถาวร".to_string())
        }
        PendingDelete::Activity { label, .. } => {
            (label.clone(), "ประวัติการติดต่อรายการนี้จะถูกลบถาวร".to_string())
        }
        PendingDelete::TodoSchedule { name, .. } => (
            name.clone(),
            "ตารางงานประจำนี้จะถูกลบถาวร (งานที่สร้างไปแล้วยังอยู่)".to_string(),
        ),
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
            ui.label(egui::RichText::new(detail).small().weak());
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
        let result = match &pending {
            PendingDelete::Contact { id, .. } => app.db.delete_contact(*id),
            PendingDelete::ActivityKind { id, .. } => app.db.delete_activity_kind(*id),
            PendingDelete::Todo { id, .. } => app.db.delete_todo(*id),
            PendingDelete::Advance { id, .. } => app.db.delete_advance(*id),
            PendingDelete::Meeting { id, .. } => app.db.delete_meeting(*id),
            PendingDelete::Activity { id, .. } => app.db.delete_activity(*id),
            PendingDelete::TodoSchedule { id, .. } => app.db.delete_todo_schedule(*id),
        };
        match result {
            Ok(()) => app.set_status(format!("ลบ \"{name}\" เรียบร้อย")),
            Err(e) => app.set_error(e),
        }
        app.pending_delete = None;
    } else if cancel || !open {
        app.pending_delete = None;
    }
}
