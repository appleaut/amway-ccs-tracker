//! Add/edit meeting modal.
//!
//! Form state lives in [`MeetingForm`] on the `AppState`; the page opens it via
//! `MeetingForm::for_new()` / `for_edit(&Meeting)`. Rendered as an `egui::Window`;
//! on save it builds a [`Meeting`] and writes through `AppState.db`. The "ลบงาน"
//! button routes through the shared confirm dialog.

use chrono::{Local, NaiveDate};
use egui_extras::DatePickerButton;

use crate::app::AppState;
use crate::models::meeting::Meeting;
use crate::ui::confirm::PendingDelete;
use crate::ui::ACCENT;

const LABEL_W: f32 = 120.0;
const FIELD_W: f32 = 300.0;

/// One labelled form row (mirrors the helper in `ui/advances.rs`).
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

/// Add/edit form state for a meeting.
pub struct MeetingForm {
    pub open: bool,
    /// `Some(id)` when editing; `None` when adding.
    pub editing_id: Option<i64>,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub description: String,
    pub fee: i64,
}

impl Default for MeetingForm {
    fn default() -> Self {
        let today = Local::now().date_naive();
        MeetingForm {
            open: false,
            editing_id: None,
            name: String::new(),
            start_date: today,
            end_date: today,
            description: String::new(),
            fee: 0,
        }
    }
}

impl MeetingForm {
    pub fn for_new() -> Self {
        MeetingForm {
            open: true,
            ..Default::default()
        }
    }

    pub fn for_edit(m: &Meeting) -> Self {
        MeetingForm {
            open: true,
            editing_id: Some(m.id),
            name: m.name.clone(),
            start_date: m.start_date,
            end_date: m.end_date,
            description: m.description.clone(),
            fee: m.fee,
        }
    }
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    if !app.meeting_form.open {
        return;
    }
    let editing = app.meeting_form.editing_id;
    let title = if editing.is_some() {
        "แก้ไขงานประชุม / Edit Meeting"
    } else {
        "เพิ่มงานประชุม / Add Meeting"
    };

    let mut window_open = true;
    let mut save = false;
    let mut cancel = false;
    let mut delete = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut window_open)
        .show(ctx, |ui| {
            let f = &mut app.meeting_form;
            ui.add_space(4.0);
            field_row(ui, "ชื่องาน *", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut f.name)
                        .hint_text("เช่น สัมมนา CCS ประจำเดือน")
                        .desired_width(FIELD_W),
                );
            });
            field_row(ui, "วันที่เริ่ม", |ui| {
                ui.add(DatePickerButton::new(&mut f.start_date).id_source("meeting_start_picker"));
            });
            field_row(ui, "วันที่สิ้นสุด", |ui| {
                ui.add(DatePickerButton::new(&mut f.end_date).id_source("meeting_end_picker"));
            });
            field_row(ui, "ค่าเข้างาน (บาท)", |ui| {
                ui.add(egui::DragValue::new(&mut f.fee).range(0..=99_999_999).suffix(" บาท"));
            });
            field_row(ui, "รายละเอียด", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut f.description)
                        .hint_text("ไม่บังคับ เช่น สถานที่ / วิทยากร")
                        .desired_rows(3)
                        .desired_width(FIELD_W),
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("💾 บันทึก").strong()).fill(ACCENT))
                    .clicked()
                {
                    save = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
                if editing.is_some() {
                    ui.add_space(20.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("🗑 ลบงาน").color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(0xD3, 0x2F, 0x2F)),
                        )
                        .clicked()
                    {
                        delete = true;
                    }
                }
            });
        });

    if delete {
        if let Some(id) = editing {
            app.pending_delete = Some(PendingDelete::Meeting {
                id,
                name: app.meeting_form.name.clone(),
            });
        }
        app.meeting_form.open = false;
        return;
    }
    if cancel || !window_open {
        app.meeting_form.open = false;
        return;
    }
    if save {
        let f = &app.meeting_form;
        let result = match editing {
            Some(id) => app.db.update_meeting(&Meeting {
                id,
                name: f.name.clone(),
                start_date: f.start_date,
                end_date: f.end_date,
                description: f.description.clone(),
                fee: f.fee,
                created_at: Local::now(), // ignored by update_meeting
            }),
            None => app
                .db
                .add_meeting(&f.name, f.start_date, f.end_date, &f.description, f.fee)
                .map(|_| ()),
        };
        match result {
            Ok(()) => {
                app.set_status(if editing.is_some() { "บันทึกงานแล้ว" } else { "เพิ่มงานแล้ว" });
                app.meeting_form.open = false;
            }
            Err(e) => app.set_error(e),
        }
    }
}
