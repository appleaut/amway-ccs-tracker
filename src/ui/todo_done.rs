//! "Log result" modal shown when a contact-linked todo is ticked complete.
//!
//! Ticking such a todo sets `AppState.pending_todo_done` instead of marking it
//! done immediately; this modal collects a free-text result and, on "บันทึก",
//! calls `complete_todo` (which marks the todo done and logs the activity).
//! Cancelling leaves the todo unfinished.

use crate::app::AppState;

/// A todo whose done-toggle is awaiting its result text. Set by ticking a
/// contact-linked todo done; consumed by [`render`].
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: String,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_todo_done.clone() else {
        return;
    };

    let mut save = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("บันทึกผลลัพธ์ / Log Result")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&pending.task).strong());
            ui.label(
                egui::RichText::new(format!("ของ: {}", pending.contact_name))
                    .small()
                    .weak(),
            );
            ui.add_space(8.0);
            ui.label("ผลลัพธ์:");
            ui.add(
                egui::TextEdit::multiline(&mut app.todo_done_result)
                    .hint_text("ผลลัพธ์ของงานนี้ (ไม่บังคับ)")
                    .desired_width(360.0)
                    .desired_rows(3),
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
        match app.db.complete_todo(pending.id, &app.todo_done_result) {
            Ok(()) => {
                app.set_status(format!("บันทึกลงประวัติของ {} แล้ว", pending.contact_name));
                // Clear only on success — on error the dialog stays open with the
                // typed result preserved so the user can retry.
                app.pending_todo_done = None;
                app.todo_done_result.clear();
            }
            Err(e) => app.set_error(e),
        }
    } else if cancel || !open {
        // Cancelling aborts completion: the done flag was never persisted, so
        // the todo simply stays "ยังไม่เสร็จ".
        app.pending_todo_done = None;
        app.todo_done_result.clear();
    }
}
