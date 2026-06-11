//! ตารางงานประจำ / Recurring task schedules. Define a template (task + optional
//! contact) plus a cadence ("ทุก N วัน" or "รายเดือน วันที่กำหนด"); the app
//! auto-creates a normal todo on the "สิ่งที่ต้องทำ" page when a cycle is due
//! (see `AppState::update` → `db.generate_due_todos`). Add/edit on the left,
//! a how-it-works note on the right, then a table with edit/delete per row.

use chrono::{Local, NaiveDate};
use egui_extras::{Column, DatePickerButton, TableBuilder};

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::models::todo_schedule::{Recurrence, TodoSchedule};
use crate::ui::confirm::PendingDelete;
use crate::ui::{filter_combo, ACCENT, ACCENT_STRONG};

/// Width of the fixed label column in each form row (mirrors `ui/todo.rs`).
const LABEL_W: f32 = 110.0;

/// One labelled form row: a fixed-width label cell, then the field widget.
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

/// Add/edit form state for the recurring-schedule page. The cadence is split
/// into a boolean mode + two value buffers so toggling between kinds keeps each
/// kind's last-typed value.
pub struct TodoScheduleForm {
    /// `Some(id)` when editing an existing schedule; `None` when adding.
    pub editing_id: Option<i64>,
    pub task: String,
    pub contact_id: Option<i64>,
    pub contact_filter: String,
    /// `false` = ทุก N วัน (EveryNDays); `true` = รายเดือน (MonthlyDay).
    pub monthly: bool,
    pub every_n_days: i64,
    pub month_day: i64,
    pub start_date: NaiveDate,
}

impl Default for TodoScheduleForm {
    fn default() -> Self {
        TodoScheduleForm {
            editing_id: None,
            task: String::new(),
            contact_id: None,
            contact_filter: String::new(),
            monthly: false,
            every_n_days: 7,
            month_day: 1,
            start_date: Local::now().date_naive(),
        }
    }
}

impl TodoScheduleForm {
    fn reset(&mut self) {
        *self = TodoScheduleForm::default();
    }

    /// Build the `Recurrence` from the current mode + value buffers.
    fn recurrence(&self) -> Recurrence {
        if self.monthly {
            Recurrence::MonthlyDay(self.month_day.clamp(1, 31) as u32)
        } else {
            Recurrence::EveryNDays(self.every_n_days.max(1) as u32)
        }
    }

    /// Populate the form from an existing schedule for editing.
    fn edit_from(s: &TodoSchedule) -> Self {
        let (monthly, every_n_days, month_day) = match s.recurrence {
            Recurrence::EveryNDays(n) => (false, n as i64, 1),
            Recurrence::MonthlyDay(d) => (true, 7, d as i64),
        };
        TodoScheduleForm {
            editing_id: Some(s.id),
            task: s.task.clone(),
            contact_id: s.contact_id,
            contact_filter: String::new(),
            monthly,
            every_n_days,
            month_day,
            start_date: s.start_date,
        }
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ตารางงานประจำ / Recurring Tasks");
    ui.label(
        egui::RichText::new("ตั้งรอบให้ระบบสร้างงานใน \"สิ่งที่ต้องทำ\" อัตโนมัติเมื่อถึงกำหนด")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Contacts for the picker, pre-fetched so the combo closure does not borrow
    // app.db while mutating app.todo_schedule_form.
    let contacts = app.db.list_contacts().unwrap_or_default();
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();

    let mut submit = false;
    let mut cancel_edit = false;
    let editing = app.todo_schedule_form.editing_id.is_some();

    ui.columns(2, |cols| {
        let field_w = (cols[0].available_width() - LABEL_W - 40.0).max(60.0);

        // Left card: add / edit form.
        let c0 = &mut cols[0];
        egui::Frame::group(c0.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c0, |ui| {
                let f = &mut app.todo_schedule_form;
                ui.label(
                    egui::RichText::new(if editing {
                        "✏ แก้ไขตารางงาน"
                    } else {
                        "➕ เพิ่มตารางงานใหม่"
                    })
                    .color(ACCENT_STRONG)
                    .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "สิ่งที่ต้องทำ", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.task)
                            .hint_text("เช่น โทรติดตามผลลูกค้า")
                            .desired_width(field_w),
                    );
                });
                field_row(ui, "เกี่ยวกับ", |ui| {
                    filter_combo(
                        ui,
                        "schedule_contact_cb",
                        &mut f.contact_id,
                        &mut f.contact_filter,
                        Some("— ไม่ระบุ —"),
                        &contact_options,
                        field_w,
                    );
                });
                field_row(ui, "รอบ", |ui| {
                    egui::ComboBox::from_id_source("schedule_freq_cb")
                        .width(field_w)
                        .selected_text(if f.monthly { "รายเดือน (วันที่กำหนด)" } else { "ทุก N วัน" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.monthly, false, "ทุก N วัน");
                            ui.selectable_value(&mut f.monthly, true, "รายเดือน (วันที่กำหนด)");
                        });
                });
                if f.monthly {
                    field_row(ui, "วันที่ของเดือน", |ui| {
                        ui.add(egui::DragValue::new(&mut f.month_day).range(1..=31));
                        ui.weak("(วันที่ 29–31 จะปัดเป็นวันสุดท้ายของเดือนสั้น)");
                    });
                } else {
                    field_row(ui, "ทุกกี่วัน", |ui| {
                        ui.add(egui::DragValue::new(&mut f.every_n_days).range(1..=365).suffix(" วัน"));
                    });
                }
                field_row(ui, "วันเริ่ม", |ui| {
                    ui.add(DatePickerButton::new(&mut f.start_date).id_source("schedule_start_picker"));
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

        // Right card: how-it-works note.
        let c1 = &mut cols[1];
        egui::Frame::group(c1.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c1, |ui| {
                ui.label(egui::RichText::new("ℹ วิธีทำงาน").color(ACCENT_STRONG).strong());
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "เมื่อถึงหรือเลยวันที่ตามรอบ ระบบจะสร้างงานใน \"สิ่งที่ต้องทำ\" \
                         ให้อัตโนมัติ (กำหนดส่ง = วันของรอบ) ตอนเปิดแอปหรือเมื่อข้ามวัน",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "ถ้าเลยมาหลายรอบ จะสร้างเพียงงานเดียว (รอบล่าสุด) • \
                         ลบตารางที่นี่ไม่ลบงานที่สร้างไปแล้ว",
                    )
                    .small()
                    .weak(),
                );
            });
    });

