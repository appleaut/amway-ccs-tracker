# Database Backup/Restore Implementation Design

**Date:** 2026-06-12
**Status:** Approved (design), pending plan

## Goal

Let the user back up the app's SQLite database to a file they choose, and restore
the database from a backup file — safely — directly from the Settings page.

## Background

The app keeps all data in one SQLite file at `%APPDATA%\AmwayCCSTracker\data.db`.
`DbConnection` (in `src/db/mod.rs`) owns the single live `rusqlite::Connection`;
the UI talks only to `DbConnection`. The DB path is resolved by the private
`db_path()` in `src/app.rs`, which also runs on startup and is reused by the
chart-export feature. There is currently no way for a non-technical user to copy
their data off the machine or recover from an accident.

The app has no native file-dialog support yet (existing tools — chart export,
promo downloader — write to fixed folders and open Explorer). Backup/restore is
exactly the case where users expect a real file picker (to store a backup on a
USB drive or a synced folder), so we add the `rfd` crate for native dialogs.

## Locked Decisions

1. **File selection:** Native OS file dialogs via the `rfd` crate (option A) —
   a Save dialog for backup, an Open dialog for restore. Not a fixed
   app-managed folder.
2. **Restore safeguards:** BOTH a confirmation dialog AND an automatic
   safety-backup of the current database immediately before any overwrite.

## Architecture

Four layers with clear boundaries:

### `src/db/mod.rs` — `DbConnection::backup_to`

- New method `pub fn backup_to(&self, dest: &Path) -> Result<()>`.
- Implemented with SQLite `VACUUM INTO`, which writes a clean, compact,
  transactionally-consistent copy of the live database to `dest` without closing
  the connection.
