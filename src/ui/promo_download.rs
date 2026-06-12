//! Promotion downloader page: one button that runs the embedded Python+Playwright
//! downloader in the background and streams its progress here. Images save to
//! `Downloads\amway-promotion-<current month>\`.

use crate::app::AppState;
use crate::promo;
use crate::ui::{ACCENT, ACCENT_STRONG};

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ดาวน์โหลดโปรโมชัน / Promotions");
    ui.label(
        egui::RichText::new("ดาวน์โหลดรูปโปรโมชันประจำเดือนจาก amway.co.th")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // How-it-works card.
    egui::Frame::group(ui.style())
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("ℹ วิธีทำงาน").strong().color(ACCENT_STRONG));
            ui.add_space(4.0);
            ui.label("• เปิดหน้าต่าง Chrome จริง — ถ้ามีหน้า CAPTCHA ให้ยืนยันในหน้าต่างนั้น");
            ui.label("• บันทึกไปที่ Downloads\\amway-promotion-<เดือนปัจจุบัน>");
            ui.label("• ตั้งชื่อรูปเรียง 0001, 0002, … และแปลงไฟล์ .avif เป็น .jpg ให้");
            ui.label("• ต้องมี Python (+Playwright) และ Google Chrome ติดตั้งบนเครื่อง");
        });
    ui.add_space(10.0);

    let today = chrono::Local::now().date_naive();
    let folder = promo::month_folder_name(today);

    ui.horizontal(|ui| {
        let btn = egui::Button::new(
            egui::RichText::new("⬇  ดาวน์โหลดโปรโมชันเดือนนี้").size(16.0),
        )
        .fill(ACCENT);
        if ui.add_enabled(!app.promo_running, btn).clicked() {
            start(app);
        }
        if app.promo_running {
            ui.add(egui::Spinner::new());
            ui.label("กำลังดาวน์โหลด…");
        }
    });
    ui.label(
        egui::RichText::new(format!("โฟลเดอร์ปลายทาง: Downloads\\{folder}"))
            .small()
            .weak(),
    );

    if let Some(result) = &app.promo_last_result {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(result).strong().color(ACCENT_STRONG));
    }

    // Streamed progress log.
    if !app.promo_log.is_empty() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("ความคืบหน้า").small().weak());
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &app.promo_log {
                    ui.label(egui::RichText::new(line).monospace().size(12.5));
                }
            });
    }
}

/// Kick off a background download for the current month.
fn start(app: &mut AppState) {
    let today = chrono::Local::now().date_naive();
    let folder = promo::month_folder_name(today);
    match promo::downloads_dir() {
        Ok(base) => {
            let out_dir = base.join(folder);
            app.promo_log.clear();
            app.promo_last_result = None;
            app.promo_running = true;
            app.promo_rx = Some(promo::start_download(out_dir));
            app.set_status("เริ่มดาวน์โหลดโปรโมชัน…");
        }
        Err(e) => app.set_error(e),
    }
}
