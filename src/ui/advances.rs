//! Advance Payments (สำรองจ่าย): money fronted to buy products for a contact, to
//! be collected later. Add/edit via a form group on the left, search/filter on
//! the right; the table shows outstanding/collected status, a per-row "เก็บเงิน"
//! action (which logs to the contact's activity history), and edit/delete.
//! Outstanding rows are listed first, oldest advance date first.

use chrono::{Local, NaiveDate};
use egui_extras::{Column, DatePickerButton, TableBuilder};

use crate::app::AppState;
use crate::db::queries::group_thousands;
use crate::models::advance::Advance;
use crate::models::enums::ContactType;
use crate::ui::advance_collect::PendingAdvanceCollect;
use crate::ui::confirm::PendingDelete;
use crate::ui::{filter_combo, ACCENT, ACCENT_STRONG};

/// Width of the fixed label column in each form/filter row.
const LABEL_W: f32 = 110.0;

/// One labelled form row: a fixed-width label cell, then the field widget.
/// (Mirrors the helper of the same name in `ui/todo.rs`.)
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

/// Add/edit form state for the Advance Payments page.
pub struct AdvanceForm {
    /// `Some(id)` when editing an existing advance; `None` when adding.
    pub editing_id: Option<i64>,
    pub contact_id: Option<i64>,
    pub contact_filter: String,
    pub item: String,
    pub amount: i64,
    pub advance_date: NaiveDate,
    pub note: String,
}

impl Default for AdvanceForm {
    fn default() -> Self {
        AdvanceForm {
            editing_id: None,
            contact_id: None,
            contact_filter: String::new(),
            item: String::new(),
            amount: 0,
            advance_date: Local::now().date_naive(),
            note: String::new(),
        }
    }
}

impl AdvanceForm {
    fn reset(&mut self) {
        *self = AdvanceForm::default();
    }
}

/// Status filter on the Advance Payments page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AdvanceStatusFilter {
    Outstanding,
    Collected,
    All,
}

