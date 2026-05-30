//! The add/edit contact modal form.
//!
//! Form state lives in [`ContactForm`] on the `AppState`. The form is rendered
//! as an `egui::Window`; on save it builds a [`Contact`] (plus the matching
//! score) and writes through `AppState.db`.

use crate::app::AppState;
use crate::error::{AppError, Result};
use crate::models::contact::{Contact, CustomerScore, ProspectScore};
use crate::models::enums::{ContactType, Gender, NetworkCategory, Rank};
use crate::ui::{ACCENT, ACCENT_STRONG};
use crate::utils::scoring;

/// Editable, string-friendly mirror of a contact and its score, bound directly
/// to widgets. Numbers that the user types (age) are kept as text and parsed on
/// save so partial input doesn't fight the widget.
pub struct ContactForm {
    pub open: bool,
    /// `Some(id)` when editing an existing contact; `None` when adding.
    pub editing_id: Option<i64>,

    pub name: String,
    pub nickname: String,
    pub phone: String,
    pub line_id: String,
    pub age: String,
    pub gender: Gender,
    pub address: String,
    pub network_category: NetworkCategory,
    pub contact_type: ContactType,

    // ABO-only
    pub rank: Rank,
    pub sponsor_id: Option<i64>,
    pub ppv: i64,

    pub notes: String,

    // Prospect score
    pub p_rel: u8,
    pub p_fin_stab: u8,
    pub p_lead: u8,
    pub p_fin_stat: u8,
    pub p_access: u8,

    // Customer score
    pub c_rel: u8,
    pub c_fin: u8,
    pub c_dec: u8,
    pub c_problems: String,
}

impl Default for ContactForm {
    fn default() -> Self {
        ContactForm {
            open: false,
            editing_id: None,
            name: String::new(),
            nickname: String::new(),
            phone: String::new(),
            line_id: String::new(),
            age: String::new(),
            gender: Gender::Male,
            address: String::new(),
            network_category: NetworkCategory::Friend,
            contact_type: ContactType::Prospect,
            rank: Rank::Koc,
            sponsor_id: None,
            ppv: 0,
            notes: String::new(),
            p_rel: 1,
            p_fin_stab: 1,
            p_lead: 1,
            p_fin_stat: 1,
            p_access: 1,
            c_rel: 1,
            c_fin: 1,
            c_dec: 1,
            c_problems: String::new(),
        }
    }
}

impl ContactForm {
    /// A blank "add" form pre-set to the given contact type.
    pub fn for_new_with_type(ty: ContactType) -> Self {
        ContactForm {
            open: true,
            contact_type: ty,
            ..Default::default()
        }
    }

    /// A blank "add" form (defaults to Prospect).
    pub fn for_new() -> Self {
        Self::for_new_with_type(ContactType::Prospect)
    }

    /// An "edit" form populated from an existing contact and its scores.
    pub fn for_edit(
        c: &Contact,
        prospect: Option<ProspectScore>,
        customer: Option<CustomerScore>,
    ) -> Self {
        let p = prospect.unwrap_or_else(|| ProspectScore::new(c.id));
        let cust = customer.unwrap_or_else(|| CustomerScore::new(c.id));
        ContactForm {
            open: true,
            editing_id: Some(c.id),
            name: c.name.clone(),
            nickname: c.nickname.clone().unwrap_or_default(),
            phone: c.phone.clone().unwrap_or_default(),
            line_id: c.line_id.clone().unwrap_or_default(),
            age: c.age.map(|a| a.to_string()).unwrap_or_default(),
            gender: c.gender,
            address: c.address.clone().unwrap_or_default(),
            network_category: c.network_category,
            contact_type: c.contact_type,
            rank: c.rank.unwrap_or(Rank::Koc),
            sponsor_id: c.sponsor_id,
            ppv: c.ppv,
            notes: c.notes.clone().unwrap_or_default(),
            p_rel: p.relationship_closeness,
            p_fin_stab: p.financial_stability,
            p_lead: p.leadership,
            p_fin_stat: p.financial_status,
            p_access: p.accessibility,
            c_rel: cust.relationship_level,
            c_fin: cust.financial_status,
            c_dec: cust.decision_power,
            c_problems: cust.problems,
        }
    }
}

/// Open the edit form for `id`, loading the contact and any score. Shared by the
/// prospect and customer list views.
pub fn open_edit(app: &mut AppState, id: i64) {
    let contact = match app.db.get_contact(id) {
        Ok(c) => c,
        Err(e) => {
            app.set_error(e);
            return;
        }
    };
    let prospect = app.db.get_prospect_score(id).ok().flatten();
    let customer = app.db.get_customer_score(id).ok().flatten();
    app.form = ContactForm::for_edit(&contact, prospect, customer);
}

