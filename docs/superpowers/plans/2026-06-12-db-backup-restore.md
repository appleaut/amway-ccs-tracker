# Database Backup/Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Settings-page buttons to back up the SQLite database to a user-chosen file and restore from a backup file, with integrity validation, a confirmation dialog, and an automatic pre-restore safety backup.

**Architecture:** A new `src/backup.rs` holds the non-UI logic (filename helper, SQLite validation, the validate→safety-backup→atomic-swap restore). `DbConnection::backup_to` wraps SQLite `VACUUM INTO`. A thin `src/ui/settings_backup.rs` renders the section and the restore-confirm modal using native `rfd` file dialogs; `app.rs` owns the drop-and-reopen-connection restore step and the `pending_restore` state.

**Tech Stack:** Rust, eframe/egui 0.28, rusqlite 0.31 (bundled SQLite), chrono, rfd 0.14 (native file dialogs).

**Conventions for every task:**
- This repo is **hand-formatted** — NEVER run `cargo fmt` (no `rustfmt.toml`; it reformats all files). Verify only with `cargo build` / `cargo test`.
- Every commit message must end with the line:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Match surrounding comment density and idiom. Thai user-facing strings, English doc-comments.

---

### Task 1: Add `rfd` dependency and `DbConnection::backup_to`

**Files:**
- Modify: `Cargo.toml:12-21` (dependencies)
- Modify: `src/db/mod.rs:14-17` (imports), `src/db/mod.rs:43-49` (add method after `open`)
- Test: `src/db/mod.rs` (new `#[cfg(test)] mod tests` at end of file)

- [ ] **Step 1: Add the `rfd` dependency**

In `Cargo.toml`, under `[dependencies]`, add `rfd` after the `png` line:

```toml
png = "0.17"
rfd = "0.14"
```

- [ ] **Step 2: Run build to fetch the crate**

Run: `cargo build`
Expected: compiles successfully (downloads `rfd` and its transitive deps on first run).

- [ ] **Step 3: Widen the rusqlite/error imports in `src/db/mod.rs`**

Change the two import lines:

```rust
use rusqlite::Connection;

use crate::error::Result;
```

to:

```rust
use rusqlite::{params, Connection};

use crate::error::{AppError, Result};
```

(`std::path::Path` is already imported on the line above.)

- [ ] **Step 4: Add the `backup_to` method**

In `src/db/mod.rs`, immediately after the `open` method (which ends at the line `Ok(DbConnection { conn })` / `}` around line 49), add:

```rust
    /// Write a clean, compact, consistent copy of the live database to `dest`
    /// using SQLite `VACUUM INTO`. The connection stays open. `VACUUM INTO`
    /// refuses a pre-existing destination, so an existing `dest` (the OS Save
    /// dialog already got the user's overwrite consent) is removed first.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        let dest_str = dest.to_str().ok_or_else(|| {
            AppError::validation("เส้นทางไฟล์ไม่ถูกต้อง (มีอักขระที่ไม่รองรับ)")
        })?;
        self.conn.execute("VACUUM INTO ?1", params![dest_str])?;
        Ok(())
    }
```

- [ ] **Step 5: Write the round-trip test**

At the very end of `src/db/mod.rs` (after the closing `}` of `impl DbConnection`), add a test module. It seeds a contact, backs up to a temp file, reopens the copy, and confirms the row survives:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::contact::Contact;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A unique temp path that won't collide across parallel tests (no RNG/clock,
    /// which are unavailable/forbidden — use pid + a counter).
    fn temp_path(tag: &str) -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "amway_db_test_{}_{}_{}.db",
            std::process::id(),
            tag,
            n
        ))
    }

    #[test]
    fn backup_to_copies_live_data() {
        let live = temp_path("live");
        let copy = temp_path("copy");

        let db = DbConnection::open(&live).unwrap();
        let mut c = Contact::new_blank();
        c.name = "สมหญิง".to_string();
        db.insert_contact(&c).unwrap();

        db.backup_to(&copy).unwrap();

        let restored = DbConnection::open(&copy).unwrap();
        let names: Vec<String> = restored
            .list_contacts()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&"สมหญิง".to_string()));

        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&copy);
    }
}
```

- [ ] **Step 6: Run the test**

Run: `cargo test --lib backup_to_copies_live_data`
Expected: PASS (1 passed).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/db/mod.rs
git commit -m "Add rfd dependency and DbConnection::backup_to

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `src/backup.rs` — filename helper, validation, and `restore_from`

**Files:**
- Create: `src/backup.rs`
- Modify: `src/main.rs:8-14` (add `mod backup;`)
- Test: `src/backup.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Create `src/backup.rs` with the implementation**

