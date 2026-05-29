//! Follow-Up Sheet: per-ABO BK1 / BK2 / C1 / Conference checklist with a
//! completion progress bar. Toggling a checkbox persists immediately.

use chrono::Local;

use crate::app::AppState;
use crate::models::followup::FollowUpSheet;
use crate::ui::ACCENT_STRONG;

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ติดตามผล / Follow Up Sheet");
    ui.add_space(6.0);

    let r = app.db.list_abos();
    let abos = app.handle(r, Vec::new());
    if abos.is_empty() {
        ui.weak("ยังไม่มีนักธุรกิจ (ABO) — เพิ่ม ABO ในหน้าเครือข่ายก่อน");
        return;
    }

    // Default selection to the first ABO; keep within the available set.
    let mut selected = app
        .selected_abo
        .filter(|id| abos.iter().any(|a| a.id == *id))
        .unwrap_or(abos[0].id);

    ui.horizontal(|ui| {
        ui.label("เลือก ABO:");
        let current = abos
            .iter()
            .find(|a| a.id == selected)
            .map(|a| a.display_name())
            .unwrap_or_default();
        egui::ComboBox::from_id_source("abo_select")
            .selected_text(current)
            .width(260.0)
            .show_ui(ui, |ui| {
                for a in &abos {
                    ui.selectable_value(&mut selected, a.id, a.display_name());
                }
            });
    });
    app.selected_abo = Some(selected);

    let r = app.db.get_follow_up(selected);
    let mut sheet = match r {
        Ok(s) => s,
        Err(e) => {
            app.set_error(e);
            return;
        }
    };
    let before = sheet.clone();

    ui.add_space(6.0);
    ui.add(
        egui::ProgressBar::new(sheet.fraction()).text(format!(
            "{} / {} ({:.0}%)",
            sheet.done_count(),
            FollowUpSheet::TOTAL,
            sheet.fraction() * 100.0
        )),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        section(ui, "BK1", |ui| {
            ui.checkbox(&mut sheet.bk1_jumpstart1, "Jumpstart Pack 1");
            ui.checkbox(&mut sheet.bk1_core_plan, "รายได้ 10 ขั้นตอน / Core Plus");
            ui.checkbox(&mut sheet.bk1_why_amway, "Why Amway");
            ui.checkbox(&mut sheet.bk1_why_nutrilite, "Why Nutrilite / Smart Health");
            ui.checkbox(&mut sheet.bk1_closed, "ปิดการสมัคร");
            ui.checkbox(&mut sheet.bk1_jumpstart2, "Jumpstart Pack 2");
            ui.checkbox(&mut sheet.bk1_why_artistry, "Why Artistry / Smart Look");
            ui.checkbox(&mut sheet.bk1_smart_home_tech, "Smart Home / Tech / Care");
            ui.checkbox(&mut sheet.bk1_aec_health, "AEC Health Check A-Start SOP");
        });

        section(ui, "BK2", |ui| {
            ui.checkbox(&mut sheet.bk2_jumpstart3, "Jumpstart Pack 3");
            ui.checkbox(&mut sheet.bk2_space_to_grow, "Space to Grow / CCS Guide");
            ui.checkbox(&mut sheet.bk2_100_dreams, "100 ความฝัน / ทำภาพความฝัน");
            ui.checkbox(&mut sheet.bk2_5f1f, "สอนการทำ 5F + 1F");
            ui.checkbox(&mut sheet.bk2_name_list, "เขียนรายชื่อ เช็คฟอร์ม สร้างนัดจริง");
            ui.checkbox(&mut sheet.bk2_study_table, "ลงตารางเรียนรู้ ขายบัตร ชวนคนเข้างาน");
            ui.checkbox(&mut sheet.bk2_analysis, "วิเคราะห์การทำงานกับอัพไลน์");
        });

        section(ui, "C1 Qualification", |ui| {
            ui.checkbox(&mut sheet.c1_link3, "พบสมอง 3 แพค");
            ui.checkbox(&mut sheet.c1_weekly_meeting, "เข้าร่วมประชุมประจำสัปดาห์");
            ui.checkbox(&mut sheet.c1_ccs_seminar, "เข้าร่วมสัมมนาของ CCS");
            ui.checkbox(&mut sheet.c1_auto_renewal, "สมัครต่ออายุนักธุรกิจอัตโนมัติ");
            ui.checkbox(&mut sheet.c1_sop, "สมัคร SOP");
            ui.checkbox(&mut sheet.c1_1abo, "1 ABO");
            ui.checkbox(&mut sheet.c1_5000pv, "ยอดธุรกิจ 5,000 PV");
        });

        section(ui, "CCS Conference", |ui| {
            ui.checkbox(&mut sheet.conf_crack_code, "Crack The Code");
            ui.checkbox(&mut sheet.conf_5stars, "5 Stars");
            ui.checkbox(&mut sheet.conf_spirit, "The Spirit");
        });
    });

    // Persist only when something actually changed.
    if sheet != before {
        sheet.updated_at = Local::now();
        if let Err(e) = app.db.save_follow_up(&sheet) {
            app.set_error(e);
        }
    }
}

/// A collapsible section header with the supplied checkboxes inside.
fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).color(ACCENT_STRONG).strong())
        .default_open(true)
        .show(ui, add);
}
