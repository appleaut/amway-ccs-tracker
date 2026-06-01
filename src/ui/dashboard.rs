//! Dashboard: summary cards and at-a-glance progress.

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::ui::{self, ACCENT_STRONG};

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("แดชบอร์ด / Dashboard");
    ui.label(egui::RichText::new("ภาพรวมธุรกิจตามแนวทาง CCS Guide").weak());
    ui.add_space(12.0);

    let r = app.db.count_by_type(ContactType::Prospect);
    let prospects = app.handle(r, 0);
    let r = app.db.count_by_type(ContactType::Customer);
    let customers = app.handle(r, 0);
    let r = app.db.count_by_type(ContactType::Abo);
    let abos = app.handle(r, 0);
    let r = app.db.count_conversions_this_month();
    let conversions = app.handle(r, 0);
    let r = app.db.count_overdue_todos();
    let overdue = app.handle(r, 0);

    let mut go_overdue = false;
    ui.horizontal_wrapped(|ui| {
        ui::metric_card(ui, "ผู้มุ่งหวัง (Prospects)", &prospects.to_string(), ACCENT_STRONG);
        ui::metric_card(
            ui,
            "ลูกค้า VIP (Customers)",
            &customers.to_string(),
            egui::Color32::from_rgb(0x2E, 0x7D, 0x32), // green 800
        );
        ui::metric_card(
            ui,
            "นักธุรกิจ (ABO)",
            &abos.to_string(),
            egui::Color32::from_rgb(0xE6, 0x51, 0x00), // orange 900
        );
        ui::metric_card(
            ui,
            "เปลี่ยนสถานะเดือนนี้",
            &conversions.to_string(),
            egui::Color32::from_rgb(0xAD, 0x14, 0x57), // pink 800
        );
        if ui::metric_card_clickable(
            ui,
            "งานเลยกำหนด (Overdue)",
            &overdue.to_string(),
            egui::Color32::from_rgb(0xD3, 0x2F, 0x2F), // red 700
        )
        .clicked()
        {
            go_overdue = true;
        }
    });
    if go_overdue {
        app.view = ui::View::Todos;
        app.todo_status_filter = ui::todo::TodoStatusFilter::Overdue;
        // Show every overdue task, regardless of any prior contact-type filter.
        app.todo_who_filter = ui::todo::TodoWhoFilter::All;
    }

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(10.0);

    ui.label(egui::RichText::new("เป้าหมายลูกค้า VIP 20 คน").strong());
    let frac = (customers as f32 / 20.0).clamp(0.0, 1.0);
    ui.add(egui::ProgressBar::new(frac).text(format!("{customers} / 20")));

    ui.add_space(16.0);
    ui.label(egui::RichText::new("ขั้นตอน Sponsor Flow (8 ขั้น)").strong());
    ui.add_space(4.0);
    ui.label(
        "1 จดรายชื่อ → 2 สร้างนัด → 3 เช็คฟอร์ม → 4 เปิดใจ → \
         5 เปิดภาพ → 6 ปิดสมัคร → 7 ติดตาม BK → 8 วางแผน",
    );
}