- `VACUUM INTO` requires `dest` to NOT already exist. The Save dialog may return
  an existing path (the OS dialog already obtained the user's overwrite consent),
  so `backup_to` removes `dest` first if it exists, then runs `VACUUM INTO`. The
  safety-backup path is timestamped and thus always fresh.
- Thin wrapper over `queries`-style SQL; keeps the UI layer free of SQL.

### `src/backup.rs` (new, non-UI logic, unit-tested)

Pure-ish helpers with no egui dependency:

- `pub fn default_backup_filename(now: NaiveDateTime) -> String` →
  `"amway-ccs-backup-YYYYMMDD-HHMMSS.db"`.
- `fn validate_sqlite_db(path: &Path) -> Result<()>` — opens `path` read-only,
  runs `PRAGMA integrity_check` (must return `ok`), and confirms a known table
  (`contacts`) exists in `sqlite_master`. Returns a Thai error if the file is not
  a valid, intact app database. Never touches the live DB.
- `pub fn restore_from(src: &Path, db_path: &Path, backups_dir: &Path, now: NaiveDateTime) -> Result<PathBuf>` —
  performs the swap and returns the path of the safety-backup it created:
  1. `validate_sqlite_db(src)` — abort before any destructive step if invalid.
  2. Safety-backup: open the current `db_path` and `VACUUM INTO`
     `backups_dir\pre-restore-YYYYMMDD-HHMMSS.db` (creating `backups_dir`). This
     opens its own short-lived connection — the app's live `DbConnection` has
     already been dropped by the caller (see app wiring) so the file is not
     locked.
  3. Swap: copy `src` to a temp file in `db_path`'s directory, then rename the
     temp over `db_path` (atomic on the same volume), so a mid-copy failure can
     never leave a half-written `data.db`.
  4. Return the safety-backup path. (Reopening the connection is the caller's
     job — see app wiring — because `restore_from` doesn't own `DbConnection`.)

  On any error after step 2, the original `data.db` is still intact (the swap is
  the only mutating step and it is atomic), and the safety-backup exists.

### `src/ui/settings_backup.rs` (new, thin view section)

- `pub fn render(app: &mut AppState, ui: &mut egui::Ui)` draws a bordered
  "ข้อมูลและการสำรอง (Data & Backup)" section, called from `app.rs::settings`.
- Two buttons:
  - `💾  สำรองข้อมูล (Backup)` — opens an `rfd::FileDialog` Save dialog
    (`set_file_name(default_backup_filename(now))`, filter `*.db`, default
    directory = Downloads). On a chosen path, call `app.db.backup_to(&path)`; set
    status `สำรองข้อมูลแล้ว: <path>` via the clickable-path status mechanism, or
    `app.set_error` on failure. Cancelled dialog = no-op.
  - `♻  กู้คืนข้อมูล (Restore)` — opens an `rfd::FileDialog` Open dialog (filter
    `*.db`). On a chosen file, store it in `app.pending_restore = Some(path)` —
    this does NOT restore yet; it triggers the confirm modal below.
- Glyph check: verify `💾` and `♻` exist in the bundled Kanit font subset before
  shipping; if either renders as tofu, fall back to a text-only label for that
  button (per the egui bundled-font-subset constraint).

### `src/ui/settings_restore_confirm.rs` (new, small modal) — or inline in settings_backup

- `pub fn render(app: &mut AppState, ctx: &egui::Context)`: if
  `app.pending_restore` is `Some(path)`, show a centered modal titled
  `ยืนยันการกู้คืน` with the message:
  *"การกู้คืนจะเขียนทับข้อมูลปัจจุบันทั้งหมด ระบบจะสำรองข้อมูลปัจจุบันไว้อัตโนมัติก่อน
  ดำเนินการต่อหรือไม่?"* and the chosen filename.
- Buttons: a red `♻ กู้คืน` confirm and a `ยกเลิก` cancel. Cancel or closing the
  window clears `pending_restore`.
- On confirm, perform the restore (see app wiring), then clear `pending_restore`.

### `src/app.rs` wiring

- `db_path()` becomes `pub(crate)` so `backup.rs` and the restore flow can locate
  `data.db`; add a `pub(crate) fn backups_dir()` →
  `%APPDATA%\AmwayCCSTracker\backups` (created on demand).
- New `AppState` field `pub pending_restore: Option<std::path::PathBuf>` (+ init
  `None`).
- `settings()` calls `ui::settings_backup::render(self, ui)` for the new section.
- The top-level `update()` (where other modals like `confirm::render` are
  already called) also calls `ui::settings_restore_confirm::render(self, ctx)`.
- **Restore execution** (in the confirm modal's confirm branch). The live
  connection MUST be dropped before `restore_from` copies over `data.db` (Windows
  holds the file open/locked otherwise), and `self.db` MUST end pointing at a
  freshly-opened connection. Concrete flow:
  1. Compute `let db_path = db_path()?;` and `let backups_dir = backups_dir()?;`.
  2. Drop the live connection by swapping in a scratch in-memory DB:
     `let scratch = DbConnection::open(Path::new(":memory:"))?;`
     `let _ = std::mem::replace(&mut self.db, scratch);` — the real connection is
     dropped here, releasing the file lock. (`Connection::open(":memory:")` opens
     an in-memory DB; `migrate()` runs harmlessly against it.)
  3. `let safety = backup::restore_from(&src, &db_path, &backups_dir, now)?;`
     (validate → safety-backup → atomic swap).
  4. `self.db = DbConnection::open(&db_path)?;` — re-runs `migrate()`, so a backup
     made on an older schema is upgraded automatically.
  5. On success: status `กู้คืนข้อมูลแล้ว (สำรองเดิมไว้ที่ <safety>)`. On any error
     in steps 3–4: `set_error`, and reopen from the still-intact `data.db`
     (`self.db = DbConnection::open(&db_path)?` again — the swap is atomic so
     `data.db` is either fully original or fully restored).
  6. Clear `pending_restore`.

  `now` is sourced once via `chrono::Local::now().naive_local()` at the call site
  and threaded into `restore_from` so the function stays testable with a fixed
  timestamp.

### `Cargo.toml`

- Add `rfd = "0.14"`.

## Data Flow

**Backup:** button → rfd Save dialog → chosen path → `db.backup_to(path)`
(`VACUUM INTO`) → status bar shows clickable saved path.

**Restore:** button → rfd Open dialog → chosen file → `pending_restore = Some` →
confirm modal → confirm → drop live connection → `restore_from` (validate →
safety-backup current DB → atomic swap) → reopen `self.db` (runs migrate) →
refresh view → status bar shows restored + safety-backup location.

## Error Handling

All messages are Thai and surfaced via `set_error`/`set_status`; the UI always
returns to a usable state:

- Backup destination unwritable / `VACUUM INTO` fails → error; nothing changed.
- Restore source fails validation (`PRAGMA integrity_check` not `ok`, or
  `contacts` table missing, or not a SQLite file) → error BEFORE any overwrite;
  current DB untouched.
- Swap or reopen fails → error; `data.db` is still the original (swap is atomic),
  and `self.db` is reopened from it; the safety-backup also exists as a fallback.
- Dialog cancelled → silent no-op.

## Behavior Details

- **Backup filename:** suggested `amway-ccs-backup-<YYYYMMDD-HHMMSS>.db`; the user
  may rename or relocate freely in the Save dialog.
- **Safety-backup location:** always `%APPDATA%\AmwayCCSTracker\backups\
  pre-restore-<YYYYMMDD-HHMMSS>.db`, regardless of where the restore source came
  from.
- **Schema upgrades:** restoring an older-schema backup is safe — `migrate()` runs
  on reopen.
- **No auto-pruning:** old safety-backups are kept (YAGNI — the folder is small
  SQLite files; user can clean manually).

## Testing

- **Unit tests** (`src/backup.rs`):
  - `default_backup_filename` formatting for a fixed `NaiveDateTime`.
  - `validate_sqlite_db`: a freshly-created app DB passes; a text/garbage file is
    rejected; an empty file is rejected.
  - `restore_from` round-trip: build DB A (seed a contact) and DB B (different
    seed) in a temp dir, `restore_from(B, A_path, backups_dir, now)`, then open
    `A_path` and confirm B's data is present AND a `pre-restore-*.db` safety file
    was created that still contains A's data.
- **Unit test** (`src/db/mod.rs`): `backup_to` round-trip — open a DB, seed a
  row, `backup_to` a temp path, open the copy, confirm the row survives.
- **Manual verification:** in the running app, Backup → pick a path → file
  appears; Restore → pick that file → confirm → data swaps and view refreshes;
  verify the `💾`/`♻` glyphs render (not tofu); confirm a `pre-restore-*.db`
  lands in the backups folder.

## Out of Scope (YAGNI)

- Scheduled/automatic backups.
- Backup encryption or compression beyond what `VACUUM` already does.
- Cloud upload / sync integration.
- A backup history/management UI or auto-pruning of old backups.
- Exporting to non-SQLite formats (CSV, JSON).
- Partial/selective restore (single table or contact).