```rust
//! Database backup/restore logic, kept free of any UI dependency so it can be
//! unit-tested. Backups are plain SQLite files produced by `VACUUM INTO`
//! (see `DbConnection::backup_to`); restore validates a chosen file, safety-backs
//! up the current database, then atomically swaps it into place.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OpenFlags};

use crate::error::{AppError, Result};

/// Suggested filename for a manual backup, e.g.
/// `"amway-ccs-backup-20260612-143005.db"`.
pub fn default_backup_filename(now: NaiveDateTime) -> String {
    format!("amway-ccs-backup-{}.db", now.format("%Y%m%d-%H%M%S"))
}

/// Confirm `path` is a real, intact app database before we ever overwrite the
/// live one. Opens read-only, runs `PRAGMA integrity_check`, and requires the
/// `contacts` table to exist. Returns a Thai error otherwise.
fn validate_sqlite_db(path: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| AppError::validation("เปิดไฟล์สำรองไม่ได้"))?;

    // A non-SQLite file fails this query ("file is not a database").
    let check: String = match conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)) {
        Ok(s) => s,
        Err(_) => {
            return Err(AppError::validation(
                "ไฟล์นี้ไม่ใช่ฐานข้อมูล SQLite ที่ถูกต้อง",
            ))
        }
    };
    if check != "ok" {
        return Err(AppError::validation("ไฟล์สำรองเสียหาย กู้คืนไม่ได้"));
    }

    // An empty 0-byte file is a "valid" but tableless SQLite DB — reject it here.
    let has_contacts: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='contacts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_contacts {
        return Err(AppError::validation(
            "ไฟล์นี้ไม่ใช่ฐานข้อมูลของแอป (ไม่พบตาราง contacts)",
        ));
    }
    Ok(())
}

/// Restore the database at `db_path` from `src`. Validates `src`, writes a
/// timestamped safety copy of the CURRENT database into `backups_dir`, then
/// atomically swaps `src` into `db_path`. Returns the safety-backup path.
///
/// The caller MUST have already dropped the live `DbConnection` so `db_path` is
/// not locked, and MUST reopen it afterwards.
pub fn restore_from(
    src: &Path,
    db_path: &Path,
    backups_dir: &Path,
    now: NaiveDateTime,
) -> Result<PathBuf> {
    // 1. Validate before any destructive step.
    validate_sqlite_db(src)?;

    // 2. Safety-backup the current database.
    std::fs::create_dir_all(backups_dir)?;
    let safety = backups_dir.join(format!("pre-restore-{}.db", now.format("%Y%m%d-%H%M%S")));
    {
        let conn = Connection::open(db_path)?;
        let safety_str = safety
            .to_str()
            .ok_or_else(|| AppError::validation("เส้นทางไฟล์สำรองไม่ถูกต้อง"))?;
        conn.execute("VACUUM INTO ?1", params![safety_str])?;
    } // connection dropped here, releasing the file

    // 3. Atomic swap: copy to a temp file beside data.db, then rename over it.
    //    std::fs::rename replaces the destination atomically on Windows.
    let dir = db_path
        .parent()
        .ok_or_else(|| AppError::validation("ไม่พบโฟลเดอร์ฐานข้อมูล"))?;
    let tmp = dir.join("data.db.restore-tmp");
    std::fs::copy(src, &tmp)?;
    std::fs::rename(&tmp, db_path)?;

    Ok(safety)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn fixed_now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 6, 12)
            .unwrap()
            .and_hms_opt(14, 30, 5)
            .unwrap()
    }

    /// Unique temp directory per test (pid + counter; no RNG/clock available).
    fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "amway_backup_test_{}_{}_{}",
            std::process::id(),
            tag,
            n
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Create a real SQLite app DB at `path` seeded with one contact named `name`.
    fn make_db(path: &Path, name: &str) {
        let db = crate::db::DbConnection::open(path).unwrap();
        let mut c = crate::models::contact::Contact::new_blank();
        c.name = name.to_string();
        db.insert_contact(&c).unwrap();
    }

    fn contact_names(path: &Path) -> Vec<String> {
        crate::db::DbConnection::open(path)
            .unwrap()
            .list_contacts()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    #[test]
    fn default_backup_filename_formats_timestamp() {
        assert_eq!(
            default_backup_filename(fixed_now()),
            "amway-ccs-backup-20260612-143005.db"
        );
    }

    #[test]
    fn validate_accepts_real_db_rejects_garbage_and_empty() {
        let dir = temp_dir("validate");

        let good = dir.join("good.db");
        make_db(&good, "ทดสอบ");
        assert!(validate_sqlite_db(&good).is_ok());

        let garbage = dir.join("garbage.db");
        std::fs::write(&garbage, b"this is not a database").unwrap();
        assert!(validate_sqlite_db(&garbage).is_err());

        let empty = dir.join("empty.db");
        std::fs::write(&empty, b"").unwrap();
        assert!(validate_sqlite_db(&empty).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_swaps_data_and_keeps_safety_backup() {
        let dir = temp_dir("restore");
        let backups = dir.join("backups");

        let live = dir.join("data.db");
        make_db(&live, "ข้อมูลเดิม"); // current data
        let source = dir.join("source.db");
        make_db(&source, "ข้อมูลใหม่"); // backup to restore from

        let safety = restore_from(&source, &live, &backups, fixed_now()).unwrap();

        // Live DB now holds the source's data.
        assert!(contact_names(&live).contains(&"ข้อมูลใหม่".to_string()));
        // Safety backup preserved the original data.
        assert!(safety.exists());
        assert!(contact_names(&safety).contains(&"ข้อมูลเดิม".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_rejects_invalid_source_without_touching_live() {
        let dir = temp_dir("reject");
        let backups = dir.join("backups");

        let live = dir.join("data.db");
        make_db(&live, "ข้อมูลเดิม");
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"not a db").unwrap();

        let err = restore_from(&bad, &live, &backups, fixed_now());
        assert!(err.is_err());
        // Live DB untouched.
        assert!(contact_names(&live).contains(&"ข้อมูลเดิม".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Register the module in `src/main.rs`**

Add `mod backup;` between `mod app;` and `mod db;` so the list reads:

```rust
mod app;
mod backup;
mod db;
mod error;
mod models;
mod promo;
mod ui;
mod utils;
```

- [ ] **Step 3: Run the backup tests**

Run: `cargo test --lib backup::`
Expected: PASS — `default_backup_filename_formats_timestamp`, `validate_accepts_real_db_rejects_garbage_and_empty`, `restore_swaps_data_and_keeps_safety_backup`, `restore_rejects_invalid_source_without_touching_live` (4 passed).

- [ ] **Step 4: Commit**

```bash
git add src/backup.rs src/main.rs
git commit -m "Add backup module: filename helper, validation, restore_from

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: App wiring — path helpers, `pending_restore` state, `perform_restore`

