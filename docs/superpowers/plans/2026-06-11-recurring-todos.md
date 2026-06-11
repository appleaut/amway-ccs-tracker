# Recurring (Scheduled) Todos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user define recurring task *schedules* on a dedicated page; the app auto-creates a normal Todo on the existing Todo List when a schedule's cycle date arrives or has passed.

**Architecture:** A new `todo_schedules` table (migration v11) backs a `TodoSchedule` model whose `Recurrence` enum (`EveryNDays` / `MonthlyDay`) owns the pure occurrence math. A `generate_due_todos(conn, today)` query materializes due cycles into `todos` inside a transaction, guarded by each schedule's `last_generated` date so missed cycles collapse into one and nothing duplicates. `AppState::update` runs generation on the first frame and whenever the calendar date changes. A new `todo_schedules` UI page manages schedules (mirrors the Advances page), with delete routed through the shared confirm modal.

**Tech Stack:** Rust, eframe/egui 0.28, egui_extras 0.28 (`TableBuilder`, `DatePickerButton`), rusqlite 0.31, chrono.

**Conventions (read before starting):**
- Do **NOT** run `cargo fmt` — this repo is hand-formatted (no rustfmt.toml). Verify only with `cargo build` / `cargo test`.
- Every commit message must end with the line:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Dates: `NaiveDate` is stored as `"%Y-%m-%d"` text; `DateTime<Local>` as RFC3339 (`Local::now().to_rfc3339()`), parsed back with the existing `parse_dt` helper.
- Tests in `src/db/queries.rs` use the in-memory helper `mem()` (a migrated `Connection`) and builders like `sample_prospect("ชื่อ")`. Reuse them.
- Run a single test with: `cargo test --lib <name>`. Run all with: `cargo test`.

---

## File Structure

- **Create** `src/models/todo_schedule.rs` — `Recurrence` enum, `TodoSchedule` struct, pure occurrence math (`latest_occurrence_on_or_before`, `next_occurrence_after`), `label_th`, DB-string mapping, and unit tests.
- **Modify** `src/models/mod.rs` — register the module + doc line.
- **Modify** `src/db/schema.rs` — migration v11 (`CURRENT_VERSION` 10 → 11) creating `todo_schedules`.
- **Modify** `src/db/queries.rs` — `TodoScheduleRow`, row mappers, CRUD (`add`/`update`/`delete`/`list`), `generate_due_todos`, and integration tests.
- **Modify** `src/db/mod.rs` — `DbConnection` passthroughs + imports.
- **Modify** `src/ui/confirm.rs` — `PendingDelete::TodoSchedule` variant + name/detail + dispatch arm.
- **Create** `src/ui/todo_schedules.rs` — the management page + `TodoScheduleForm`.
- **Modify** `src/ui/mod.rs` — register the module + `View::TodoSchedules`.
- **Modify** `src/app.rs` — new state fields, init, sidebar entry, dispatch arm, generation call site.

---

## Task 1: Migration v11 — `todo_schedules` table

**Files:**
- Modify: `src/db/schema.rs:11` (CURRENT_VERSION) and append a `version < 11` block.

- [ ] **Step 1: Bump the schema version**

In `src/db/schema.rs`, change the constant:

```rust
/// Current schema version understood by this build.
const CURRENT_VERSION: i64 = 11;
```

- [ ] **Step 2: Add the migration block**

In `migrate`, immediately after the `if version < 10 { … }` block and before the
`if version != CURRENT_VERSION` block, insert:

```rust
    if version < 11 {
        // Recurring task schedules: a template + cadence that auto-creates a
        // normal todo when a cycle is due. contact_id is nullable + SET NULL so
        // a schedule survives its contact being deleted (mirrors `todos`).
        // last_generated is the occurrence date of the most recent todo created
        // from this schedule (NULL = none yet) — it guards against duplicates.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS todo_schedules (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_id     INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
                task           TEXT    NOT NULL,
                freq_kind      TEXT    NOT NULL,
                freq_value     INTEGER NOT NULL,
                start_date     TEXT    NOT NULL,
                last_generated TEXT,
                created_at     TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_todo_schedules_contact ON todo_schedules(contact_id);",
        )?;
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: builds (the migration test is added in a later task, but the code must compile now).

- [ ] **Step 4: Commit**

```bash
git add src/db/schema.rs
git commit -m "Add migration v11 for todo_schedules table

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: `TodoSchedule` model + pure occurrence math

**Files:**
- Create: `src/models/todo_schedule.rs`
- Modify: `src/models/mod.rs`

- [ ] **Step 1: Create the model file with the failing tests**

Create `src/models/todo_schedule.rs` with the full content below. It contains
the types, the pure math, and the unit tests up front (TDD).

