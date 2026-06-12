//! Settings "Data & Backup" section: back up the database to a user-chosen file
//! and restore from a backup file. Restore goes through a confirmation modal and
//! an automatic safety backup (see `AppState::perform_restore`).

use chrono::Local;

use crate::app::AppState;

/// Draw the backup/restore section inside the Settings page.
pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(egui::RichText::new("ข้อมูลและการสำรอง (Data & Backup)").strong());
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "สำรองข้อมูลเก็บไว้เป็นไฟล์ หรือกู้คืนจากไฟล์สำรอง — การกู้คืนจะเขียนทับข้อมูลปัจจุบันทั้งหมด",
        )
        .small()
        .weak(),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("💾  สำรองข้อมูล").clicked() {
            backup(app);
        }
        if ui.button("♻  กู้คืนข้อมูล").clicked() {
            pick_restore_file(app);
        }
    });
}

/// Open a Save dialog and back up to the chosen path.
fn backup(app: &mut AppState) {
    let default_name = crate::backup::default_backup_filename(Local::now().naive_local());
    let mut dialog = rfd::FileDialog::new()
        .set_file_name(default_name)
        .add_filter("ฐานข้อมูล SQLite", &["db"]);
    if let Ok(downloads) = crate::promo::downloads_dir() {
        dialog = dialog.set_directory(downloads);
    }
    let Some(path) = dialog.save_file() else {
        return; // cancelled
    };
    match app.db.backup_to(&path) {
        Ok(()) => app.set_status(format!("สำรองข้อมูลแล้ว: {}", path.display())),
        Err(e) => app.set_error(e),
    }
}

/// Open an Open dialog; a chosen file is staged for the confirm modal.
fn pick_restore_file(app: &mut AppState) {
    let mut dialog = rfd::FileDialog::new().add_filter("ฐานข้อมูล SQLite", &["db"]);
    if let Ok(downloads) = crate::promo::downloads_dir() {
        dialog = dialog.set_directory(downloads);
    }
    if let Some(path) = dialog.pick_file() {
        app.pending_restore = Some(path);
    }
}

/// Restore-confirmation modal. Rendered from the top-level update loop so it
/// floats over any view. Performs the restore only on explicit confirm.
pub fn render_restore_confirm(app: &mut AppState, ctx: &egui::Context) {
    let Some(src) = app.pending_restore.clone() else {
        return;
    };
    let filename = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| src.display().to_string());

    let mut confirm = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("ยืนยันการกู้คืน")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label("การกู้คืนจะเขียนทับข้อมูลปัจจุบันทั้งหมด");
            ui.label("ระบบจะสำรองข้อมูลปัจจุบันไว้อัตโนมัติก่อน ดำเนินการต่อหรือไม่?");
            ui.label(
                egui::RichText::new(format!("ไฟล์: {filename}"))
                    .small()
                    .weak(),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("♻ กู้คืน").color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(0xD3, 0x2F, 0x2F)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        app.perform_restore(src);
        app.pending_restore = None;
    } else if cancel || !open {
        app.pending_restore = None;
    }
}