**Files:**
- Modify: `src/app.rs:58-59` (add `pending_restore` field), `src/app.rs:158` (init), `src/app.rs:212` (add `perform_restore` after `set_saved_image`), `src/app.rs:597-604` (make `db_path` `pub(crate)`, add `backups_dir`)

- [ ] **Step 1: Add the `pending_restore` field to `AppState`**

In `src/app.rs`, right after the `pending_delete` field (line 59, `pub pending_delete: Option<ui::confirm::PendingDelete>,`), add:

```rust
    /// Backup file the user picked to restore from, awaiting confirmation.
    pub pending_restore: Option<PathBuf>,
```

- [ ] **Step 2: Initialize it in `AppState::new`**

In the struct-literal returned by `AppState::new`, right after the `pending_delete: None,` line (around line 158), add:

```rust
            pending_restore: None,
```

- [ ] **Step 3: Make `db_path` crate-visible and add `backups_dir`**

In `src/app.rs`, change the signature:

```rust
fn db_path() -> Result<PathBuf> {
```

to:

```rust
pub(crate) fn db_path() -> Result<PathBuf> {
```

Then, immediately after the `db_path` function (after its closing `}` near line 604), add:

```rust
/// Resolve `%APPDATA%\AmwayCCSTracker\backups`, creating the directory.
pub(crate) fn backups_dir() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|_| AppError::validation("APPDATA environment variable is not set"))?;
    let dir = PathBuf::from(base).join("AmwayCCSTracker").join("backups");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
```

