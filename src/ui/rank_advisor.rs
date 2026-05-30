//! Rank Advisor modal.
//!
//! For a selected ABO it evaluates the "5 Steps to 21%" rank conditions from
//! their Personal Point Value (PPV, editable here) and the number of direct
//! downline legs that reach each rank, shows a per-rank checklist, and offers to
//! apply the qualified rank.

use crate::app::AppState;
use crate::models::enums::Rank;
use crate::ui::{ACCENT, ACCENT_STRONG};
use crate::utils::scoring;

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(id) = app.rank_advisor else {
        return;
    };

    let contact = match app.db.get_contact(id) {
        Ok(c) => c,
        Err(e) => {
            app.set_error(e);
            app.rank_advisor = None;
            return;
        }
    };
    let (c1_legs, cl_legs, cl15_legs) = app.db.abo_leg_counts(id).unwrap_or((0, 0, 0));
    let current = contact.rank.unwrap_or(Rank::Koc);

    let mut ppv = contact.ppv;
    let mut open = true;
    let mut apply = false;
    let mut close = false;

    egui::Window::new("ประเมินระดับ / Rank Advisor")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.label(egui::RichText::new(contact.display_name()).size(18.0).strong());
            ui.horizontal(|ui| {
                ui.label("ระดับปัจจุบัน:");
                ui.colored_label(ACCENT_STRONG, current.as_str());
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("ยอดส่วนตัว (PPV):");
                ui.add(
                    egui::DragValue::new(&mut ppv)
                        .range(0..=10_000_000)
                        .speed(100.0),
                );
            });
            ui.label(
                egui::RichText::new(format!(
                    "สายงานดาวน์ไลน์ (ตรง):  C1+ = {c1_legs}   CL+ = {cl_legs}   CL15+ = {cl15_legs}"
                ))
                .small()
                .weak(),
            );

            let qualified = scoring::qualified_rank(ppv, c1_legs, cl_legs, cl15_legs);

            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("ระดับที่ผ่านเงื่อนไข:");
                ui.label(
                    egui::RichText::new(qualified.as_str())
                        .size(22.0)
                        .strong()
                        .color(ACCENT),
                );
                ui.label(
                    egui::RichText::new(format!("(โบนัส {}%)", scoring::bonus_percent_for_pv(ppv)))
                        .small()
                        .weak(),
                );
            });

            ui.add_space(6.0);
            ui.label(egui::RichText::new("เงื่อนไขแต่ละระดับ").strong());
            for rank in [Rank::C1, Rank::Cl, Rank::Cl15, Rank::Cl21] {
                if let Some((min_ppv, leg_rank, legs)) = scoring::rank_requirement(rank) {
                    let have = match leg_rank {
                        Rank::Cl => cl_legs,
                        Rank::Cl15 => cl15_legs,
                        _ => c1_legs,
                    };
                    let ok = ppv >= min_ppv && (legs == 0 || have >= legs);
                    let mut txt = format!("{}: PPV >= {}", rank.as_str(), min_ppv);
                    if legs > 0 {
                        txt.push_str(&format!(
                            " + {} {}+ สาย (มี {})",
                            legs,
                            leg_rank.as_str(),
                            have
                        ));
                    }
                    let color = if ok {
                        egui::Color32::from_rgb(0x2E, 0x7D, 0x32)
                    } else {
                        egui::Color32::from_rgb(0xC6, 0x28, 0x28)
                    };
                    ui.colored_label(color, format!("{} {}", if ok { "✅" } else { "❌" }, txt));
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.horizontal(|ui| {
                if qualified.ordinal() > current.ordinal() {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(format!("▲ ปรับระดับเป็น {}", qualified.as_str()))
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(ACCENT),
                        )
                        .clicked()
                    {
                        apply = true;
                    }
                } else {
                    ui.weak("ระดับปัจจุบันเหมาะสมแล้ว");
                }
                if ui.button("ปิด").clicked() {
                    close = true;
                }
            });
        });

    let qualified = scoring::qualified_rank(ppv, c1_legs, cl_legs, cl15_legs);

    if apply {
        let mut c = contact.clone();
        c.ppv = ppv;
        c.rank = Some(qualified);
        match app.db.update_contact(&c) {
            Ok(()) => {
                app.set_status(format!(
                    "ปรับระดับ {} เป็น {}",
                    contact.display_name(),
                    qualified.as_str()
                ));
                app.rank_advisor = None;
            }
            Err(e) => app.set_error(e),
        }
    } else {
        // Persist a PPV edit even if the user just closes the dialog.
        if ppv != contact.ppv {
            if let Err(e) = app.db.update_ppv(id, ppv) {
                app.set_error(e);
            }
        }
        if close || !open {
            app.rank_advisor = None;
        }
    }
}