```rust
//! A recurring task schedule: a template (task + optional contact) plus a
//! cadence. When a cycle date arrives or has passed, the app materializes a
//! normal `Todo` from it (see `db::queries::generate_due_todos`). The occurrence
//! math here is pure (no DB, no clock) so it can be unit-tested in isolation.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};

/// How often a schedule fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    /// Every `n` days (n >= 1), phased from the schedule's `start_date`.
    EveryNDays(u32),
    /// On a fixed day-of-month (1..=31), clamped to the month's last day when
    /// that day does not exist (e.g. day 31 in February → 28/29).
    MonthlyDay(u32),
}

impl Recurrence {
    /// The DB discriminator string for `freq_kind`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Recurrence::EveryNDays(_) => "EveryNDays",
            Recurrence::MonthlyDay(_) => "MonthlyDay",
        }
    }

    /// The DB integer for `freq_value`.
    pub fn value(&self) -> i64 {
        match self {
            Recurrence::EveryNDays(n) => *n as i64,
            Recurrence::MonthlyDay(d) => *d as i64,
        }
    }

    /// Rebuild from the stored (`freq_kind`, `freq_value`) pair. Returns `None`
    /// for an unknown kind or an out-of-range value.
    pub fn from_db(kind: &str, value: i64) -> Option<Recurrence> {
        match kind {
            "EveryNDays" if value >= 1 => Some(Recurrence::EveryNDays(value as u32)),
            "MonthlyDay" if (1..=31).contains(&value) => Some(Recurrence::MonthlyDay(value as u32)),
            _ => None,
        }
    }

    /// Thai label for the cadence, e.g. "ทุก 7 วัน" / "ทุกวันที่ 1".
    pub fn label_th(&self) -> String {
        match self {
            Recurrence::EveryNDays(n) => format!("ทุก {n} วัน"),
            Recurrence::MonthlyDay(d) => format!("ทุกวันที่ {d}"),
        }
    }

    /// The most recent occurrence on or before `today`, not earlier than
    /// `start`. `None` when no occurrence has happened yet (`today < start`, or
    /// — for monthly — the first qualifying day-of-month is still in the future).
    pub fn latest_occurrence_on_or_before(
        &self,
        start: NaiveDate,
        today: NaiveDate,
    ) -> Option<NaiveDate> {
        match self {
            Recurrence::EveryNDays(n) => {
                if today < start {
                    return None;
                }
                let n = *n as i64;
                let k = (today - start).num_days() / n; // floor, both >= 0
                Some(start + Duration::days(k * n))
            }
            Recurrence::MonthlyDay(d) => {
                let this = occ_in_month(today.year(), today.month(), *d);
                let occ = if this <= today {
                    this
                } else {
                    let (py, pm) = prev_month(today.year(), today.month());
                    occ_in_month(py, pm, *d)
                };
                if occ < start {
                    None
                } else {
                    Some(occ)
                }
            }
        }
    }

    /// The next occurrence strictly after `after` that is also `>= start` — used
    /// only to show "รอบถัดไป" in the schedule table.
    pub fn next_occurrence_after(&self, start: NaiveDate, after: NaiveDate) -> NaiveDate {
        match self {
            Recurrence::EveryNDays(n) => {
                if start > after {
                    return start;
                }
                let n = *n as i64;
                let k = (after - start).num_days() / n + 1;
                start + Duration::days(k * n)
            }
            Recurrence::MonthlyDay(d) => {
                let (mut y, mut m) = if start > after {
                    (start.year(), start.month())
                } else {
                    (after.year(), after.month())
                };
                loop {
                    let occ = occ_in_month(y, m, *d);
                    if occ > after && occ >= start {
                        return occ;
                    }
                    let (ny, nm) = next_month(y, m);
                    y = ny;
                    m = nm;
                }
            }
        }
    }
}

/// A recurring schedule row. `last_generated` is the occurrence date of the most
/// recent `Todo` created from it (`None` = none yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoSchedule {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub task: String,
    pub recurrence: Recurrence,
    pub start_date: NaiveDate,
    pub last_generated: Option<NaiveDate>,
    pub created_at: DateTime<Local>,
}

/// Day-of-month `day` in (`year`, `month`), clamped to the month's last day.
fn occ_in_month(year: i32, month: u32, day: u32) -> NaiveDate {
    let d = day.min(last_day_of_month(year, month));
    NaiveDate::from_ymd_opt(year, month, d).expect("clamped day is valid")
}

/// Number of days in (`year`, `month`): the day before the first of next month.
fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = next_month(year, month);
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .expect("first of month is valid")
        .pred_opt()
        .expect("has a previous day")
        .day()
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn every_n_days_latest_occurrence() {
        let r = Recurrence::EveryNDays(7);
        let start = d("2026-06-01");
        // Before start → none.
        assert_eq!(r.latest_occurrence_on_or_before(start, d("2026-05-31")), None);
        // Exactly on start → start.
        assert_eq!(r.latest_occurrence_on_or_before(start, start), Some(start));
        // Mid-cycle (start 1, N=7, today 23) → 22.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-23")),
            Some(d("2026-06-22"))
        );
        // Exactly on a cycle boundary → that day.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-15")),
            Some(d("2026-06-15"))
        );
    }

    #[test]
    fn every_n_days_next_occurrence() {
        let r = Recurrence::EveryNDays(7);
        let start = d("2026-06-01");
        // Future start → start itself.
        assert_eq!(r.next_occurrence_after(start, d("2026-05-20")), start);
        // On start → next cycle.
        assert_eq!(r.next_occurrence_after(start, start), d("2026-06-08"));
        // Mid-cycle → next boundary.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-23")), d("2026-06-29"));
    }

    #[test]
    fn monthly_day_latest_occurrence() {
        let r = Recurrence::MonthlyDay(15);
        let start = d("2026-01-15");
        // Today after the 15th → this month's 15th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-20")),
            Some(d("2026-06-15"))
        );
        // Today before the 15th → previous month's 15th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-10")),
            Some(d("2026-05-15"))
        );
        // today < start → none.
        assert_eq!(r.latest_occurrence_on_or_before(start, d("2026-01-10")), None);
    }

    #[test]
    fn monthly_day_clamps_to_month_end() {
        let r = Recurrence::MonthlyDay(31);
        let start = d("2026-01-01");
        // February 2026 has 28 days → the 31st clamps to the 28th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-02-28")),
            Some(d("2026-02-28"))
        );
        // On Feb 27 the latest is January's 31st.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-02-27")),
            Some(d("2026-01-31"))
        );
    }

    #[test]
    fn monthly_day_next_occurrence() {
        let r = Recurrence::MonthlyDay(10);
        let start = d("2026-01-10");
        // Mid-month, after the 10th → next month's 10th.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-25")), d("2026-07-10"));
        // Before the 10th → this month's 10th.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-05")), d("2026-06-10"));
        // Future start → the first occurrence on/after start.
        let future = d("2027-03-10");
        assert_eq!(r.next_occurrence_after(future, d("2026-06-25")), d("2027-03-10"));
    }

    #[test]
    fn from_db_round_trips_and_rejects_bad_values() {
        let a = Recurrence::EveryNDays(7);
        let b = Recurrence::MonthlyDay(15);
        assert_eq!(Recurrence::from_db(a.kind_str(), a.value()), Some(a));
        assert_eq!(Recurrence::from_db(b.kind_str(), b.value()), Some(b));
        assert_eq!(Recurrence::from_db("EveryNDays", 0), None);
        assert_eq!(Recurrence::from_db("MonthlyDay", 32), None);
        assert_eq!(Recurrence::from_db("Bogus", 5), None);
    }
}
```