- [ ] **Step 4: Add the `perform_restore` method**

In `src/app.rs`, inside `impl AppState`, right after the `set_saved_image` method (ends around line 212), add:

```rust
    /// Restore the database from `src`: drop the live connection so the file is
    /// unlocked, run the validate→safety-backup→swap, then reopen. On any error,
    /// reopen the still-intact original so the app always ends with a live
    /// connection.
    pub fn perform_restore(&mut self, src: PathBuf) {
        let now = Local::now().naive_local();
        let result = (|| -> Result<PathBuf> {
            let db_file = db_path()?;
            let backups = backups_dir()?;
            // Open the scratch DB BEFORE replacing, so a failure here leaves the
            // real connection intact.
            let scratch = DbConnection::open(std::path::Path::new(":memory:"))?;
            let _ = std::mem::replace(&mut self.db, scratch); // drops the real conn
            let safety = crate::backup::restore_from(&src, &db_file, &backups, now)?;
            self.db = DbConnection::open(&db_file)?;
            Ok(safety)
        })();
        match result {
            Ok(safety) => self.set_status(format!(
                "กู้คืนข้อมูลแล้ว (สำรองเดิมไว้ที่ {})",
                safety.display()
            )),
            Err(e) => {
                // Ensure a live connection on the original data.db before reporting.
                if let Ok(p) = db_path() {
                    if let Ok(db) = DbConnection::open(&p) {
                        self.db = db;
                    }
                }
                self.set_error(e);
            }
        }
    }
```

- [ ] **Step 5: Build to confirm it compiles (field, init, methods wired)**

Run: `cargo build`
Expected: compiles (a warning that `perform_restore` / `pending_restore` are unused is fine — Task 4 wires them).

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "Wire backup paths, pending_restore state, and perform_restore

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Settings UI section + restore-confirm modal

**Files:**
- Create: `src/ui/settings_backup.rs`
- Modify: `src/ui/mod.rs` (add `pub mod settings_backup;`)
- Modify: `src/app.rs` (call section renderer inside `settings()`; call modal in `update()`)

- [ ] **Step 1: Create `src/ui/settings_backup.rs`**

```rust
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
        Ok(()) => app.set_saved_image(
            format!("สำรองข้อมูลแล้ว: {}", path.display()),
            path.display().to_string(),
        ),
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
```

- [ ] **Step 2: Register the module in `src/ui/mod.rs`**

Add the declaration alongside the other `pub mod` lines (e.g. right after `pub mod promo_download;`):

```rust
pub mod settings_backup;
```

- [ ] **Step 3: Call the section renderer from `settings()`**

In `src/app.rs`, inside `fn settings(&mut self, ui: &mut egui::Ui)`, after the weak network-note label block (the `ui.label(egui::RichText::new("ข้อมูลรายชื่อถูกบันทึกในเครื่อง ...").small().weak());` ending around line 352) and BEFORE the rank-calculator block (`ui.add_space(12.0); ui.separator();` at line 354), insert:

