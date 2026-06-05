//! "Collect payment" modal shown when an advance is marked collected.
//!
//! Clicking "เก็บเงิน" on an outstanding advance sets
//! `AppState.pending_advance_collect` instead of collecting immediately; this
//! modal collects the real collection date and an optional note and, on
//! "บันทึก", calls `collect_advance` (which marks it collected and logs the
//! activity). Cancelling leaves the advance outstanding.

use egui_extras::DatePickerButton;

use crate::app::AppState;
use crate::db::queries::group_thousands;

/// An advance awaiting its collection date + note. Set by clicking "เก็บเงิน";
/// consumed by [`render`].
#[derive(Clone)]
pub struct PendingAdvanceCollect {
    pub id: i64,
    pub item: String,
    pub amount: i64,
    pub contact_name: String,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_advance_collect.clone() else {
        return;
    };

    let mut save = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("เก็บเงินค่าสินค้า / Collect Payment")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&pending.item).strong());
            ui.label(
                egui::RichText::new(format!(
                    "{} บาท · ของ: {}",
                    group_thousands(pending.amount),
                    pending.contact_name
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("วันที่เก็บ:");
                ui.add(
                    DatePickerButton::new(&mut app.advance_collect_date)
                        .id_source("advance_collect_picker"),
                );
            });
            ui.add_space(6.0);
            ui.label("หมายเหตุ:");
            ui.add(
                egui::TextEdit::multiline(&mut app.advance_collect_note)
                    .hint_text("เช่น โอนผ่านพร้อมเพย์ (ไม่บังคับ)")
                    .desired_width(360.0)
                    .desired_rows(2),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("💾 บันทึก").fill(crate::ui::ACCENT))
                    .clicked()
                {
                    save = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
            });
        });

    if save {
        match app
            .db
            .collect_advance(pending.id, app.advance_collect_date, &app.advance_collect_note)
        {
            Ok(()) => {
                app.set_status(format!("บันทึกการเก็บเงินจาก {} แล้ว", pending.contact_name));
                // Clear only on success — on error keep the dialog open with the
                // typed values preserved so the user can retry.
                app.pending_advance_collect = None;
                app.advance_collect_note.clear();
            }
            Err(e) => app.set_error(e),
        }
    } else if cancel || !open {
        app.pending_advance_collect = None;
        app.advance_collect_note.clear();
    }
}