- [ ] **Step 2: Register the module**

In `src/models/mod.rs`, add the doc line in the module list comment and the
`pub mod` declaration (keep alphabetical order — after `todo`):

```rust
//! * [`todo_schedule`] — a recurring task schedule (template + cadence) that auto-creates todos.
```

```rust
pub mod todo;
pub mod todo_schedule;
```

- [ ] **Step 3: Run the model tests**

Run: `cargo test --lib todo_schedule`
Expected: PASS — all six tests in the new module pass.

- [ ] **Step 4: Commit**

```bash
git add src/models/todo_schedule.rs src/models/mod.rs
git commit -m "Add TodoSchedule model with pure occurrence math

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: CRUD queries + row mapping

**Files:**
- Modify: `src/db/queries.rs` (add `use`, a new section after the Todos section near line 947, and tests in the `#[cfg(test)] mod tests`).

- [ ] **Step 1: Add the model import**

At the top of `src/db/queries.rs`, after `use crate::models::todo::Todo;`
(line ~21), add:

```rust
use crate::models::todo_schedule::{Recurrence, TodoSchedule};
```

- [ ] **Step 2: Add the schedule queries section**

In `src/db/queries.rs`, immediately after the `list_todos` / `count_*_todos`
functions (i.e. after `count_due_soon_todos`, around line 975) and before the
Advances section, add:

```rust
// ---------------------------------------------------------------------------
// Todo schedules (recurring tasks)
// ---------------------------------------------------------------------------

/// A schedule joined with its contact (name + type), for the management table.
pub struct TodoScheduleRow {
    pub schedule: TodoSchedule,
    pub contact_name: Option<String>,
    pub contact_type: Option<ContactType>,
}

/// The eight schedule columns, in the order the row mappers below expect.
const SCHED_COLS: &str =
    "s.id, s.contact_id, s.task, s.freq_kind, s.freq_value, s.start_date, s.last_generated, s.created_at";

/// Map the first eight columns (in `SCHED_COLS` order) into a `TodoSchedule`.
/// A corrupt cadence falls back to `EveryNDays(1)` (we only ever write valid
/// rows, so this is defensive — it keeps the mapper infallible).
fn row_to_schedule(row: &Row) -> rusqlite::Result<TodoSchedule> {
    let kind: String = row.get(3)?;
    let value: i64 = row.get(4)?;
    let start: String = row.get(5)?;
    let last: Option<String> = row.get(6)?;
    let created: String = row.get(7)?;
    Ok(TodoSchedule {
        id: row.get(0)?,
        contact_id: row.get(1)?,
        task: row.get(2)?,
        recurrence: Recurrence::from_db(&kind, value).unwrap_or(Recurrence::EveryNDays(1)),
        start_date: NaiveDate::parse_from_str(&start, "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive()),
        last_generated: last.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
        created_at: parse_dt(&created),
    })
}

/// Map a row of `SCHED_COLS` + (c.name, c.nickname, c.contact_type) into a row.
fn row_to_schedule_row(row: &Row) -> rusqlite::Result<TodoScheduleRow> {
    let schedule = row_to_schedule(row)?;
    let name: Option<String> = row.get(8)?;
    let nickname: Option<String> = row.get(9)?;
    let ctype: Option<String> = row.get(10)?;
    let contact_name = name.map(|n| match nickname {
        Some(nk) if !nk.is_empty() => format!("{n} ({nk})"),
        _ => n,
    });
    Ok(TodoScheduleRow {
        schedule,
        contact_name,
        contact_type: ctype.map(|s| ContactType::from_db(&s)),
    })
}

/// Validate the shared fields of an add/update. `task` is trimmed; the cadence
/// values must be in range.
fn validate_schedule(task: &str, recurrence: Recurrence) -> Result<()> {
    if task.trim().is_empty() {
        return Err(AppError::validation("กรุณากรอกสิ่งที่ต้องทำ"));
    }
    match recurrence {
        Recurrence::EveryNDays(n) if n < 1 => {
            Err(AppError::validation("จำนวนวันต้องมากกว่า 0"))
        }
        Recurrence::MonthlyDay(d) if !(1..=31).contains(&d) => {
            Err(AppError::validation("วันที่ของเดือนต้องอยู่ระหว่าง 1–31"))
        }
        _ => Ok(()),
    }
}

/// Add a schedule; returns the new id. `task` is trimmed and must be non-empty.
pub fn add_todo_schedule(
    conn: &Connection,
    contact_id: Option<i64>,
    task: &str,
    recurrence: Recurrence,
    start_date: NaiveDate,
) -> Result<i64> {
    validate_schedule(task, recurrence)?;
    conn.execute(
        "INSERT INTO todo_schedules
            (contact_id, task, freq_kind, freq_value, start_date, last_generated, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)",
        params![
            contact_id,
            task.trim(),
            recurrence.kind_str(),
            recurrence.value(),
            start_date.format("%Y-%m-%d").to_string(),
            Local::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update a schedule's contact, task, cadence, and start date (not
/// `last_generated` / `created_at`).
pub fn update_todo_schedule(conn: &Connection, s: &TodoSchedule) -> Result<()> {
    validate_schedule(&s.task, s.recurrence)?;
    conn.execute(
        "UPDATE todo_schedules
            SET contact_id = ?1, task = ?2, freq_kind = ?3, freq_value = ?4, start_date = ?5
          WHERE id = ?6",
        params![
            s.contact_id,
            s.task.trim(),
            s.recurrence.kind_str(),
            s.recurrence.value(),
            s.start_date.format("%Y-%m-%d").to_string(),
            s.id,
        ],
    )?;
    Ok(())
}

/// Delete a schedule (does not touch any todos it already created).
pub fn delete_todo_schedule(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM todo_schedules WHERE id = ?1", [id])?;
    Ok(())
}

/// All schedules joined with their contact, newest first.
pub fn list_todo_schedules(conn: &Connection) -> Result<Vec<TodoScheduleRow>> {
    let sql = format!(
        "SELECT {SCHED_COLS}, c.name, c.nickname, c.contact_type
         FROM todo_schedules s
         LEFT JOIN contacts c ON c.id = s.contact_id
         ORDER BY s.created_at DESC, s.id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_schedule_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
```