    ui.add_space(6.0);

    // --- load schedules ---
    let r = app.db.list_todo_schedules();
    let rows = app.handle(r, Vec::new());
    let today = Local::now().date_naive();

    ui.label(
        egui::RichText::new(format!("ทั้งหมด {} ตาราง", rows.len()))
            .small()
            .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ยังไม่มีตารางงานประจำ —");
        apply_form(app, submit, cancel_edit);
        return;
    }

    let mut edit_req: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(200.0)) // สิ่งที่ต้องทำ
        .column(Column::auto().at_least(160.0)) // เกี่ยวกับ
        .column(Column::auto().at_least(120.0)) // รอบ
        .column(Column::auto().at_least(110.0)) // รอบถัดไป
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            for h in ["สิ่งที่ต้องทำ", "เกี่ยวกับ", "รอบ", "รอบถัดไป", "จัดการ"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(row.schedule.task.as_str());
                    });
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
                    tr.col(|ui| {
                        ui.label(row.schedule.recurrence.label_th());
                    });
                    tr.col(|ui| {
                        let next = row
                            .schedule
                            .recurrence
                            .next_occurrence_after(row.schedule.start_date, today);
                        ui.label(egui::RichText::new(next.format("%Y-%m-%d").to_string()).small());
                    });
                    tr.col(|ui| {
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_req = Some(row.schedule.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.schedule.id, row.schedule.task.clone()));
                        }
                    });
                });
            }
        });

    // --- apply deferred row actions ---
    if let Some(id) = edit_req {
        if let Some(row) = rows.iter().find(|r| r.schedule.id == id) {
            app.todo_schedule_form = TodoScheduleForm::edit_from(&row.schedule);
        }
    }
    if let Some((id, name)) = delete_req {
        app.pending_delete = Some(PendingDelete::TodoSchedule { id, name });
    }

    apply_form(app, submit, cancel_edit);
}

/// Apply the add/edit form's submit or cancel (factored out so it runs whether or
/// not the table was drawn). Contact is optional (mirrors the Todo page).
fn apply_form(app: &mut AppState, submit: bool, cancel_edit: bool) {
    if cancel_edit {
        app.todo_schedule_form.reset();
    }
    if !submit {
        return;
    }
    let f = &app.todo_schedule_form;
    let editing = f.editing_id;
    let recurrence = f.recurrence();
    let result = match editing {
        Some(id) => {
            let s = TodoSchedule {
                id,
                contact_id: f.contact_id,
                task: f.task.clone(),
                recurrence,
                start_date: f.start_date,
                // update_todo_schedule ignores these two.
                last_generated: None,
                created_at: Local::now(),
            };
            app.db.update_todo_schedule(&s)
        }
        None => app
            .db
            .add_todo_schedule(f.contact_id, &f.task, recurrence, f.start_date)
            .map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.todo_schedule_form.reset();
            app.set_status(if editing.is_some() { "บันทึกตารางงานแล้ว" } else { "เพิ่มตารางงานแล้ว" });
        }
        Err(e) => app.set_error(e),
    }
}