/// Render the modal if it is open, and perform save/cancel after the window
/// closure returns (so we can borrow `app` freely).
pub fn render(app: &mut AppState, ctx: &egui::Context) {
    if !app.form.open {
        return;
    }

    let title = if app.form.editing_id.is_some() {
        "แก้ไขรายชื่อ / Edit Contact"
    } else {
        "เพิ่มรายชื่อใหม่ / Add Contact"
    };

    // Pre-fetch ABOs for the sponsor selector so the closure never borrows
    // `app.db` while it is mutating `app.form`.
    let abos = app.db.list_abos().unwrap_or_default();
    let editing_id = app.form.editing_id;

    let mut window_open = true;
    let mut save_clicked = false;
    let mut cancel_clicked = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(540.0)
        .open(&mut window_open)
        .show(ctx, |ui| {
            let f = &mut app.form;

            egui::ScrollArea::vertical()
                .max_height(560.0)
                .show(ui, |ui| {
                    egui::Grid::new("contact_form_grid")
                        .num_columns(2)
                        .spacing([10.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("ชื่อ-นามสกุล *");
                            ui.text_edit_singleline(&mut f.name);
                            ui.end_row();

                            ui.label("ชื่อเล่น");
                            ui.text_edit_singleline(&mut f.nickname);
                            ui.end_row();

                            ui.label("เบอร์โทร");
                            ui.text_edit_singleline(&mut f.phone);
                            ui.end_row();

                            ui.label("LINE ID");
                            ui.text_edit_singleline(&mut f.line_id);
                            ui.end_row();

                            ui.label("อายุ");
                            ui.text_edit_singleline(&mut f.age);
                            ui.end_row();

                            ui.label("เพศ");
                            egui::ComboBox::from_id_source("gender_cb")
                                .selected_text(f.gender.label_th())
                                .show_ui(ui, |ui| {
                                    for g in Gender::ALL {
                                        ui.selectable_value(&mut f.gender, g, g.label_th());
                                    }
                                });
                            ui.end_row();

                            ui.label("ที่อยู่");
                            ui.text_edit_singleline(&mut f.address);
                            ui.end_row();

                            ui.label("กลุ่มเครือข่าย");
                            egui::ComboBox::from_id_source("netcat_cb")
                                .selected_text(f.network_category.label_th())
                                .show_ui(ui, |ui| {
                                    for n in NetworkCategory::ALL {
                                        ui.selectable_value(
                                            &mut f.network_category,
                                            n,
                                            n.label_th(),
                                        );
                                    }
                                });
                            ui.end_row();

                            ui.label("ประเภท");
                            egui::ComboBox::from_id_source("ctype_cb")
                                .selected_text(f.contact_type.label_th())
                                .show_ui(ui, |ui| {
                                    for t in ContactType::ALL {
                                        ui.selectable_value(&mut f.contact_type, t, t.label_th());
                                    }
                                });
                            ui.end_row();
                        });

                    ui.add_space(6.0);
                    ui.separator();

                    match f.contact_type {
                        ContactType::Prospect => prospect_score_section(ui, f),
                        ContactType::Customer => customer_score_section(ui, f),
                        ContactType::Abo => abo_section(ui, f, &abos, editing_id),
                    }

                    ui.add_space(6.0);
                    ui.separator();
                    ui.label("บันทึกเพิ่มเติม / Notes");
                    ui.text_edit_multiline(&mut f.notes);
                });

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("💾 บันทึก").strong()).fill(ACCENT))
                    .clicked()
                {
                    save_clicked = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel_clicked = true;
                }
            });
        });

    // The window's X button or Cancel closes without saving.
    if cancel_clicked || !window_open {
        app.form.open = false;
        return;
    }

    if save_clicked {
        match save_form(app) {
            Ok(name) => {
                app.set_status(format!("บันทึก {name} เรียบร้อย"));
                app.form.open = false;
            }
            Err(e) => app.set_error(e),
        }
    }
}

fn prospect_score_section(ui: &mut egui::Ui, f: &mut ContactForm) {
    ui.label(egui::RichText::new("คะแนนผู้มุ่งหวัง (Sponsor List)").color(ACCENT_STRONG).strong());
    egui::Grid::new("p_score_grid")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("ความสัมพันธ์ (1-10)");
            ui.add(egui::DragValue::new(&mut f.p_rel).range(1..=10));
            ui.end_row();
            ui.label("ความมั่นคง (1-5)");
            ui.add(egui::DragValue::new(&mut f.p_fin_stab).range(1..=5));
            ui.end_row();
            ui.label("ความเป็นผู้นำ (1-5)");
            ui.add(egui::DragValue::new(&mut f.p_lead).range(1..=5));
            ui.end_row();
            ui.label("สถานะการเงิน (1-5)");
            ui.add(egui::DragValue::new(&mut f.p_fin_stat).range(1..=5));
            ui.end_row();
            ui.label("ติดต่อง่าย (1-5)");
            ui.add(egui::DragValue::new(&mut f.p_access).range(1..=5));
            ui.end_row();
        });
    let total = scoring::prospect_total(f.p_rel, f.p_fin_stab, f.p_lead, f.p_fin_stat, f.p_access);
    ui.label(
        egui::RichText::new(format!("คะแนนรวม: {total} / 30"))
            .strong()
            .color(crate::ui::score_color(total, 20)),
    );
}