- [ ] **Step 3: Add CRUD tests**

In the `#[cfg(test)] mod tests` block of `src/db/queries.rs` (after the existing
todo tests, e.g. after `complete_todo_twice_logs_two_activities`), add:

```rust
    #[test]
    fn schedule_add_list_update_delete() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("เอ")).unwrap();
        let id = add_todo_schedule(
            &conn,
            Some(cid),
            "  โทรติดตาม  ",
            Recurrence::EveryNDays(7),
            d("2026-06-01"),
        )
        .unwrap();

        // Blank task is rejected.
        assert!(add_todo_schedule(&conn, None, "  ", Recurrence::EveryNDays(7), d("2026-06-01")).is_err());

        let rows = list_todo_schedules(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.schedule.task, "โทรติดตาม"); // trimmed
        assert_eq!(r.schedule.recurrence, Recurrence::EveryNDays(7));
        assert_eq!(r.schedule.start_date, d("2026-06-01"));
        assert_eq!(r.schedule.last_generated, None);
        assert_eq!(r.contact_name.as_deref(), Some("เอ"));

        // Update cadence + task + start.
        let mut s = r.schedule.clone();
        s.task = "โทรติดตามรายเดือน".into();
        s.recurrence = Recurrence::MonthlyDay(1);
        s.start_date = d("2026-07-01");
        update_todo_schedule(&conn, &s).unwrap();
        let rows = list_todo_schedules(&conn).unwrap();
        assert_eq!(rows[0].schedule.task, "โทรติดตามรายเดือน");
        assert_eq!(rows[0].schedule.recurrence, Recurrence::MonthlyDay(1));
        assert_eq!(rows[0].schedule.start_date, d("2026-07-01"));

        // Delete.
        delete_todo_schedule(&conn, id).unwrap();
        assert!(list_todo_schedules(&conn).unwrap().is_empty());
    }

    #[test]
    fn schedule_contact_set_null_on_delete() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("บี")).unwrap();
        add_todo_schedule(&conn, Some(cid), "งาน", Recurrence::EveryNDays(3), d("2026-06-01")).unwrap();
        delete_contact(&conn, cid).unwrap();
        let rows = list_todo_schedules(&conn).unwrap();
        assert_eq!(rows.len(), 1, "schedule survives contact deletion");
        assert_eq!(rows[0].schedule.contact_id, None);
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib schedule_`
Expected: PASS — `schedule_add_list_update_delete` and `schedule_contact_set_null_on_delete`.

- [ ] **Step 5: Commit**

```bash
git add src/db/queries.rs
git commit -m "Add todo_schedules CRUD queries

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: `generate_due_todos` + migration test

**Files:**
- Modify: `src/db/queries.rs` (add `generate_due_todos` after `list_todo_schedules`, and tests).

- [ ] **Step 1: Add the generation function**

In `src/db/queries.rs`, directly after `list_todo_schedules`, add:

```rust
/// Materialize any due cycles into `todos`. For each schedule whose latest
/// occurrence on or before `today` is newer than its `last_generated`, insert
/// one todo (due on that occurrence) and advance `last_generated` — both in one
/// transaction. Missed cycles collapse into a single todo. Returns how many
/// todos were created.
pub fn generate_due_todos(conn: &Connection, today: NaiveDate) -> Result<usize> {
    // Collect first so the prepared statement's borrow is released before we
    // start the per-schedule transactions below.
    let schedules: Vec<TodoSchedule> = {
        let sql = format!("SELECT {SCHED_COLS} FROM todo_schedules s");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], row_to_schedule)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let mut created = 0usize;
    for s in &schedules {
        let Some(occ) = s.recurrence.latest_occurrence_on_or_before(s.start_date, today) else {
            continue;
        };
        let already = s.last_generated.is_some_and(|lg| occ <= lg);
        if already {
            continue;
        }
        let occ_str = occ.format("%Y-%m-%d").to_string();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO todos (contact_id, task, due_date, done, created_at)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![s.contact_id, s.task, occ_str, Local::now().to_rfc3339()],
        )?;
        tx.execute(
            "UPDATE todo_schedules SET last_generated = ?1 WHERE id = ?2",
            params![occ_str, s.id],
        )?;
        tx.commit()?;
        created += 1;
    }
    Ok(created)
}
```

- [ ] **Step 2: Add the generation + migration tests**

In the `#[cfg(test)] mod tests` block, after the schedule CRUD tests, add:

