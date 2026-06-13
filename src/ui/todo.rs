//! Todo List: tasks to do — optionally for a specific contact — with a due date
//! and a done flag. Add/edit via a titled form group above the list; the table
//! supports status + contact-type filters and text search, toggling done in
//! place, and edit/delete per row. Overdue (unfinished, past due) shows red.

use chrono::{Local, NaiveDate};
use egui_extras::{Column, DatePickerButton, TableBuilder};

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::models::todo::Todo;
use crate::ui::confirm::PendingDelete;
use crate::ui::{filter_combo, ACCENT, ACCENT_STRONG};

/// Width of the fixed label column in each form/filter row; the field after it
/// fills the rest of its card's column.
const LABEL_W: f32 = 110.0;

/// One labelled form row: a fixed-width label cell, then the field widget. Laid
/// out manually (not via `egui::Grid`) so combo/date widgets don't under-report
/// their height — same reason as `ui/forms.rs`.
fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_W, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        add(ui);
    });
}

/// Add/edit form state for the Todo page.
pub struct TodoForm {
    /// `Some(id)` when editing an existing todo; `None` when adding.
    pub editing_id: Option<i64>,
    pub task: String,
    pub contact_id: Option<i64>,
    pub contact_filter: String,
    pub due_date: Option<NaiveDate>,
}

impl Default for TodoForm {
    fn default() -> Self {
        TodoForm {
            editing_id: None,
            task: String::new(),
            contact_id: None,
            contact_filter: String::new(),
            due_date: Some(Local::now().date_naive()),
        }
    }
}

impl TodoForm {
    fn reset(&mut self) {
        *self = TodoForm::default();
    }
}

/// Status filter on the Todo page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TodoStatusFilter {
    Pending,
    Overdue,
    Done,
    All,
}

impl TodoStatusFilter {
    const ALL: [TodoStatusFilter; 4] = [
        TodoStatusFilter::Pending,
        TodoStatusFilter::Overdue,
        TodoStatusFilter::Done,
        TodoStatusFilter::All,
    ];
    fn label(self) -> &'static str {
        match self {
            TodoStatusFilter::Pending => "ยังไม่เสร็จ",
            TodoStatusFilter::Overdue => "เลยกำหนด",
            TodoStatusFilter::Done => "เสร็จแล้ว",
            TodoStatusFilter::All => "ทั้งหมด",
        }
    }
}

/// Contact-type filter on the Todo page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TodoWhoFilter {
    All,
    Type(ContactType),
    Unassigned,
}

