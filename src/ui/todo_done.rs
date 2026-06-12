//! "Log result" modal shown when a todo is ticked complete.
//!
//! Ticking a todo sets `AppState.pending_todo_done` instead of marking it done
//! immediately; this modal collects a free-text result and, on "บันทึก",
//! completes the todo. For a contact-linked todo it logs the result to that
//! contact's history; for a contactless todo it shows a contact picker so the
//! user may choose a contact to log against (or none). Cancelling leaves the
//! todo unfinished.

use crate::app::AppState;

/// A todo whose done-toggle is awaiting its result text. Set by ticking a todo
/// done; consumed by [`render`]. `contact_name` is `Some` for a contact-linked
/// todo (shown read-only) and `None` for a contactless todo (a contact picker
/// is shown so the result can be logged against a chosen contact).
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: Option<String>,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_todo_done.clone() else {
        return;
    };

    // Contact options for the contactless picker, fetched once so the
    // filter_combo closure doesn't borrow app.db while mutating picker state.
    // Empty for a contact-linked todo (no picker shown).
    let contact_options: Vec<(i64, String)> = if pending.contact_name.is_none() {
        app.db
            .list_contacts()
            .unwrap_or_default()
            .iter()
            .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
            .collect()
    } else {
        Vec::new()
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
            match &pending.contact_name {
                Some(name) => {
                    ui.label(
                        egui::RichText::new(format!("ของ: {}", name))
                            .small()
                            .weak(),
                    );
                }
                None => {
                    ui.add_space(8.0);
                    ui.label("บันทึกประวัติของ:");
                    crate::ui::filter_combo(
                        ui,
                        "todo_done_contact_cb",
                        &mut app.todo_done_contact_id,
                        &mut app.todo_done_contact_filter,
                        Some("— ไม่บันทึกประวัติ —"),
                        &contact_options,
                        360.0,
                    );
                }
            }
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
        // Linked todo  -> log to its own contact (complete_todo).
        // Contactless + picked -> log to the chosen contact.
        // Contactless + none   -> just mark done, no history.
        let result = match &pending.contact_name {
            Some(_) => app.db.complete_todo(pending.id, &app.todo_done_result),
            None => match app.todo_done_contact_id {
                Some(cid) => {
                    app.db.complete_todo_to_contact(pending.id, cid, &app.todo_done_result)
                }
                None => app.db.set_todo_done(pending.id, true),
            },
        };
        match result {
            Ok(()) => {
                // Name for the success toast: the linked contact, or the picked one.
                let logged_name: Option<String> = match &pending.contact_name {
                    Some(name) => Some(name.clone()),
                    None => app.todo_done_contact_id.and_then(|cid| {
                        contact_options
                            .iter()
                            .find(|(id, _)| *id == cid)
                            .map(|(_, label)| label.clone())
                    }),
                };
                match logged_name {
                    Some(name) => app.set_status(format!("บันทึกลงประวัติของ {} แล้ว", name)),
                    None => app.set_status("ทำเครื่องหมายเสร็จแล้ว"),
                }
                // Clear only on success — on error the dialog stays open with
                // input preserved so the user can retry.
                app.pending_todo_done = None;
                app.todo_done_result.clear();
                app.todo_done_contact_id = None;
                app.todo_done_contact_filter.clear();
            }
            Err(e) => app.set_error(e),
        }
    } else if cancel || !open {
        // Cancelling aborts completion: the done flag was never persisted, so
        // the todo simply stays "ยังไม่เสร็จ".
        app.pending_todo_done = None;
        app.todo_done_result.clear();
        app.todo_done_contact_id = None;
        app.todo_done_contact_filter.clear();
    }
}
