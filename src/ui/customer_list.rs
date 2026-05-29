//! Customer list (Customer Name List): searchable, score-sorted table.

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
    let rows = app.handle(r, Vec::new());
    if rows.is_empty() {
        ui.weak("— ไม่มีข้อมูลลูกค้า —");
        return;
    }

    let mut edit_id: Option<i64> = None;
    let mut delete_id: Option<i64> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("customer_table")
            .striped(true)
            .num_columns(4)
            .spacing([14.0, 8.0])
            .show(ui, |ui| {
                for h in ["ชื่อ", "เบอร์โทร", "คะแนน", "จัดการ"] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();

                for row in &rows {
                    ui.label(row.contact.display_name());
                    ui.label(row.contact.phone.clone().unwrap_or_default());
                    ui.label(
                        egui::RichText::new(row.score_total.to_string())
                            .color(ui::score_color(row.score_total, 10))
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_id = Some(row.contact.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_id = Some(row.contact.id);
                        }
                    });
                    ui.end_row();
                }
            });
    });

    if let Some(id) = edit_id {
        forms::open_edit(app, id);
    }
    if let Some(id) = delete_id {
        match app.db.delete_contact(id) {
            Ok(()) => app.set_status("ลบรายชื่อเรียบร้อย"),
            Err(e) => app.set_error(e),
        }
    }
}