fn customer_score_section(ui: &mut egui::Ui, f: &mut ContactForm) {
    ui.label(egui::RichText::new("คะแนนลูกค้า (Customer List)").color(ACCENT_STRONG).strong());
    egui::Grid::new("c_score_grid")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("สายสัมพันธ์ (1-10)");
            ui.add(egui::DragValue::new(&mut f.c_rel).range(1..=10));
            ui.end_row();
            ui.label("สถานะการเงิน (1-5)");
            ui.add(egui::DragValue::new(&mut f.c_fin).range(1..=5));
            ui.end_row();
            ui.label("อำนาจการตัดสินใจ (1-5)");
            ui.add(egui::DragValue::new(&mut f.c_dec).range(1..=5));
            ui.end_row();
        });
    ui.label("ปัญหา / ความต้องการ");
    ui.text_edit_multiline(&mut f.c_problems);
    let total = scoring::customer_total(f.c_rel, f.c_fin, f.c_dec);
    ui.label(
        egui::RichText::new(format!("คะแนนรวม: {total} / 20"))
            .strong()
            .color(crate::ui::score_color(total, 10)),
    );
}

fn abo_section(ui: &mut egui::Ui, f: &mut ContactForm, abos: &[Contact], editing_id: Option<i64>) {
    ui.label(egui::RichText::new("ข้อมูลนักธุรกิจ (ABO)").color(ACCENT_STRONG).strong());
    egui::Grid::new("abo_grid")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label("ระดับ (Rank)");
            egui::ComboBox::from_id_source("rank_cb")
                .selected_text(f.rank.as_str())
                .show_ui(ui, |ui| {
                    for r in Rank::ALL {
                        ui.selectable_value(&mut f.rank, r, r.label_th());
                    }
                });
            ui.end_row();

            ui.label("ยอดส่วนตัว (PPV)");
            ui.add(egui::DragValue::new(&mut f.ppv).range(0..=10_000_000).speed(100.0));
            ui.end_row();

            ui.label("อัพไลน์ (Sponsor)");
            let current = f
                .sponsor_id
                .and_then(|sid| abos.iter().find(|a| a.id == sid))
                .map(|a| a.display_name())
                .unwrap_or_else(|| "— ไม่มี —".to_string());
            egui::ComboBox::from_id_source("sponsor_cb")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.sponsor_id, None, "— ไม่มี —");
                    for a in abos {
                        // An ABO cannot sponsor itself.
                        if Some(a.id) == editing_id {
                            continue;
                        }
                        ui.selectable_value(&mut f.sponsor_id, Some(a.id), a.display_name());
                    }
                });
            ui.end_row();
        });
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Build a contact (+ score) from the form and persist it. Returns the display
/// name on success.
fn save_form(app: &mut AppState) -> Result<String> {
    let f = &app.form;
    if f.name.trim().is_empty() {
        return Err(AppError::validation("กรุณากรอกชื่อ (name is required)"));
    }

    let age = if f.age.trim().is_empty() {
        None
    } else {
        Some(
            f.age
                .trim()
                .parse::<u8>()
                .map_err(|_| AppError::validation("อายุไม่ถูกต้อง (age must be 0-255)"))?,
        )
    };

    let mut c = Contact::new_blank();
    c.id = f.editing_id.unwrap_or(0);
    c.name = f.name.trim().to_string();
    c.nickname = opt(&f.nickname);
    c.phone = opt(&f.phone);
    c.line_id = opt(&f.line_id);
    c.age = age;
    c.gender = f.gender;
    c.address = opt(&f.address);
    c.network_category = f.network_category;
    c.contact_type = f.contact_type;
    c.rank = if f.contact_type == ContactType::Abo {
        Some(f.rank)
    } else {
        None
    };
    c.sponsor_id = if f.contact_type == ContactType::Abo {
        f.sponsor_id
    } else {
        None
    };
    c.ppv = f.ppv;
    c.notes = opt(&f.notes);

    let display = c.display_name();
    let contact_type = f.contact_type;

    // Capture score inputs before we stop borrowing the form.
    let prospect_inputs = (f.p_rel, f.p_fin_stab, f.p_lead, f.p_fin_stat, f.p_access);
    let customer_inputs = (f.c_rel, f.c_fin, f.c_dec, f.c_problems.clone());

    let id = match f.editing_id {
        Some(id) => {
            app.db.update_contact(&c)?;
            id
        }
        None => app.db.insert_contact(&c)?,
    };

    match contact_type {
        ContactType::Prospect => {
            let mut s = ProspectScore::new(id);
            s.relationship_closeness = prospect_inputs.0;
            s.financial_stability = prospect_inputs.1;
            s.leadership = prospect_inputs.2;
            s.financial_status = prospect_inputs.3;
            s.accessibility = prospect_inputs.4;
            s.recompute();
            app.db.upsert_prospect_score(&s)?;
        }
        ContactType::Customer => {
            let mut s = CustomerScore::new(id);
            s.relationship_level = customer_inputs.0;
            s.financial_status = customer_inputs.1;
            s.decision_power = customer_inputs.2;
            s.problems = customer_inputs.3;
            s.recompute();
            app.db.upsert_customer_score(&s)?;
        }
        ContactType::Abo => {}
    }

    Ok(display)
}