impl TodoWhoFilter {
    fn label(self) -> &'static str {
        match self {
            TodoWhoFilter::All => "ทั้งหมด",
            TodoWhoFilter::Type(t) => t.label_th(),
            TodoWhoFilter::Unassigned => "ไม่ระบุ",
        }
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("สิ่งที่ต้องทำ / Todo List");
    ui.label(
        egui::RichText::new("งานที่ต้องดำเนินการให้ผู้มุ่งหวัง / ลูกค้า VIP / นักธุรกิจ")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Contacts for the picker (all types), pre-fetched so the combo closure does
    // not borrow app.db while mutating app.todo_form. `list_contacts` hides the
    // me-row, so prepend "ฉัน (Me)" explicitly as the first option.
    let me_id = app.db.me_contact_id().ok();
    let contacts = app.db.list_contacts().unwrap_or_default();
    let mut contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();
    if let Some(mid) = me_id {
        contact_options.insert(0, (mid, "ฉัน (Me)".to_string()));
    }

    let mut submit = false; // add or save, depending on mode
    let mut cancel_edit = false;
    let editing = app.todo_form.editing_id.is_some();

    // --- add/edit form (left) and search/filter (right) as two equal cards ---
    ui.columns(2, |cols| {
        // Field widths from the real column width (both columns are equal) so each
        // card fills its column without overflowing it — measuring availability
        // deeper inside the nested frame/rows over-reports and overflows.
        let field_w = (cols[0].available_width() - LABEL_W - 40.0).max(60.0);
        let search_field_w = (field_w - 60.0).max(60.0); // room for the "ล้าง" button

        // Left card: add / edit form.
        let c0 = &mut cols[0];
        egui::Frame::group(c0.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c0, |ui| {
                let f = &mut app.todo_form;
                ui.label(
                    egui::RichText::new(if editing { "✏ แก้ไขงาน" } else { "➕ เพิ่มงานใหม่" })
                        .color(ACCENT_STRONG)
                        .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "สิ่งที่ต้องทำ", |ui| {
                    let w = field_w;
                    ui.add(
                        egui::TextEdit::singleline(&mut f.task)
                            .hint_text("เช่น โทรนัดดูสินค้า Nutrilite")
                            .desired_width(w),
                    );
                });
                field_row(ui, "เกี่ยวกับ", |ui| {
                    let w = field_w;
                    filter_combo(
                        ui,
                        "todo_contact_cb",
                        &mut f.contact_id,
                        &mut f.contact_filter,
                        Some("— ไม่ระบุ —"),
                        &contact_options,
                        w,
                    );
                });
                field_row(ui, "กำหนดส่ง", |ui| {
                    let mut has_due = f.due_date.is_some();
                    ui.checkbox(&mut has_due, "มีกำหนดส่ง");
                    if has_due {
                        let mut due = f.due_date.unwrap_or_else(|| Local::now().date_naive());
                        ui.add(DatePickerButton::new(&mut due).id_source("todo_due_picker"));
                        f.due_date = Some(due);
                    } else {
                        f.due_date = None;
                        ui.weak("ไม่มีกำหนด");
                    }
                });

                ui.add_space(8.0);
                field_row(ui, "", |ui| {
                    if editing {
                        if ui.add(egui::Button::new("💾 บันทึก").fill(ACCENT)).clicked() {
                            submit = true;
                        }
                        if ui.button("ยกเลิก").clicked() {
                            cancel_edit = true;
                        }
                    } else if ui.add(egui::Button::new("➕ เพิ่ม").fill(ACCENT)).clicked() {
                        submit = true;
                    }
                });
            });

        // Right card: search + filters — same aligned label/field rows as the form.
        let c1 = &mut cols[1];
        egui::Frame::group(c1.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c1, |ui| {
                ui.label(
                    egui::RichText::new("🔍 ค้นหา / กรอง")
                        .color(ACCENT_STRONG)
                        .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "ค้นหา", |ui| {
                    let w = search_field_w;
                    ui.add(
                        egui::TextEdit::singleline(&mut app.search)
                            .hint_text("งาน / ชื่อ")
                            .desired_width(w),
                    );
                    if ui.button("ล้าง").clicked() {
                        app.search.clear();
                    }
                });
                field_row(ui, "สถานะ", |ui| {
                    let w = field_w;
                    egui::ComboBox::from_id_source("todo_status_cb")
                        .width(w)
                        .selected_text(app.todo_status_filter.label())
                        .show_ui(ui, |ui| {
                            for s in TodoStatusFilter::ALL {
                                ui.selectable_value(&mut app.todo_status_filter, s, s.label());
                            }
                        });
                });
                field_row(ui, "ของ", |ui| {
                    let w = field_w;
                    egui::ComboBox::from_id_source("todo_who_cb")
                        .width(w)
                        .selected_text(app.todo_who_filter.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.todo_who_filter,
                                TodoWhoFilter::All,
                                TodoWhoFilter::All.label(),
                            );
                            for t in ContactType::ALL {
                                ui.selectable_value(
                                    &mut app.todo_who_filter,
                                    TodoWhoFilter::Type(t),
                                    t.label_th(),
                                );
                            }
                            ui.selectable_value(
                                &mut app.todo_who_filter,
                                TodoWhoFilter::Unassigned,
                                TodoWhoFilter::Unassigned.label(),
                            );
                        });
                });
            });
    });

    ui.add_space(6.0);

    // --- load + filter rows ---
    let r = app.db.list_todos(&app.search);
    let all_rows = app.handle(r, Vec::new());
    let today = Local::now().date_naive();
    let status = app.todo_status_filter;
    let who = app.todo_who_filter;
    let rows: Vec<_> = all_rows
        .into_iter()
        .filter(|row| match status {
            TodoStatusFilter::All => true,
            TodoStatusFilter::Pending => !row.todo.done,
            TodoStatusFilter::Done => row.todo.done,
            TodoStatusFilter::Overdue => row.todo.is_overdue(today),
        })
        .filter(|row| match who {
            TodoWhoFilter::All => true,
            TodoWhoFilter::Unassigned => row.todo.contact_id.is_none(),
            TodoWhoFilter::Type(t) => row.contact_type == Some(t),
        })
        .collect();

    ui.label(
        egui::RichText::new(format!("ทั้งหมด {} รายการ", rows.len()))
            .small()
            .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ไม่มีงานในตัวกรองนี้ —");
    } else {
        let mut toggle: Option<(i64, bool)> = None;
        let mut edit_req: Option<i64> = None;
        let mut delete_req: Option<(i64, String)> = None;
        let overdue_color = egui::Color32::from_rgb(0xD3, 0x2F, 0x2F);

        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::auto()) // เสร็จ
            .column(Column::auto().at_least(110.0)) // กำหนดส่ง
            .column(Column::remainder().at_least(200.0)) // สิ่งที่ต้องทำ
            .column(Column::auto().at_least(160.0)) // เกี่ยวกับ
            .column(Column::auto()) // จัดการ
            .header(28.0, |mut header| {
                for h in ["เสร็จ", "กำหนดส่ง", "สิ่งที่ต้องทำ", "เกี่ยวกับ", "จัดการ"] {
                    header.col(|ui| {
                        ui.strong(h);
                    });
                }
            })
            .body(|mut body| {
                for row in &rows {
                    let overdue = row.todo.is_overdue(today);
                    body.row(30.0, |mut tr| {
                        // done checkbox (persists immediately)
                        tr.col(|ui| {
                            let mut done = row.todo.done;
                            if ui.checkbox(&mut done, "").changed() {
                                toggle = Some((row.todo.id, done));
                            }
                        });
                        // due date
                        tr.col(|ui| match row.todo.due_date {
                            Some(due) => {
                                let text = due.format("%Y-%m-%d").to_string();
                                if overdue {
                                    ui.label(egui::RichText::new(text).color(overdue_color).strong());
                                } else {
                                    ui.label(egui::RichText::new(text).small());
                                }
                            }
                            None => {
                                ui.weak("—");
                            }
                        });
                        // task
                        tr.col(|ui| {
                            let mut rt = egui::RichText::new(row.todo.task.as_str());
                            if row.todo.done {
                                rt = rt.strikethrough().weak();
                            }
                            ui.label(rt);
                        });
                        // contact
                        tr.col(|ui| match (&row.contact_name, row.contact_type) {
                            (Some(name), Some(ty)) => {
                                let color = match ty {
                                    ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
                                    ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
                                    ContactType::Abo => ACCENT_STRONG,
                                };
                                ui.label(egui::RichText::new(name.as_str()).color(color));
                            }
                            _ => {
                                ui.weak("—");
                            }
                        });
                        // actions
                        tr.col(|ui| {
                            if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                                edit_req = Some(row.todo.id);
                            }
                            if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                                delete_req = Some((row.todo.id, row.todo.task.clone()));
                            }
                        });
                    });
                }
            });

        // --- apply deferred row actions ---
        if let Some((id, done)) = toggle {
            if !done {
                // un-tick: back to pending, history untouched
                if let Err(e) = app.db.set_todo_done(id, false) {
                    app.set_error(e);
                }
            } else if let Some(row) = rows.iter().find(|r| r.todo.id == id) {
                // Open the Log Result dialog. A contact-linked todo shows its
                // contact read-only; a contactless todo shows a contact picker.
                // Completion is deferred until "บันทึก".
                let contact_name = match (row.todo.contact_id, &row.contact_name) {
                    (Some(_), Some(name)) => Some(name.clone()),
                    _ => None,
                };
                app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone {
                    id,
                    task: row.todo.task.clone(),
                    contact_name,
                });
                app.todo_done_result.clear();
                app.todo_done_contact_id = None;
                app.todo_done_contact_filter.clear();
            }
        }
        if let Some(id) = edit_req {
            if let Some(row) = rows.iter().find(|r| r.todo.id == id) {
                app.todo_form = TodoForm {
                    editing_id: Some(row.todo.id),
                    task: row.todo.task.clone(),
                    contact_id: row.todo.contact_id,
                    contact_filter: String::new(),
                    due_date: row.todo.due_date,
                };
            }
        }
        if let Some((id, name)) = delete_req {
            app.pending_delete = Some(PendingDelete::Todo { id, name });
        }
    }

    // --- apply form submit / cancel ---
    if cancel_edit {
        app.todo_form.reset();
    }
    if submit {
        let editing = app.todo_form.editing_id;
        let result = match editing {
            Some(id) => {
                let t = Todo {
                    id,
                    contact_id: app.todo_form.contact_id,
                    task: app.todo_form.task.clone(),
                    due_date: app.todo_form.due_date,
                    // `update_todo` writes only contact_id/task/due_date, so
                    // these two are placeholders it ignores (never persisted).
                    done: false,
                    created_at: Local::now(),
                };
                app.db.update_todo(&t)
            }
            None => app
                .db
                .add_todo(app.todo_form.contact_id, &app.todo_form.task, app.todo_form.due_date)
                .map(|_| ()),
        };
        match result {
            Ok(()) => {
                app.todo_form.reset();
                app.set_status(if editing.is_some() { "บันทึกงานแล้ว" } else { "เพิ่มงานแล้ว" });
            }
            Err(e) => app.set_error(e),
        }
    }
}
