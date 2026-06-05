//! งานประชุม (Meetings): a contacts × meetings attendance matrix. Rows are
//! contacts (filterable by name and type); each column is a meeting (by default
//! only those not yet finished). Each cell shows the contact's RSVP status, an
//! entry-fee paid marker, and — once recorded — actual attendance; clicking a
//! cell opens a popup to set them. Clicking a column header edits that meeting.

use std::collections::HashMap;

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::db::queries::group_thousands;
use crate::models::contact::Contact;
use crate::models::enums::{AttendeeStatus, ContactType};
use crate::models::meeting::{Meeting, MeetingAttendee};
use crate::ui::meeting_form::MeetingForm;
use crate::ui::{ACCENT, ACCENT_STRONG};

/// Contact-type filter on the Meetings page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MeetingWhoFilter {
    All,
    Type(ContactType),
}

impl MeetingWhoFilter {
    fn label(self) -> &'static str {
        match self {
            MeetingWhoFilter::All => "ทั้งหมด",
            MeetingWhoFilter::Type(t) => t.label_th(),
        }
    }
}

/// A deferred cell edit, applied after the table closure (so it does not borrow
/// `app.db` while the table borrows `app`).
enum CellAction {
    Upsert {
        meeting_id: i64,
        contact_id: i64,
        status: AttendeeStatus,
        paid: bool,
        attended: Option<bool>,
    },
    Remove {
        meeting_id: i64,
        contact_id: i64,
    },
}

fn status_color(s: AttendeeStatus) -> egui::Color32 {
    match s {
        AttendeeStatus::Attending => egui::Color32::from_rgb(0x2E, 0x7D, 0x32), // green
        AttendeeStatus::Undecided => egui::Color32::from_rgb(0x9E, 0x9E, 0x9E), // grey
        AttendeeStatus::NotAttending => egui::Color32::from_rgb(0xD3, 0x2F, 0x2F), // red
    }
}

fn type_color(t: ContactType) -> egui::Color32 {
    match t {
        ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
        ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
        ContactType::Abo => ACCENT_STRONG,
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("งานประชุม / Meetings");
    ui.label(
        egui::RichText::new("ตามรายชื่อเข้าร่วมงาน — คลิกช่องเพื่อตั้งสถานะเข้าร่วม / จ่ายเงิน / ผลหลังงาน")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Toolbar.
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new("➕ เพิ่มงานประชุม").fill(ACCENT)).clicked() {
            app.meeting_form = MeetingForm::for_new();
        }
        ui.separator();
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("ค้นหาชื่อ")
                .desired_width(160.0),
        );
        if ui.button("ล้าง").clicked() {
            app.search.clear();
        }
        ui.separator();
        ui.label("ประเภท:");
        egui::ComboBox::from_id_source("meeting_who_cb")
            .selected_text(app.meeting_who_filter.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.meeting_who_filter,
                    MeetingWhoFilter::All,
                    MeetingWhoFilter::All.label(),
                );
                for t in ContactType::ALL {
                    ui.selectable_value(
                        &mut app.meeting_who_filter,
                        MeetingWhoFilter::Type(t),
                        t.label_th(),
                    );
                }
            });
        ui.separator();
        ui.checkbox(&mut app.meeting_show_past, "แสดงงานที่ผ่านมาแล้ว");
    });
    ui.add_space(8.0);

    // Load data (pre-fetched so the table closure does not borrow app.db).
    let meetings = app.handle(app.db.list_meetings(app.meeting_show_past), Vec::new());
    let contacts = app.handle(app.db.list_contacts(), Vec::new());
    let attendees = app.handle(app.db.attendee_map(), HashMap::new());

    // Filter rows by the shared search box and the contact-type filter.
    let needle = app.search.trim().to_lowercase();
    let who = app.meeting_who_filter;
    let rows: Vec<&Contact> = contacts
        .iter()
        .filter(|c| {
            let name_ok = needle.is_empty()
                || c.name.to_lowercase().contains(&needle)
                || c.nickname.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle));
            let who_ok = match who {
                MeetingWhoFilter::All => true,
                MeetingWhoFilter::Type(t) => c.contact_type == t,
            };
            name_ok && who_ok
        })
        .collect();

    if meetings.is_empty() {
        ui.weak("— ยังไม่มีงานประชุม กดปุ่ม ➕ เพิ่มงานประชุม เพื่อเริ่ม —");
        return;
    }
    if rows.is_empty() {
        ui.weak("— ไม่มีรายชื่อในตัวกรองนี้ —");
        return;
    }

    let mut action: Option<CellAction> = None;
    let mut edit_meeting: Option<i64> = None;

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(170.0)); // contact name
    for _ in &meetings {
        table = table.column(Column::auto().at_least(96.0)); // one per meeting
    }
    table
        .header(40.0, |mut header| {
            header.col(|ui| {
                ui.strong("รายชื่อ \\ งาน");
            });
            for m in &meetings {
                header.col(|ui| {
                    ui.vertical(|ui| {
                        let title = ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(&m.name).strong().color(ACCENT_STRONG),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("คลิกเพื่อแก้ไข / ลบงาน")
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if title.clicked() {
                            edit_meeting = Some(m.id);
                        }
                        let date = if m.start_date == m.end_date {
                            m.start_date.format("%d/%m/%y").to_string()
                        } else {
                            format!("{}–{}", m.start_date.format("%d/%m"), m.end_date.format("%d/%m/%y"))
                        };
                        ui.label(egui::RichText::new(date).small().weak());
                        let fee = if m.fee > 0 {
                            format!("{} บาท", group_thousands(m.fee))
                        } else {
                            "ฟรี".to_string()
                        };
                        ui.label(egui::RichText::new(fee).small().weak());
                    });
                });
            }
        })
        .body(|mut body| {
            for c in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(c.display_name()).color(type_color(c.contact_type)),
                        );
                    });
                    for m in &meetings {
                        tr.col(|ui| {
                            let cell = attendees.get(&(m.id, c.id));
                            cell_widget(ui, m, c.id, cell, &mut action);
                        });
                    }
                });
            }
        });

    // Apply deferred edits.
    if let Some(act) = action {
        let result = match act {
            CellAction::Upsert { meeting_id, contact_id, status, paid, attended } => {
                app.db.upsert_attendee(meeting_id, contact_id, status, paid, attended)
            }
            CellAction::Remove { meeting_id, contact_id } => {
                app.db.remove_attendee(meeting_id, contact_id)
            }
        };
        if let Err(e) = result {
            app.set_error(e);
        }
    }
    if let Some(id) = edit_meeting {
        if let Some(m) = meetings.iter().find(|m| m.id == id) {
            app.meeting_form = MeetingForm::for_edit(m);
        }
    }
}

