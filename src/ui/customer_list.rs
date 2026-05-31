//! Customer list (Customer Name List): searchable, score-sorted table.

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::ui::forms::{self, ContactForm};
use crate::ui::{self, ACCENT};

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ลูกค้า VIP / Customers");
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("ค้นหา ชื่อ / เบอร์")
                .desired_width(240.0),
        );
        if ui.button("ล้าง").clicked() {
            app.search.clear();
        }
        ui.separator();
        if ui
            .add(egui::Button::new("➕ เพิ่มลูกค้า").fill(ACCENT))
            .clicked()
        {
            app.form = ContactForm::for_new_with_type(ContactType::Customer);
        }
    });

    ui.add_space(8.0);

    let r = app.db.list_customer_rows(&app.search);
    let mut rows = app.handle(r, Vec::new());
    if rows.is_empty() {
        ui.weak("— ไม่มีข้อมูลลูกค้า —");
        return;
    }

    let mut sort = app.customer_sort;
    match sort.col {
        0 => rows.sort_by(|a, b| {
            a.contact
                .display_name()
                .to_lowercase()
                .cmp(&b.contact.display_name().to_lowercase())
        }),
        1 => rows.sort_by(|a, b| a.contact.phone.cmp(&b.contact.phone)),
        2 => rows.sort_by_key(|a| a.score_total),
        _ => {}
    }
    if !sort.ascending {
        rows.reverse();
    }

    let mut edit_id: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;
    let mut activity_id: Option<i64> = None;
    let mut sort_clicked: Option<usize> = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(160.0)) // ชื่อ
        .column(Column::auto().at_least(120.0)) // เบอร์โทร
        .column(Column::auto()) // คะแนน
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            let cols: [(&str, Option<usize>); 4] = [
                ("ชื่อ", Some(0)),
                ("เบอร์โทร", Some(1)),
                ("คะแนน", Some(2)),
                ("จัดการ", None),
            ];
            for (label, col) in cols {
                header.col(|ui| match col {
                    Some(c) => {
                        let txt = format!("{label}{}", sort.arrow(c));
                        if ui
                            .add(egui::Button::new(egui::RichText::new(txt).strong()).frame(false))
                            .clicked()
                        {
                            sort_clicked = Some(c);
                        }
                    }
                    None => {
                        ui.strong(label);
                    }
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(row.contact.display_name());
                    });
                    tr.col(|ui| {
                        ui.label(row.contact.phone.clone().unwrap_or_default());
                    });
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(row.score_total.to_string())
                                .color(ui::score_color(row.score_total, 10))
                                .strong(),
                        );
                    });
                    tr.col(|ui| {
                        if ui.small_button("📝").on_hover_text("ประวัติการติดต่อ").clicked() {
                            activity_id = Some(row.contact.id);
                        }
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_id = Some(row.contact.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.contact.id, row.contact.display_name()));
                        }
                    });
                });
            }
        });

    if let Some(c) = sort_clicked {
        sort.toggle(c);
        app.customer_sort = sort;
    }
    if let Some(id) = activity_id {
        app.activity_contact = Some(id);
        app.activity_note.clear();
    }
    if let Some(id) = edit_id {
        forms::open_edit(app, id);
    }
    if let Some((id, name)) = delete_req {
        app.pending_delete = Some(crate::ui::confirm::PendingDelete::Contact { id, name });
    }
}