impl AdvanceStatusFilter {
    const ALL: [AdvanceStatusFilter; 3] = [
        AdvanceStatusFilter::Outstanding,
        AdvanceStatusFilter::Collected,
        AdvanceStatusFilter::All,
    ];
    fn label(self) -> &'static str {
        match self {
            AdvanceStatusFilter::Outstanding => "รอเก็บเงิน",
            AdvanceStatusFilter::Collected => "เก็บเงินแล้ว",
            AdvanceStatusFilter::All => "ทั้งหมด",
        }
    }
    /// The `collected_filter` argument for `list_advances`.
    fn as_filter(self) -> Option<bool> {
        match self {
            AdvanceStatusFilter::Outstanding => Some(false),
            AdvanceStatusFilter::Collected => Some(true),
            AdvanceStatusFilter::All => None,
        }
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("สำรองจ่าย / Advance Payments");
    ui.label(
        egui::RichText::new("เงินที่จ่ายล่วงหน้าซื้อสินค้าให้รายชื่อ แล้วรอเก็บคืนภายหลัง")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Contacts for the picker (all types), pre-fetched so the combo closure does
    // not borrow app.db while mutating app.advance_form.
    let contacts = app.db.list_contacts().unwrap_or_default();
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();

    let mut submit = false;
    let mut cancel_edit = false;
    let editing = app.advance_form.editing_id.is_some();

    ui.columns(2, |cols| {
        let field_w = (cols[0].available_width() - LABEL_W - 40.0).max(60.0);
        let search_field_w = (field_w - 60.0).max(60.0);

        // Left card: add / edit form.
        let c0 = &mut cols[0];
        egui::Frame::group(c0.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c0, |ui| {
                let f = &mut app.advance_form;
                ui.label(
                    egui::RichText::new(if editing {
                        "✏ แก้ไขรายการ"
                    } else {
                        "➕ เพิ่มรายการสำรองจ่าย"
                    })
                    .color(ACCENT_STRONG)
                    .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "รายชื่อ", |ui| {
                    filter_combo(
                        ui,
                        "advance_contact_cb",
                        &mut f.contact_id,
                        &mut f.contact_filter,
                        Some("— เลือกรายชื่อ —"),
                        &contact_options,
                        field_w,
                    );
                });
                field_row(ui, "รายการสินค้า", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.item)
                            .hint_text("เช่น Nutrilite โปรตีน 2 กระปุก")
                            .desired_width(field_w),
                    );
                });
                field_row(ui, "จำนวนเงิน (บาท)", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut f.amount)
                            .range(0..=99_999_999)
                            .suffix(" บาท"),
                    );
                });
                field_row(ui, "วันที่จ่าย", |ui| {
                    ui.add(
                        DatePickerButton::new(&mut f.advance_date)
                            .id_source("advance_date_picker"),
                    );
                });
                field_row(ui, "หมายเหตุ", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.note)
                            .hint_text("ไม่บังคับ เช่น รับของที่ร้านแล้ว")
                            .desired_width(field_w),
                    );
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

        // Right card: search + status filter.
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
                    ui.add(
                        egui::TextEdit::singleline(&mut app.search)
                            .hint_text("สินค้า / ชื่อ")
                            .desired_width(search_field_w),
                    );
                    if ui.button("ล้าง").clicked() {
                        app.search.clear();
                    }
                });
                field_row(ui, "สถานะ", |ui| {
                    egui::ComboBox::from_id_source("advance_status_cb")
                        .width(field_w)
                        .selected_text(app.advance_status_filter.label())
                        .show_ui(ui, |ui| {
                            for s in AdvanceStatusFilter::ALL {
                                ui.selectable_value(&mut app.advance_status_filter, s, s.label());
                            }
                        });
                });
            });
    });

    ui.add_space(6.0);

    // --- load rows (status filter applied in SQL) ---
    let filter = app.advance_status_filter.as_filter();
    let r = app.db.list_advances(&app.search, filter);
    let rows = app.handle(r, Vec::new());

    // Outstanding total comes from the DB (ALL outstanding rows, regardless of
    // the current status filter); rows.len() is what the filter currently shows.
    let rt = app.db.outstanding_total();
    let out_total = app.handle(rt, 0);
    ui.label(
        egui::RichText::new(format!(
            "ยอดรอเก็บรวมทั้งหมด: {} บาท • แสดง {} รายการ",
            group_thousands(out_total),
            rows.len()
        ))
        .small()
        .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ไม่มีรายการในตัวกรองนี้ —");
        apply_form(app, submit, cancel_edit);
        return;
    }

    let mut collect_req: Option<i64> = None;
    let mut edit_req: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;
    let collected_color = egui::Color32::from_rgb(0x2E, 0x7D, 0x32);
    let outstanding_color = egui::Color32::from_rgb(0xB2, 0x6A, 0x00);

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(100.0)) // วันที่จ่าย
        .column(Column::auto().at_least(140.0)) // ชื่อ
        .column(Column::remainder().at_least(160.0)) // รายการสินค้า
        .column(Column::auto().at_least(90.0)) // จำนวนเงิน
        .column(Column::auto().at_least(120.0)) // สถานะ
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            for h in ["วันที่จ่าย", "ชื่อ", "รายการสินค้า", "จำนวนเงิน", "สถานะ", "จัดการ"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(
                                row.advance.advance_date.format("%Y-%m-%d").to_string(),
                            )
                            .small()
                            .weak(),
                        );
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
                        let resp = ui.label(&row.advance.item);
                        if !row.advance.note.is_empty() {
                            resp.on_hover_text(&row.advance.note);
                        }
                    });
                    tr.col(|ui| {
                        ui.label(format!("{} บาท", group_thousands(row.advance.amount)));
                    });
                    tr.col(|ui| {
                        if row.advance.collected {
                            let when = row
                                .advance
                                .collected_at
                                .map(|d| d.format("%Y-%m-%d").to_string())
                                .unwrap_or_default();
                            ui.label(
                                egui::RichText::new(format!("✅ เก็บแล้ว {when}"))
                                    .small()
                                    .color(collected_color),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("รอเก็บเงิน").small().color(outstanding_color),
                            );
                        }
                    });
                    tr.col(|ui| {
                        if !row.advance.collected
                            && ui.small_button("เก็บเงิน").on_hover_text("บันทึกการเก็บเงิน").clicked()
                        {
                            collect_req = Some(row.advance.id);
                        }
                        if !row.advance.collected
                            && ui.small_button("✏").on_hover_text("แก้ไข").clicked()
                        {
                            edit_req = Some(row.advance.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.advance.id, row.advance.item.clone()));
                        }
                    });
                });
            }
        });

    // --- apply deferred row actions ---
    if let Some(id) = collect_req {
        if let Some(row) = rows.iter().find(|r| r.advance.id == id) {
            match (row.advance.contact_id, &row.contact_name) {
                (Some(_), Some(name)) => {
                    app.pending_advance_collect = Some(PendingAdvanceCollect {
                        id,
                        item: row.advance.item.clone(),
                        amount: row.advance.amount,
                        contact_name: name.clone(),
                    });
                    app.advance_collect_date = Local::now().date_naive();
                    app.advance_collect_note.clear();
                }
                _ => {
                    // Orphaned (contact deleted): collect now, nothing to log.
                    if let Err(e) = app.db.collect_advance(id, Local::now().date_naive(), "") {
                        app.set_error(e);
                    } else {
                        app.set_status(
                            "ทำเครื่องหมายเก็บเงินแล้ว — รายการนี้ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ",
                        );
                    }
                }
            }
        }
    }
    if let Some(id) = edit_req {
        if let Some(row) = rows.iter().find(|r| r.advance.id == id) {
            app.advance_form = AdvanceForm {
                editing_id: Some(row.advance.id),
                contact_id: row.advance.contact_id,
                contact_filter: String::new(),
                item: row.advance.item.clone(),
                amount: row.advance.amount,
                advance_date: row.advance.advance_date,
                note: row.advance.note.clone(),
            };
        }
    }
    if let Some((id, item)) = delete_req {
        app.pending_delete = Some(PendingDelete::Advance { id, item });
    }

    apply_form(app, submit, cancel_edit);
}