/// Render one matrix cell: the status/paid/attended markers as a clickable label,
/// plus the edit popup. Records the chosen change into `action`.
fn cell_widget(
    ui: &mut egui::Ui,
    m: &Meeting,
    contact_id: i64,
    cell: Option<&MeetingAttendee>,
    action: &mut Option<CellAction>,
) {
    let popup_id = ui.make_persistent_id(("meeting_cell", m.id, contact_id));
    let cur_status = cell.map(|a| a.status);
    let cur_paid = cell.is_some_and(|a| a.paid);
    let cur_attended = cell.and_then(|a| a.attended);

    let resp = match cell {
        Some(a) => {
            let mut text = String::from("●");
            if m.fee > 0 && a.paid {
                text.push_str(" 💵");
            }
            match a.attended {
                Some(true) => text.push_str(" ✓"),
                Some(false) => text.push_str(" ✗"),
                None => {}
            }
            ui.add(
                egui::Label::new(egui::RichText::new(text).color(status_color(a.status)))
                    .sense(egui::Sense::click()),
            )
        }
        None => ui.add(
            egui::Label::new(egui::RichText::new("·").weak()).sense(egui::Sense::click()),
        ),
    }
    .on_hover_cursor(egui::CursorIcon::PointingHand);

    if resp.clicked() {
        ui.memory_mut(|mm| mm.toggle_popup(popup_id));
    }

    let base_status = cur_status.unwrap_or(AttendeeStatus::Undecided);
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(170.0);
            ui.label(egui::RichText::new("สถานะตอบรับ").small().weak());
            for s in AttendeeStatus::ALL {
                if ui.selectable_label(cur_status == Some(s), s.label_th()).clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: s,
                        paid: cur_paid,
                        attended: cur_attended,
                    });
                }
            }
            if cell.is_some() && ui.selectable_label(false, "เอาออกจากงาน").clicked() {
                *action = Some(CellAction::Remove { meeting_id: m.id, contact_id });
            }

            if m.fee > 0 {
                ui.separator();
                let mut paid = cur_paid;
                if ui.checkbox(&mut paid, "จ่ายค่าเข้างานแล้ว").changed() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid,
                        attended: cur_attended,
                    });
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("ผลหลังงาน").small().weak());
            ui.horizontal(|ui| {
                if ui.selectable_label(cur_attended == Some(true), "มาจริง").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: Some(true),
                    });
                }
                if ui.selectable_label(cur_attended == Some(false), "ไม่มา").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: Some(false),
                    });
                }
                if ui.selectable_label(cur_attended.is_none(), "ล้าง").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: None,
                    });
                }
            });
        },
    );
}