```rust
    #[test]
    fn migration_creates_todo_schedules_table() {
        let conn = mem();
        // An empty list (rather than an error) proves the table exists.
        assert!(list_todo_schedules(&conn).unwrap().is_empty());
    }

    #[test]
    fn generate_creates_one_todo_when_due() {
        let conn = mem();
        add_todo_schedule(&conn, None, "งานรายสัปดาห์", Recurrence::EveryNDays(7), d("2026-06-01")).unwrap();
        // Day 8 → occurrence 2026-06-08 is due.
        assert_eq!(generate_due_todos(&conn, d("2026-06-08")).unwrap(), 1);
        let todos = list_todos(&conn, "").unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].todo.task, "งานรายสัปดาห์");
        assert_eq!(todos[0].todo.due_date, Some(d("2026-06-08")));
        assert!(!todos[0].todo.done);
    }

    #[test]
    fn generate_is_idempotent_same_day() {
        let conn = mem();
        add_todo_schedule(&conn, None, "x", Recurrence::EveryNDays(7), d("2026-06-01")).unwrap();
        assert_eq!(generate_due_todos(&conn, d("2026-06-08")).unwrap(), 1);
        // Running again the same day creates nothing (last_generated guard).
        assert_eq!(generate_due_todos(&conn, d("2026-06-08")).unwrap(), 0);
        assert_eq!(list_todos(&conn, "").unwrap().len(), 1);
    }

    #[test]
    fn generate_collapses_missed_cycles_to_one() {
        let conn = mem();
        add_todo_schedule(&conn, None, "x", Recurrence::EveryNDays(7), d("2026-06-01")).unwrap();
        // Three cycles passed (8th, 15th, 22nd) but only one todo is created,
        // due on the most recent occurrence (22nd).
        assert_eq!(generate_due_todos(&conn, d("2026-06-23")).unwrap(), 1);
        let todos = list_todos(&conn, "").unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].todo.due_date, Some(d("2026-06-22")));
    }

    #[test]
    fn generate_skips_future_start() {
        let conn = mem();
        add_todo_schedule(&conn, None, "x", Recurrence::EveryNDays(7), d("2026-07-01")).unwrap();
        assert_eq!(generate_due_todos(&conn, d("2026-06-23")).unwrap(), 0);
        assert!(list_todos(&conn, "").unwrap().is_empty());
    }

    #[test]
    fn generate_creates_next_todo_on_later_cycle() {
        let conn = mem();
        add_todo_schedule(&conn, None, "x", Recurrence::EveryNDays(7), d("2026-06-01")).unwrap();
        assert_eq!(generate_due_todos(&conn, d("2026-06-08")).unwrap(), 1);
        // A later run after the next cycle creates a second todo.
        assert_eq!(generate_due_todos(&conn, d("2026-06-15")).unwrap(), 1);
        assert_eq!(list_todos(&conn, "").unwrap().len(), 2);
    }
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib generate_ ; cargo test --lib migration_creates_todo_schedules`
Expected: PASS — all five `generate_*` tests and the migration test.

- [ ] **Step 4: Commit**

```bash
git add src/db/queries.rs
git commit -m "Add generate_due_todos with collapse + idempotency guard

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: `DbConnection` passthroughs

**Files:**
- Modify: `src/db/mod.rs`

- [ ] **Step 1: Add imports**

In `src/db/mod.rs`, after `use crate::models::todo::Todo;` (line ~24) add:

```rust
use crate::models::todo_schedule::{Recurrence, TodoSchedule};
```

And extend the `queries::{…}` import (line ~25) to include `TodoScheduleRow`:

```rust
use queries::{
    AboRow, ActivityKindRow, ActivityLogRow, AdvanceRow, CustomerRow, ProspectRow, TodoRow,
    TodoScheduleRow,
};
```

- [ ] **Step 2: Add the passthrough methods**

In `src/db/mod.rs`, inside `impl DbConnection`, after the `// --- todos ---`
block (after `count_due_soon_todos`, around line 182) add:

```rust
    // --- todo schedules (recurring tasks) ---------------------------------

    pub fn add_todo_schedule(
        &self,
        contact_id: Option<i64>,
        task: &str,
        recurrence: Recurrence,
        start_date: NaiveDate,
    ) -> Result<i64> {
        queries::add_todo_schedule(&self.conn, contact_id, task, recurrence, start_date)
    }
    pub fn update_todo_schedule(&self, s: &TodoSchedule) -> Result<()> {
        queries::update_todo_schedule(&self.conn, s)
    }
    pub fn delete_todo_schedule(&self, id: i64) -> Result<()> {
        queries::delete_todo_schedule(&self.conn, id)
    }
    pub fn list_todo_schedules(&self) -> Result<Vec<TodoScheduleRow>> {
        queries::list_todo_schedules(&self.conn)
    }
    pub fn generate_due_todos(&self, today: NaiveDate) -> Result<usize> {
        queries::generate_due_todos(&self.conn, today)
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: builds with no errors (a `dead_code` warning on the new methods is
acceptable until Task 7/8 call them).

- [ ] **Step 4: Commit**

```bash
git add src/db/mod.rs
git commit -m "Expose todo_schedule operations on DbConnection

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: Confirm-delete variant for schedules

**Files:**
- Modify: `src/ui/confirm.rs`

- [ ] **Step 1: Add the enum variant**

In `src/ui/confirm.rs`, add a variant to `PendingDelete` (after `Activity`):

```rust
    Activity { id: i64, label: String },
    TodoSchedule { id: i64, name: String },
}
```

- [ ] **Step 2: Add the name/detail arm**

In the `match &pending` that builds `(name, detail)`, after the `Activity` arm,
add:

```rust
        PendingDelete::TodoSchedule { name, .. } => (
            name.clone(),
            "ตารางงานประจำนี้จะถูกลบถาวร (งานที่สร้างไปแล้วยังอยู่)".to_string(),
        ),
```

- [ ] **Step 3: Add the delete dispatch arm**

In the `match &pending` inside `if confirm { let result = … }`, after the
`Activity` arm, add:

```rust
            PendingDelete::TodoSchedule { id, .. } => app.db.delete_todo_schedule(*id),
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: builds (the variant is constructed in Task 8; a `dead_code`-style
warning is acceptable until then, though enum variants typically warn only if
never constructed — this resolves in Task 8).

- [ ] **Step 5: Commit**

```bash
git add src/ui/confirm.rs
git commit -m "Add TodoSchedule case to the confirm-delete modal

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Wire generation + state into `AppState`

**Files:**
- Modify: `src/app.rs` (struct fields, `new()` init, sidebar entry, dispatch arm, `update` generation call).