/// Apply the add/edit form's submit or cancel (factored out so it runs whether or
/// not the table was drawn). Contact is required.
fn apply_form(app: &mut AppState, submit: bool, cancel_edit: bool) {
    if cancel_edit {
        app.advance_form.reset();
    }
    if !submit {
        return;
    }
    if app.advance_form.contact_id.is_none() {
        app.set_error("กรุณาเลือกรายชื่อ");
        return;
    }
    let editing = app.advance_form.editing_id;
    let result = match editing {
        Some(id) => {
            let a = Advance {
                id,
                contact_id: app.advance_form.contact_id,
                item: app.advance_form.item.clone(),
                amount: app.advance_form.amount,
                advance_date: app.advance_form.advance_date,
                note: app.advance_form.note.clone(),
                // update_advance writes only contact/item/amount/date/note;
                // these are placeholders it ignores.
                collected: false,
                collected_at: None,
                collected_note: None,
                created_at: Local::now(),
            };
            app.db.update_advance(&a)
        }
        None => app
            .db
            .add_advance(
                app.advance_form.contact_id,
                &app.advance_form.item,
                app.advance_form.amount,
                app.advance_form.advance_date,
                &app.advance_form.note,
            )
            .map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.advance_form.reset();
            app.set_status(if editing.is_some() { "บันทึกรายการแล้ว" } else { "เพิ่มรายการแล้ว" });
        }
        Err(e) => app.set_error(e),
    }
}