```rust
        ui::settings_backup::render(self, ui);
```

- [ ] **Step 4: Call the restore-confirm modal from `update()`**

In `src/app.rs`, in the modal-rendering block of `update()`, immediately after the line `ui::confirm::render(self, ctx);` (around line 496), add:

```rust
        ui::settings_backup::render_restore_confirm(self, ctx);
```

- [ ] **Step 5: Build and run the full test suite**

Run: `cargo build`
Expected: compiles with no warnings about unused `perform_restore` / `pending_restore`.

Run: `cargo test`
Expected: all tests pass (the prior 90 + the 5 new ones = 95 passed; 0 failed).

- [ ] **Step 6: Commit**

```bash
git add src/ui/settings_backup.rs src/ui/mod.rs src/app.rs
git commit -m "Add Settings backup/restore UI section and confirm modal

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Glyph verification, manual round-trip, and finish

**Files:** none (verification only), unless a glyph fix is needed in `src/ui/settings_backup.rs`.

- [ ] **Step 1: Launch the app and screenshot the Settings page**

Build and run the app, navigate to the ⚙ ตั้งค่า view, and capture the window to confirm the new section renders. Use the established Windows screenshot approach (PowerShell + `Add-Type` C# `PrintWindow` against the process `MainWindowHandle`). The earlier promo-downloader work used this same technique.

Run (background the app, then capture):
```
cargo run --release
```
Then capture the window of the `amway_ccs_tracker` process to a PNG and open it.

- [ ] **Step 2: Verify the `💾` and `♻` glyphs render (not tofu)**

Look at the two buttons:
- `💾  สำรองข้อมูล` — `💾` is already used elsewhere in the app and is known-good.
- `♻  กู้คืนข้อมูล` — confirm `♻` (U+267B) shows a recycle glyph, not a tofu box.

**If `♻` renders as tofu:** it is not in egui's bundled font subset. Fix by dropping the leading glyph from BOTH the section button and the modal confirm button — change `"♻  กู้คืนข้อมูล"` to `"กู้คืนข้อมูล"` and `"♻ กู้คืน"` to `"กู้คืน"` in `src/ui/settings_backup.rs`. Rebuild and re-screenshot to confirm. (Do not substitute an unverified emoji.)

- [ ] **Step 3: Manual backup→restore round-trip**

In the running app:
1. Click `💾 สำรองข้อมูล`, save to `Downloads\test-backup.db`. Confirm the status bar shows the saved path and the file exists.
2. Add or delete a contact so the live data differs from the backup.
3. Click `♻ กู้คืนข้อมูล`, pick `test-backup.db`, confirm in the modal.
4. Confirm the contact list reverts to the backup's state and the status bar reports the safety-backup path.
5. Confirm a `pre-restore-*.db` file exists in `%APPDATA%\AmwayCCSTracker\backups\`.

- [ ] **Step 4: Commit any glyph fix (only if Step 2 required one)**

```bash
git add src/ui/settings_backup.rs
git commit -m "Use text-only restore label (glyph not in font subset)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 5: Finish the branch**

Use the **superpowers:finishing-a-development-branch** skill: verify `cargo test` passes, then present the merge/PR options to the user. (Per project rule, do NOT merge to main without explicit user approval.)

---

## Notes for the implementer

- **rfd dialogs are blocking** — `save_file()` / `pick_file()` block the UI thread while the native dialog is open. That is acceptable here (a deliberate, momentary modal action), unlike the promo downloader's long-running work which needed a background thread. Do not add threading.
- **Why drop the connection for restore:** on Windows, SQLite holds `data.db` open; you cannot rename a file over it while the handle is live. `perform_restore` swaps in a `:memory:` scratch connection to drop the real one, then reopens after the swap.
- **`migrate()` on reopen** upgrades an older-schema backup automatically — no special handling needed.
- The `:memory:` scratch `DbConnection::open` runs `migrate()` against an empty in-memory DB; this is cheap and harmless.