- [ ] **Step 1: Add the state fields**

In `src/app.rs`, in the `pub struct AppState { … }`, after the meeting fields
(after `pub meeting_show_past: bool,`, line ~110) add:

```rust
    /// Recurring-task schedule add/edit form state.
    pub todo_schedule_form: crate::ui::todo_schedules::TodoScheduleForm,
    /// Calendar date of the last auto-generation run. Initialised to a sentinel
    /// in the past so the first `update` frame generates due todos (covering app
    /// start); thereafter it re-runs whenever the date changes while open.
    pub last_gen_check: chrono::NaiveDate,
```

- [ ] **Step 2: Initialise the fields in `new()`**

In `AppState::new`, in the struct literal returned in `Ok(AppState { … })`,
after `meeting_show_past: false,` (line ~169) add:

```rust
            todo_schedule_form: crate::ui::todo_schedules::TodoScheduleForm::default(),
            last_gen_check: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
```

- [ ] **Step 3: Add the sidebar menu entry**

In `AppState::sidebar`, in the `items` array, add an entry right after the
`(View::Todos, "📅  สิ่งที่ต้องทำ"),` line:

```rust
            (View::Todos, "📅  สิ่งที่ต้องทำ"),
            (View::TodoSchedules, "🔁  ตารางงานประจำ"),
```

> Glyph note: 🔁 (U+1F501) is in the same emoji block as the menu's existing
> 🎟/💵/🌳 icons, which render via egui's bundled emoji fallback. Task 9 verifies
> it is not tofu; if it is, replace the leading glyph with `📅` is taken, so use
> a plain text label `"⟳  ตารางงานประจำ"` only if `🔁` fails — confirm visually
> first.

- [ ] **Step 4: Add the dispatch arm**

In `eframe::App::update`, in the `match self.view { … }` of the central panel,
after `View::Todos => ui::todo::render(self, ui),` add:

```rust
            View::Todos => ui::todo::render(self, ui),
            View::TodoSchedules => ui::todo_schedules::render(self, ui),
```

- [ ] **Step 5: Add the generation call at the top of `update`**

In `eframe::App::update`, as the very first statements (before the
`egui::SidePanel::left(…)` call), add:

```rust
        // Auto-create due recurring todos on the first frame (covers app start)
        // and whenever the calendar date changes while the app stays open.
        let today = chrono::Local::now().date_naive();
        if today != self.last_gen_check {
            self.last_gen_check = today;
            let r = self.db.generate_due_todos(today);
            let _ = self.handle(r, 0);
        }
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build`
Expected: fails to compile **only** because `View::TodoSchedules`,
`ui::todo_schedules`, and `TodoScheduleForm` do not exist yet — those are created
in Task 8. If any *other* error appears, fix it. (Implementer note: Tasks 7 and 8
together form one compilable unit; commit Task 7 even though the build is red,
then complete Task 8 to make it green. Alternatively, do Step 1–5 here and Task 8
before the first `cargo build`.)

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "Wire recurring-todo generation and schedule state into AppState

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 8: The `todo_schedules` management page

**Files:**
- Create: `src/ui/todo_schedules.rs`
- Modify: `src/ui/mod.rs` (module registration + `View` variant)

- [ ] **Step 1: Register the module and View variant**

In `src/ui/mod.rs`, add the module declaration (after `pub mod todo_done;`,
keeping order):

```rust
pub mod todo;
pub mod todo_done;
pub mod todo_schedules;
```

And add the variant to `enum View` (after `Todos,`):

```rust
    Todos,
    TodoSchedules,
```

- [ ] **Step 2: Create the page**

Create `src/ui/todo_schedules.rs` with the full content below.

