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
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let safety_str = safety
            .to_str()
            .ok_or_else(|| AppError::validation("เส้นทางไฟล์สำรองไม่ถูกต้อง"))?;
        conn.execute("VACUUM INTO ?1", params![safety_str])?;
    } // connection dropped here, releasing the file

    // 3. Swap: copy to a temp file beside the database, then rename over it.
    //    On Windows std::fs::rename replaces the destination (MoveFileEx with
    //    REPLACE_EXISTING). Same-volume rename keeps the window tiny; clean up the
    //    temp file if the rename fails so a stale copy isn't left behind.
    let tmp = db_path.with_extension("restore-tmp");
    std::fs::copy(src, &tmp)?;
    if let Err(e) = std::fs::rename(&tmp, db_path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }

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
        // Validation fails before any safety-backup, so backups_dir is untouched.
        assert!(!backups.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