```rust
//! ตารางงานประจำ / Recurring task schedules. Define a template (task + optional
//! contact) plus a cadence ("ทุก N วัน" or "รายเดือน วันที่กำหนด"); the app
//! auto-creates a normal todo on the "สิ่งที่ต้องทำ" page when a cycle is due
//! (see `AppState::update` → `db.generate_due_todos`). Add/edit on the left,
//! a how-it-works note on the right, then a table with edit/delete per row.

use chrono::{Local, NaiveDate};
use egui_extras::{Column, DatePickerButton, TableBuilder};

use crate::app::AppState;
use crate::models::enums::ContactType;
use crate::models::todo_schedule::{Recurrence, TodoSchedule};
use crate::ui::confirm::PendingDelete;
use crate::ui::{filter_combo, ACCENT, ACCENT_STRONG};

/// Width of the fixed label column in each form row (mirrors `ui/todo.rs`).
const LABEL_W: f32 = 110.0;

/// One labelled form row: a fixed-width label cell, then the field widget.
fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_W, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        add(ui);
    });
}

/// Add/edit form state for the recurring-schedule page. The cadence is split
/// into a boolean mode + two value buffers so toggling between kinds keeps each
/// kind's last-typed value.
pub struct TodoScheduleForm {
    /// `Some(id)` when editing an existing schedule; `None` when adding.
    pub editing_id: Option<i64>,
    pub task: String,
    pub contact_id: Option<i64>,
    pub contact_filter: String,
    /// `false` = ทุก N วัน (EveryNDays); `true` = รายเดือน (MonthlyDay).
    pub monthly: bool,
    pub every_n_days: i64,
    pub month_day: i64,
    pub start_date: NaiveDate,
}

impl Default for TodoScheduleForm {
    fn default() -> Self {
        TodoScheduleForm {
            editing_id: None,
            task: String::new(),
            contact_id: None,
            contact_filter: String::new(),
            monthly: false,
            every_n_days: 7,
            month_day: 1,
            start_date: Local::now().date_naive(),
        }
    }
}

impl TodoScheduleForm {
    fn reset(&mut self) {
        *self = TodoScheduleForm::default();
    }

    /// Build the `Recurrence` from the current mode + value buffers.
    fn recurrence(&self) -> Recurrence {
        if self.monthly {
            Recurrence::MonthlyDay(self.month_day.clamp(1, 31) as u32)
        } else {
            Recurrence::EveryNDays(self.every_n_days.max(1) as u32)
        }
    }

    /// Populate the form from an existing schedule for editing.
    fn edit_from(s: &TodoSchedule) -> Self {
        let (monthly, every_n_days, month_day) = match s.recurrence {
            Recurrence::EveryNDays(n) => (false, n as i64, 1),
            Recurrence::MonthlyDay(d) => (true, 7, d as i64),
        };
        TodoScheduleForm {
            editing_id: Some(s.id),
            task: s.task.clone(),
            contact_id: s.contact_id,
            contact_filter: String::new(),
            monthly,
            every_n_days,
            month_day,
            start_date: s.start_date,
        }
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("ตารางงานประจำ / Recurring Tasks");
    ui.label(
        egui::RichText::new("ตั้งรอบให้ระบบสร้างงานใน \"สิ่งที่ต้องทำ\" อัตโนมัติเมื่อถึงกำหนด")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Contacts for the picker, pre-fetched so the combo closure does not borrow
    // app.db while mutating app.todo_schedule_form.
    let contacts = app.db.list_contacts().unwrap_or_default();
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();

    let mut submit = false;
    let mut cancel_edit = false;
    let editing = app.todo_schedule_form.editing_id.is_some();

    ui.columns(2, |cols| {
        let field_w = (cols[0].available_width() - LABEL_W - 40.0).max(60.0);

        // Left card: add / edit form.
        let c0 = &mut cols[0];
        egui::Frame::group(c0.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c0, |ui| {
                let f = &mut app.todo_schedule_form;
                ui.label(
                    egui::RichText::new(if editing {
                        "✏ แก้ไขตารางงาน"
                    } else {
                        "➕ เพิ่มตารางงานใหม่"
                    })
                    .color(ACCENT_STRONG)
                    .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "สิ่งที่ต้องทำ", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.task)
                            .hint_text("เช่น โทรติดตามผลลูกค้า")
                            .desired_width(field_w),
                    );
                });
                field_row(ui, "เกี่ยวกับ", |ui| {
                    filter_combo(
                        ui,
                        "schedule_contact_cb",
                        &mut f.contact_id,
                        &mut f.contact_filter,
                        Some("— ไม่ระบุ —"),
                        &contact_options,
                        field_w,
                    );
                });
                field_row(ui, "รอบ", |ui| {
                    egui::ComboBox::from_id_source("schedule_freq_cb")
                        .width(field_w)
                        .selected_text(if f.monthly { "รายเดือน (วันที่กำหนด)" } else { "ทุก N วัน" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.monthly, false, "ทุก N วัน");
                            ui.selectable_value(&mut f.monthly, true, "รายเดือน (วันที่กำหนด)");
                        });
                });
                if f.monthly {
                    field_row(ui, "วันที่ของเดือน", |ui| {
                        ui.add(egui::DragValue::new(&mut f.month_day).range(1..=31));
                        ui.weak("(วันที่ 29–31 จะปัดเป็นวันสุดท้ายของเดือนสั้น)");
                    });
                } else {
                    field_row(ui, "ทุกกี่วัน", |ui| {
                        ui.add(egui::DragValue::new(&mut f.every_n_days).range(1..=365).suffix(" วัน"));
                    });
                }
                field_row(ui, "วันเริ่ม", |ui| {
                    ui.add(DatePickerButton::new(&mut f.start_date).id_source("schedule_start_picker"));
                });

                ui.add_space(8.0);
                field_row(ui, "", |ui| {
                    if editing {
                        if ui.add(egui::Button::new("💾 บันทึก").fill(ACCENT)).clicked() {
                            submit = true;
                        }
                        if ui.button("ยกเลิก").clicked() {
                            cancel_edit = true;
                        }
                    } else if ui.add(egui::Button::new("➕ เพิ่ม").fill(ACCENT)).clicked() {
                        submit = true;
                    }
                });
            });

        // Right card: how-it-works note.
        let c1 = &mut cols[1];
        egui::Frame::group(c1.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c1, |ui| {
                ui.label(egui::RichText::new("ℹ วิธีทำงาน").color(ACCENT_STRONG).strong());
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "เมื่อถึงหรือเลยวันที่ตามรอบ ระบบจะสร้างงานใน \"สิ่งที่ต้องทำ\" \
                         ให้อัตโนมัติ (กำหนดส่ง = วันของรอบ) ตอนเปิดแอปหรือเมื่อข้ามวัน",
                    )
                    .small(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "ถ้าเลยมาหลายรอบ จะสร้างเพียงงานเดียว (รอบล่าสุด) • \
                         ลบตารางที่นี่ไม่ลบงานที่สร้างไปแล้ว",
                    )
                    .small()
                    .weak(),
                );
            });
    });

    ui.add_space(6.0);

    // --- load schedules ---
    let r = app.db.list_todo_schedules();
    let rows = app.handle(r, Vec::new());
    let today = Local::now().date_naive();

    ui.label(
        egui::RichText::new(format!("ทั้งหมด {} ตาราง", rows.len()))
            .small()
            .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ยังไม่มีตารางงานประจำ —");
        apply_form(app, submit, cancel_edit);
        return;
    }

    let mut edit_req: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::remainder().at_least(200.0)) // สิ่งที่ต้องทำ
        .column(Column::auto().at_least(160.0)) // เกี่ยวกับ
        .column(Column::auto().at_least(120.0)) // รอบ
        .column(Column::auto().at_least(110.0)) // รอบถัดไป
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            for h in ["สิ่งที่ต้องทำ", "เกี่ยวกับ", "รอบ", "รอบถัดไป", "จัดการ"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(row.schedule.task.as_str());
                    });
                    tr.col(|ui| match (&row.contact_name, row.contact_type) {
                        (Some(name), Some(ty)) => {
                            let color = match ty {
                                ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
                                ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
                                ContactType::Abo => ACCENT_STRONG,
                            };
                            ui.label(egui::RichText::new(name.as_str()).color(color));
                        }
                        _ => {
                            ui.weak("—");
                        }
                    });
                    tr.col(|ui| {
                        ui.label(row.schedule.recurrence.label_th());
                    });
                    tr.col(|ui| {
                        let next = row
                            .schedule
                            .recurrence
                            .next_occurrence_after(row.schedule.start_date, today);
                        ui.label(egui::RichText::new(next.format("%Y-%m-%d").to_string()).small());
                    });
                    tr.col(|ui| {
                        if ui.small_button("✏").on_hover_text("แก้ไข").clicked() {
                            edit_req = Some(row.schedule.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.schedule.id, row.schedule.task.clone()));
                        }
                    });
                });
            }
        });

    // --- apply deferred row actions ---
    if let Some(id) = edit_req {
        if let Some(row) = rows.iter().find(|r| r.schedule.id == id) {
            app.todo_schedule_form = TodoScheduleForm::edit_from(&row.schedule);
        }
    }
    if let Some((id, name)) = delete_req {
        app.pending_delete = Some(PendingDelete::TodoSchedule { id, name });
    }

    apply_form(app, submit, cancel_edit);
}

/// Apply the add/edit form's submit or cancel (factored out so it runs whether or
/// not the table was drawn). Contact is optional (mirrors the Todo page).
fn apply_form(app: &mut AppState, submit: bool, cancel_edit: bool) {
    if cancel_edit {
        app.todo_schedule_form.reset();
    }
    if !submit {
        return;
    }
    let f = &app.todo_schedule_form;
    let editing = f.editing_id;
    let recurrence = f.recurrence();
    let result = match editing {
        Some(id) => {
            let s = TodoSchedule {
                id,
                contact_id: f.contact_id,
                task: f.task.clone(),
                recurrence,
                start_date: f.start_date,
                // update_todo_schedule ignores these two.
                last_generated: None,
                created_at: Local::now(),
            };
            app.db.update_todo_schedule(&s)
        }
        None => app
            .db
            .add_todo_schedule(f.contact_id, &f.task, recurrence, f.start_date)
            .map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.todo_schedule_form.reset();
            app.set_status(if editing.is_some() { "บันทึกตารางงานแล้ว" } else { "เพิ่มตารางงานแล้ว" });
        }
        Err(e) => app.set_error(e),
    }
}
```

- [ ] **Step 3: Build the whole app**

Run: `cargo build`
Expected: builds with no errors (Task 7's references now resolve).

- [ ] **Step 4: Run the full test suite**

Run: `cargo test`
Expected: PASS — all existing tests plus the new model/query tests; 0 failures.

- [ ] **Step 5: Commit**

```bash
git add src/ui/todo_schedules.rs src/ui/mod.rs
git commit -m "Add the ตารางงานประจำ recurring-task management page

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 9: Visual + glyph verification

**Files:** none (verification only).

- [ ] **Step 1: Build the release-ish binary**

Run: `cargo build`
Expected: clean build, no warnings introduced by this feature.

- [ ] **Step 2: Launch and screenshot the app**

Use the project's "egui app screenshot verify" approach (per memory): run the
exe and capture its window via `MainWindowHandle`. Confirm by eye:
- The sidebar shows "🔁  ตารางงานประจำ" with a real icon (NOT a tofu □ box).
  If the 🔁 glyph is tofu, change the menu label's leading glyph in
  `src/app.rs` to a plain symbol that renders (e.g. `"⟳  ตารางงานประจำ"`) and
  rebuild. Re-verify.
- Clicking the entry opens the page: form card on the left, info card on the
  right, and (once a schedule is added) a table.
- Add a schedule "ทุก N วัน" with start date today; switch the view to
  "สิ่งที่ต้องทำ" and confirm a matching todo appears (due today, since the
  first occurrence is on the start date).
- On the schedule, click 🗑 → the shared "ยืนยันการลบ" confirm modal appears;
  confirm it deletes the schedule and that the previously generated todo
  remains on the Todo page.

- [ ] **Step 3: Final full-suite check**

Run: `cargo test`
Expected: `test result: ok. <N> passed; 0 failed`.

- [ ] **Step 4: Commit any glyph fix (only if Step 2 required one)**

```bash
git add src/app.rs
git commit -m "Use a rendering glyph for the ตารางงานประจำ menu entry

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review (completed by plan author)

**Spec coverage:**
- Recurrence kinds (every-N-days + monthly day-of-month) → Task 2 `Recurrence`.
- Missed cycles collapse to one → Task 4 `generate_collapses_missed_cycles_to_one`.
- No end condition → no end-date column; schedules persist until deleted (Task 1/3).
- Separate management page → Task 8 `todo_schedules.rs` + Task 7 menu/dispatch.
- Generated due date = latest passed occurrence → Task 4 insert uses `occ`.
- Generation on start + date change → Task 7 sentinel `last_gen_check` in `update`.
- User-chosen start date, default today → Task 8 form `start_date`.
- Monthly day 29–31 clamps to month end → Task 2 `occ_in_month` + `monthly_day_clamps_to_month_end` test.
- Delete via confirm modal → Task 6 + Task 8 wiring.
- Contact link nullable, survives contact delete → Task 1 `ON DELETE SET NULL` + Task 3 `schedule_contact_set_null_on_delete`.

**Type consistency:** `Recurrence`, `TodoSchedule`, `TodoScheduleRow`,
`TodoScheduleForm`, and method names (`latest_occurrence_on_or_before`,
`next_occurrence_after`, `label_th`, `kind_str`, `value`, `from_db`,
`add_todo_schedule`, `update_todo_schedule`, `delete_todo_schedule`,
`list_todo_schedules`, `generate_due_todos`) are used identically across tasks.

**Placeholder scan:** no TBD/TODO/"similar to"; every code step is complete.
