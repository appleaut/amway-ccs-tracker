# Meetings & Attendance Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "งานประชุม" (Meetings) page with a contacts × meetings matrix that records each contact's RSVP status, entry-fee payment, and post-event actual attendance.

**Architecture:** Two new SQLite tables (`meetings`, `meeting_attendees`) behind the existing `queries` → `DbConnection` layering; a new `View::Meetings` rendering a dynamic-column `egui_extras::TableBuilder` matrix (rows = contacts, one column per shown meeting), with a per-cell popup. Cell writes go through one transactional `upsert_attendee` that logs an activity on the "attending" and "attended" milestones, mirroring `collect_advance`.

**Tech Stack:** Rust, eframe/egui 0.28, egui_extras 0.28 (`TableBuilder`, `DatePickerButton`), rusqlite 0.31, chrono.

**Spec:** `docs/superpowers/specs/2026-06-05-meetings-attendance-design.md`

**Repo conventions (MUST follow):**
- **NEVER run `cargo fmt`** — this repo is hand-formatted. Verify only with `cargo test` / `cargo build`.
- Every commit message ends with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Match surrounding code style (4-space indent, doc-comments `//!`/`///`, Thai UI strings).
- Transient `dead_code` warnings are expected between Tasks 2–6 (new code is wired to `main` only in Task 7); they must be gone after Task 7. Warnings do not fail `cargo test`.

---

## File Structure

- **Modify** `src/db/schema.rs` — migration v10: create both tables + seed two activity kinds; bump `CURRENT_VERSION` to 10.
- **Modify** `src/models/enums.rs` — add `AttendeeStatus` enum (+ test).
- **Create** `src/models/meeting.rs` — `Meeting` and `MeetingAttendee` structs.
- **Modify** `src/models/mod.rs` — register `meeting`.
- **Modify** `src/db/queries.rs` — `MEETING_RSVP_KIND` / `MEETING_ATTENDED_KIND` constants; `row_to_meeting`, `validate_meeting`; `add_meeting` / `update_meeting` / `delete_meeting` / `list_meetings`; `attendee_map` / `upsert_attendee` / `remove_attendee`; tests.
- **Modify** `src/db/mod.rs` — `DbConnection` passthroughs.
- **Create** `src/ui/meeting_form.rs` — `MeetingForm` state + add/edit modal.
- **Create** `src/ui/meetings.rs` — the matrix page, `MeetingWhoFilter`, `status_color`, cell popup.
- **Modify** `src/ui/confirm.rs` — `PendingDelete::Meeting` variant.
- **Modify** `src/ui/mod.rs` — register `meetings` + `meeting_form`; add `View::Meetings`.
- **Modify** `src/app.rs` — `AppState` fields; sidebar item; dispatch; modal render.

---

## Task 1: Migration v10 (tables + seeded kinds)

**Files:**
- Modify: `src/db/queries.rs` (add two constants after line 317)
- Modify: `src/db/schema.rs:11` (`CURRENT_VERSION`) and the migration body (after the v9 block, before line 216)
- Test: `src/db/queries.rs` (tests module)

- [ ] **Step 1: Add the two kind constants in `queries.rs`**

After the `ADVANCE_COLLECTED_KIND` constant (line 317), add:

```rust
/// Activity kind logged when a contact is set to "จะเข้าร่วม" for a meeting.
/// Seeded by the v10 migration; stored as text on each activity row.
pub const MEETING_RSVP_KIND: &str = "ตอบรับเข้างานประชุม";

/// Activity kind logged when a contact is recorded as "มาจริง" for a meeting.
/// Seeded by the v10 migration; stored as text on each activity row.
pub const MEETING_ATTENDED_KIND: &str = "เข้าร่วมงานประชุม";
```

- [ ] **Step 2: Bump the schema version**

In `src/db/schema.rs`, change line 11:

```rust
const CURRENT_VERSION: i64 = 10;
```

- [ ] **Step 3: Add the v10 migration block**

In `src/db/schema.rs`, immediately after the `if version < 9 { ... }` block (just before the `if version != CURRENT_VERSION {` line), insert:

```rust
    if version < 10 {
        // Meetings/events and per-contact attendance. Both FKs cascade: an
        // attendance cell is meaningless without its meeting and contact, so it
        // is removed when either is deleted (unlike advances/todos).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meetings (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT    NOT NULL,
                start_date  TEXT    NOT NULL,
                end_date    TEXT    NOT NULL,
                description TEXT    NOT NULL DEFAULT '',
                fee         INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT    NOT NULL
            );
            CREATE TABLE IF NOT EXISTS meeting_attendees (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id INTEGER NOT NULL REFERENCES meetings(id)  ON DELETE CASCADE,
                contact_id INTEGER NOT NULL REFERENCES contacts(id)  ON DELETE CASCADE,
                status     TEXT    NOT NULL,
                paid       INTEGER NOT NULL DEFAULT 0,
                attended   INTEGER,
                created_at TEXT    NOT NULL,
                updated_at TEXT    NOT NULL,
                UNIQUE(meeting_id, contact_id)
            );
            CREATE INDEX IF NOT EXISTS idx_attendees_meeting ON meeting_attendees(meeting_id);
            CREATE INDEX IF NOT EXISTS idx_attendees_contact ON meeting_attendees(contact_id);",
        )?;
        // Seed the two activity kinds logged from the matrix.
        conn.execute(
            "INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1)",
            params![crate::db::queries::MEETING_RSVP_KIND],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1)",
            params![crate::db::queries::MEETING_ATTENDED_KIND],
        )?;
    }
```

- [ ] **Step 4: Write the failing tests**

In the `tests` module of `src/db/queries.rs` (after `migration_seeds_advance_collected_kind`, ~line 1573), add:

```rust
    #[test]
    fn migration_seeds_meeting_kinds() {
        let conn = mem();
        let kinds = list_activity_kinds(&conn).unwrap();
        assert!(kinds.iter().any(|k| k.name == MEETING_RSVP_KIND));
        assert!(kinds.iter().any(|k| k.name == MEETING_ATTENDED_KIND));
    }

    #[test]
    fn migration_creates_meeting_tables() {
        let conn = mem();
        let m: i64 = conn.query_row("SELECT COUNT(*) FROM meetings", [], |r| r.get(0)).unwrap();
        let a: i64 =
            conn.query_row("SELECT COUNT(*) FROM meeting_attendees", [], |r| r.get(0)).unwrap();
        assert_eq!(m, 0);
        assert_eq!(a, 0);
    }
```

- [ ] **Step 5: Run the tests**

Run: `cargo test migration_ -- --nocapture`
Expected: `migration_seeds_meeting_kinds` and `migration_creates_meeting_tables` PASS (plus the existing migration tests).

- [ ] **Step 6: Commit**

```bash
git add src/db/schema.rs src/db/queries.rs
git commit -m "$(cat <<'EOF'
Add meetings schema (migration v10) + seeded activity kinds

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: AttendeeStatus enum + Meeting/MeetingAttendee models

**Files:**
- Modify: `src/models/enums.rs` (add enum + a test module)
- Create: `src/models/meeting.rs`
- Modify: `src/models/mod.rs`

- [ ] **Step 1: Add `AttendeeStatus` to `enums.rs`**

Append to `src/models/enums.rs` (after the `SponsorStep` impl, before any test module):

```rust
/// A contact's RSVP for a meeting. Stored as a stable string; `from_db` falls
/// back to `Undecided`. Actual post-event attendance is a separate nullable
/// boolean on the attendee row, not part of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttendeeStatus {
    Attending,
    Undecided,
    NotAttending,
}

impl AttendeeStatus {
    pub const ALL: [AttendeeStatus; 3] = [
        AttendeeStatus::Attending,
        AttendeeStatus::Undecided,
        AttendeeStatus::NotAttending,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AttendeeStatus::Attending => "Attending",
            AttendeeStatus::Undecided => "Undecided",
            AttendeeStatus::NotAttending => "NotAttending",
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            AttendeeStatus::Attending => "จะเข้าร่วม",
            AttendeeStatus::Undecided => "รอตัดสินใจ",
            AttendeeStatus::NotAttending => "ไม่เข้า",
        }
    }

    pub fn from_db(s: &str) -> AttendeeStatus {
        match s {
            "Attending" => AttendeeStatus::Attending,
            "NotAttending" => AttendeeStatus::NotAttending,
            _ => AttendeeStatus::Undecided,
        }
    }
}
```

- [ ] **Step 2: Add a test for the enum round-trip**

At the end of `src/models/enums.rs`, add a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attendee_status_round_trips_and_defaults() {
        for s in AttendeeStatus::ALL {
            assert_eq!(AttendeeStatus::from_db(s.as_str()), s);
        }
        assert_eq!(AttendeeStatus::from_db("???"), AttendeeStatus::Undecided);
    }
}
```

- [ ] **Step 3: Create the model file `src/models/meeting.rs`**

```rust
//! A meeting/event and a contact's attendance of it.
//!
//! [`Meeting`] is the event (name, dates, description, entry fee).
//! [`MeetingAttendee`] is one cell of the attendance matrix: a contact's RSVP
//! status for a meeting, whether they paid the entry fee, and their actual
//! post-event attendance (`None` = not recorded yet).

use chrono::{DateTime, Local, NaiveDate};

use crate::models::enums::AttendeeStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
    pub id: i64,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub description: String,
    /// Entry fee in baht (0 = free).
    pub fee: i64,
    pub created_at: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingAttendee {
    pub meeting_id: i64,
    pub contact_id: i64,
    pub status: AttendeeStatus,
    pub paid: bool,
    /// `None` = not recorded, `Some(true)` = came, `Some(false)` = no-show.
    pub attended: Option<bool>,
}
```

- [ ] **Step 4: Register the module in `src/models/mod.rs`**

Add the doc-comment line in the module list and the `pub mod`:

```rust
//! * [`meeting`] — a meeting/event plus per-contact attendance (RSVP, fee paid, actual attendance).
```

```rust
pub mod meeting;
```

(Place `pub mod meeting;` in alphabetical position — between `pub mod followup;` and `pub mod todo;`. The doc-comment bullet goes after the `advance` bullet.)

- [ ] **Step 5: Run the tests**

Run: `cargo test attendee_status_round_trips_and_defaults`
Expected: PASS. (A transient `dead_code` warning for `Meeting`/`MeetingAttendee` is expected until Task 3.)

- [ ] **Step 6: Commit**

```bash
git add src/models/enums.rs src/models/meeting.rs src/models/mod.rs
git commit -m "$(cat <<'EOF'
Add AttendeeStatus enum and Meeting/MeetingAttendee models

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Meeting CRUD queries

**Files:**
- Modify: `src/db/queries.rs` (imports; a new "Meetings" section after the Advances section, ~line 1153; tests)

- [ ] **Step 1: Extend the imports**

In `src/db/queries.rs`, change the enums import (line 18) to add `AttendeeStatus`, and add a meeting-models import after the advance import (line 16):

```rust
use crate::models::enums::{AttendeeStatus, ContactType, Gender, NetworkCategory, Rank, SponsorStep};
```

```rust
use crate::models::meeting::{Meeting, MeetingAttendee};
```

- [ ] **Step 2: Write the failing tests**

In the `tests` module of `src/db/queries.rs` (after the advance tests, before the closing `}` at line 1985), add:

```rust
    #[test]
    fn meeting_crud_round_trips() {
        let conn = mem();
        let id = add_meeting(&conn, "  สัมมนา CCS  ", d("2026-07-01"), d("2026-07-03"), "  ที่โรงแรม  ", 1500)
            .unwrap();
        let m = list_meetings(&conn, true).unwrap().into_iter().find(|m| m.id == id).unwrap();
        assert_eq!(m.name, "สัมมนา CCS"); // trimmed
        assert_eq!(m.start_date, d("2026-07-01"));
        assert_eq!(m.end_date, d("2026-07-03"));
        assert_eq!(m.description, "ที่โรงแรม"); // trimmed
        assert_eq!(m.fee, 1500);

        let mut m2 = m.clone();
        m2.name = "สัมมนาใหญ่".into();
        m2.fee = 2000;
        m2.end_date = d("2026-07-04");
        update_meeting(&conn, &m2).unwrap();
        let m3 = list_meetings(&conn, true).unwrap().into_iter().find(|x| x.id == id).unwrap();
        assert_eq!(m3.name, "สัมมนาใหญ่");
        assert_eq!(m3.fee, 2000);
        assert_eq!(m3.end_date, d("2026-07-04"));

        delete_meeting(&conn, id).unwrap();
        assert!(list_meetings(&conn, true).unwrap().is_empty());
    }

    #[test]
    fn meeting_validation_rejects_bad_input() {
        let conn = mem();
        assert!(add_meeting(&conn, "   ", d("2026-07-01"), d("2026-07-01"), "", 0).is_err());
        assert!(add_meeting(&conn, "x", d("2026-07-05"), d("2026-07-01"), "", 0).is_err());
        assert!(add_meeting(&conn, "x", d("2026-07-01"), d("2026-07-01"), "", -1).is_err());
        assert!(add_meeting(&conn, "x", d("2026-07-01"), d("2026-07-01"), "", 0).is_ok());
    }

    #[test]
    fn list_meetings_filters_past_by_end_date() {
        let conn = mem();
        let today = Local::now().date_naive();
        add_meeting(&conn, "เก่า", today - chrono::Duration::days(3), today - chrono::Duration::days(2), "", 0)
            .unwrap();
        add_meeting(&conn, "จบวันนี้", today - chrono::Duration::days(1), today, "", 0).unwrap();
        add_meeting(&conn, "อนาคต", today + chrono::Duration::days(5), today + chrono::Duration::days(5), "", 0)
            .unwrap();

        let upcoming: Vec<String> =
            list_meetings(&conn, false).unwrap().into_iter().map(|m| m.name).collect();
        assert_eq!(upcoming, vec!["จบวันนี้", "อนาคต"]); // past excluded; ordered by start_date
        assert_eq!(list_meetings(&conn, true).unwrap().len(), 3);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test meeting_ list_meetings_ -- --nocapture`
Expected: FAIL — `cannot find function add_meeting`/`list_meetings`/etc.

- [ ] **Step 4: Implement the meeting CRUD**

In `src/db/queries.rs`, after the Advances section (after `collect_advance`, line 1153) add:

```rust
// ---------------------------------------------------------------------------
// Meetings
// ---------------------------------------------------------------------------

fn row_to_meeting(row: &Row) -> rusqlite::Result<Meeting> {
    let start: String = row.get(2)?;
    let end: String = row.get(3)?;
    let created: String = row.get(6)?;
    Ok(Meeting {
        id: row.get(0)?,
        name: row.get(1)?,
        start_date: NaiveDate::parse_from_str(&start, "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive()),
        end_date: NaiveDate::parse_from_str(&end, "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive()),
        description: row.get(4)?,
        fee: row.get(5)?,
        created_at: parse_dt(&created),
    })
}

/// Validate a meeting's fields: name non-empty, end not before start, fee >= 0.
fn validate_meeting(name: &str, start: NaiveDate, end: NaiveDate, fee: i64) -> Result<()> {
    if name.trim().is_empty() {
        return Err(AppError::validation("กรุณากรอกชื่องาน"));
    }
    if end < start {
        return Err(AppError::validation("วันที่สิ้นสุดต้องไม่ก่อนวันที่เริ่ม"));
    }
    if fee < 0 {
        return Err(AppError::validation("ค่าเข้างานต้องไม่ติดลบ"));
    }
    Ok(())
}

/// Add a meeting; returns the new id. `name`/`description` are trimmed.
pub fn add_meeting(
    conn: &Connection,
    name: &str,
    start: NaiveDate,
    end: NaiveDate,
    description: &str,
    fee: i64,
) -> Result<i64> {
    validate_meeting(name, start, end, fee)?;
    conn.execute(
        "INSERT INTO meetings (name, start_date, end_date, description, fee, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            name.trim(),
            start.format("%Y-%m-%d").to_string(),
            end.format("%Y-%m-%d").to_string(),
            description.trim(),
            fee,
            Local::now().to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update a meeting's fields (not `created_at`).
pub fn update_meeting(conn: &Connection, m: &Meeting) -> Result<()> {
    validate_meeting(&m.name, m.start_date, m.end_date, m.fee)?;
    conn.execute(
        "UPDATE meetings SET name = ?1, start_date = ?2, end_date = ?3, description = ?4, fee = ?5
         WHERE id = ?6",
        params![
            m.name.trim(),
            m.start_date.format("%Y-%m-%d").to_string(),
            m.end_date.format("%Y-%m-%d").to_string(),
            m.description.trim(),
            m.fee,
            m.id,
        ],
    )?;
    Ok(())
}

/// Delete a meeting; its attendee rows cascade.
pub fn delete_meeting(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM meetings WHERE id = ?1", [id])?;
    Ok(())
}

/// All meetings ordered by start date. When `include_past` is false, only
/// meetings whose `end_date` is today or later are returned.
pub fn list_meetings(conn: &Connection, include_past: bool) -> Result<Vec<Meeting>> {
    const COLS: &str = "id, name, start_date, end_date, description, fee, created_at";
    if include_past {
        let mut stmt =
            conn.prepare(&format!("SELECT {COLS} FROM meetings ORDER BY start_date ASC, id ASC"))?;
        let rows = stmt.query_map([], row_to_meeting)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    } else {
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM meetings WHERE end_date >= ?1 ORDER BY start_date ASC, id ASC"
        ))?;
        let rows = stmt.query_map([today], row_to_meeting)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test meeting_ list_meetings_ -- --nocapture`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/db/queries.rs
git commit -m "$(cat <<'EOF'
Add meeting CRUD queries (add/update/delete/list with past filter)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Attendee queries (matrix fetch + upsert with logging + remove)

**Files:**
- Modify: `src/db/queries.rs` (Meetings section; tests)

- [ ] **Step 1: Write the failing tests**

In the `tests` module of `src/db/queries.rs`, add:

```rust
    #[test]
    fn upsert_attendee_inserts_then_updates_single_row() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ก")).unwrap();
        let mid = add_meeting(&conn, "สัมมนา", d("2026-07-01"), d("2026-07-01"), "", 500).unwrap();

        upsert_attendee(&conn, mid, cid, AttendeeStatus::Undecided, false, None).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, true, Some(true)).unwrap();

        let map = attendee_map(&conn).unwrap();
        let a = map.get(&(mid, cid)).unwrap();
        assert_eq!(a.status, AttendeeStatus::Attending);
        assert!(a.paid);
        assert_eq!(a.attended, Some(true));

        let n: i64 =
            conn.query_row("SELECT COUNT(*) FROM meeting_attendees", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "upsert must not duplicate the (meeting, contact) row");
    }

    #[test]
    fn upsert_attendee_attended_null_round_trips() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ข")).unwrap();
        let mid = add_meeting(&conn, "งาน", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Undecided, false, None).unwrap();
        assert_eq!(attendee_map(&conn).unwrap().get(&(mid, cid)).unwrap().attended, None);
    }

    #[test]
    fn upsert_attendee_logs_attending_once() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ธ")).unwrap();
        let mid = add_meeting(&conn, "งานA", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();

        upsert_attendee(&conn, mid, cid, AttendeeStatus::Undecided, false, None).unwrap();
        assert_eq!(list_activities(&conn, cid).unwrap().len(), 0);

        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap();
        let acts = list_activities(&conn, cid).unwrap();
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, MEETING_RSVP_KIND);
        assert_eq!(acts[0].note, "ตอบรับเข้าร่วม: งานA");

        // Staying attending (e.g. ticking paid) logs nothing more.
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, true, None).unwrap();
        assert_eq!(list_activities(&conn, cid).unwrap().len(), 1);
    }

    #[test]
    fn upsert_attendee_logs_attended_once() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("น")).unwrap();
        let mid = add_meeting(&conn, "งานB", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();

        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap(); // 1 rsvp
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, Some(true)).unwrap(); // +1 attended

        let acts = list_activities(&conn, cid).unwrap();
        assert_eq!(acts.len(), 2);
        assert!(acts.iter().any(|a| a.kind == MEETING_ATTENDED_KIND && a.note == "เข้าร่วมงานจริง: งานB"));

        // Re-recording came, or recording no-show/clear, logs nothing more.
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, Some(true)).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, Some(false)).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap();
        assert_eq!(list_activities(&conn, cid).unwrap().len(), 2);
    }

    #[test]
    fn upsert_attendee_can_create_undecided_walk_in() {
        // The matrix records a walk-in by upserting status Undecided + attended.
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("วอล์ค")).unwrap();
        let mid = add_meeting(&conn, "งานC", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Undecided, false, Some(true)).unwrap();
        let a = attendee_map(&conn).unwrap().get(&(mid, cid)).unwrap().clone();
        assert_eq!(a.status, AttendeeStatus::Undecided);
        assert_eq!(a.attended, Some(true));
        assert_eq!(list_activities(&conn, cid).unwrap()[0].kind, MEETING_ATTENDED_KIND);
    }

    #[test]
    fn remove_attendee_clears_the_cell() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ล")).unwrap();
        let mid = add_meeting(&conn, "งานD", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap();
        remove_attendee(&conn, mid, cid).unwrap();
        assert!(attendee_map(&conn).unwrap().get(&(mid, cid)).is_none());
    }

    #[test]
    fn delete_meeting_cascades_attendees() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ม")).unwrap();
        let mid = add_meeting(&conn, "งานE", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap();
        delete_meeting(&conn, mid).unwrap();
        assert!(attendee_map(&conn).unwrap().is_empty());
    }

    #[test]
    fn delete_contact_cascades_attendees() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ค")).unwrap();
        let mid = add_meeting(&conn, "งานF", d("2026-07-01"), d("2026-07-01"), "", 0).unwrap();
        upsert_attendee(&conn, mid, cid, AttendeeStatus::Attending, false, None).unwrap();
        delete_contact(&conn, cid).unwrap();
        assert!(attendee_map(&conn).unwrap().is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test attendee -- --nocapture`
Expected: FAIL — `cannot find function upsert_attendee`/`attendee_map`/`remove_attendee`.

- [ ] **Step 3: Implement the attendee queries**

In `src/db/queries.rs`, append to the Meetings section (after `list_meetings`):

```rust
/// Every attendee cell, keyed by `(meeting_id, contact_id)`. Small enough to
/// load whole; the matrix page looks up only the cells it renders.
pub fn attendee_map(conn: &Connection) -> Result<HashMap<(i64, i64), MeetingAttendee>> {
    let mut stmt = conn
        .prepare("SELECT meeting_id, contact_id, status, paid, attended FROM meeting_attendees")?;
    let rows = stmt.query_map([], |row| {
        let status: String = row.get(2)?;
        let attended: Option<i64> = row.get(4)?;
        Ok(MeetingAttendee {
            meeting_id: row.get(0)?,
            contact_id: row.get(1)?,
            status: AttendeeStatus::from_db(&status),
            paid: row.get::<_, i64>(3)? != 0,
            attended: attended.map(|v| v != 0),
        })
    })?;
    let mut map = HashMap::new();
    for r in rows {
        let a = r?;
        map.insert((a.meeting_id, a.contact_id), a);
    }
    Ok(map)
}

/// Insert or update one attendee cell. Logs an activity on the two milestone
/// transitions — both inside one transaction (mirrors `collect_advance`):
/// status becoming `Attending`, and `attended` becoming `true` — each only when
/// the prior state was different, so repeated writes don't spam the history.
pub fn upsert_attendee(
    conn: &Connection,
    meeting_id: i64,
    contact_id: i64,
    status: AttendeeStatus,
    paid: bool,
    attended: Option<bool>,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let prior: Option<(String, Option<i64>)> = tx
        .query_row(
            "SELECT status, attended FROM meeting_attendees WHERE meeting_id = ?1 AND contact_id = ?2",
            params![meeting_id, contact_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let was_attending = prior
        .as_ref()
        .is_some_and(|(s, _)| AttendeeStatus::from_db(s) == AttendeeStatus::Attending);
    let was_attended = prior.as_ref().and_then(|(_, a)| *a).is_some_and(|v| v != 0);

    let now = Local::now().to_rfc3339();
    tx.execute(
        "INSERT INTO meeting_attendees
            (meeting_id, contact_id, status, paid, attended, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
         ON CONFLICT(meeting_id, contact_id) DO UPDATE SET
            status = ?3, paid = ?4, attended = ?5, updated_at = ?6",
        params![
            meeting_id,
            contact_id,
            status.as_str(),
            paid as i64,
            attended.map(|b| b as i64),
            now,
        ],
    )?;

    if status == AttendeeStatus::Attending && !was_attending {
        let name: String =
            tx.query_row("SELECT name FROM meetings WHERE id = ?1", [meeting_id], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                contact_id,
                MEETING_RSVP_KIND,
                format!("ตอบรับเข้าร่วม: {name}"),
                Local::now().to_rfc3339()
            ],
        )?;
    }
    if attended == Some(true) && !was_attended {
        let name: String =
            tx.query_row("SELECT name FROM meetings WHERE id = ?1", [meeting_id], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                contact_id,
                MEETING_ATTENDED_KIND,
                format!("เข้าร่วมงานจริง: {name}"),
                Local::now().to_rfc3339()
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Remove a contact from a meeting (the cell returns to empty).
pub fn remove_attendee(conn: &Connection, meeting_id: i64, contact_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM meeting_attendees WHERE meeting_id = ?1 AND contact_id = ?2",
        params![meeting_id, contact_id],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test attendee -- --nocapture`
Expected: all attendee tests PASS.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test`
Expected: every test PASS (the new tests plus all pre-existing ones).

- [ ] **Step 6: Commit**

```bash
git add src/db/queries.rs
git commit -m "$(cat <<'EOF'
Add attendee matrix queries: attendee_map, upsert_attendee, remove_attendee

upsert_attendee logs an activity on the attending and attended milestones,
each only on transition, in one transaction.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: DbConnection passthroughs

**Files:**
- Modify: `src/db/mod.rs` (imports + a new section after the Advances passthroughs, line 212)

- [ ] **Step 1: Extend the imports in `src/db/mod.rs`**

Add at the top with the other `use` lines:

```rust
use std::collections::HashMap;
```

Change the enums import (line 20) to include `AttendeeStatus`:

```rust
use crate::models::enums::{AttendeeStatus, ContactType, SponsorStep};
```

Add the meeting-models import (after line 21):

```rust
use crate::models::meeting::{Meeting, MeetingAttendee};
```

- [ ] **Step 2: Add the passthroughs**

In `impl DbConnection`, after `outstanding_total` (line 212), add:

```rust
    // --- meetings ---------------------------------------------------------

    pub fn add_meeting(
        &self,
        name: &str,
        start: NaiveDate,
        end: NaiveDate,
        description: &str,
        fee: i64,
    ) -> Result<i64> {
        queries::add_meeting(&self.conn, name, start, end, description, fee)
    }
    pub fn update_meeting(&self, m: &Meeting) -> Result<()> {
        queries::update_meeting(&self.conn, m)
    }
    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        queries::delete_meeting(&self.conn, id)
    }
    pub fn list_meetings(&self, include_past: bool) -> Result<Vec<Meeting>> {
        queries::list_meetings(&self.conn, include_past)
    }
    pub fn attendee_map(&self) -> Result<HashMap<(i64, i64), MeetingAttendee>> {
        queries::attendee_map(&self.conn)
    }
    pub fn upsert_attendee(
        &self,
        meeting_id: i64,
        contact_id: i64,
        status: AttendeeStatus,
        paid: bool,
        attended: Option<bool>,
    ) -> Result<()> {
        queries::upsert_attendee(&self.conn, meeting_id, contact_id, status, paid, attended)
    }
    pub fn remove_attendee(&self, meeting_id: i64, contact_id: i64) -> Result<()> {
        queries::remove_attendee(&self.conn, meeting_id, contact_id)
    }
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles. (Transient `dead_code` warnings for the new passthroughs are expected until Task 7.)

- [ ] **Step 4: Commit**

```bash
git add src/db/mod.rs
git commit -m "$(cat <<'EOF'
Expose meeting + attendee operations on DbConnection

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Meeting form modal + confirm variant + modal wiring

**Files:**
- Create: `src/ui/meeting_form.rs`
- Modify: `src/ui/confirm.rs` (add the `Meeting` variant + match arms)
- Modify: `src/ui/mod.rs` (register `meeting_form`)
- Modify: `src/app.rs` (`meeting_form` field + default + render the modal)

- [ ] **Step 1: Add the `Meeting` delete variant in `src/ui/confirm.rs`**

Add the variant to the `PendingDelete` enum (after `Advance`, line 15):

```rust
    Meeting { id: i64, name: String },
```

Add the name/detail arm in the `match &pending` block (after the `Advance` arm, line 47):

```rust
        PendingDelete::Meeting { name, .. } => {
            (name.clone(), "งานประชุมนี้และสถานะการเข้าร่วมทั้งหมดของงานนี้จะถูกลบถาวร".to_string())
        }
```

Add the delete arm in the `match &pending` inside `if confirm` (after the `Advance` arm, line 87):

```rust
            PendingDelete::Meeting { id, .. } => app.db.delete_meeting(*id),
```

- [ ] **Step 2: Create `src/ui/meeting_form.rs`**

```rust
//! Add/edit meeting modal.
//!
//! Form state lives in [`MeetingForm`] on the `AppState`; the page opens it via
//! `MeetingForm::for_new()` / `for_edit(&Meeting)`. Rendered as an `egui::Window`;
//! on save it builds a [`Meeting`] and writes through `AppState.db`. The "ลบงาน"
//! button routes through the shared confirm dialog.

use chrono::{Local, NaiveDate};
use egui_extras::DatePickerButton;

use crate::app::AppState;
use crate::models::meeting::Meeting;
use crate::ui::confirm::PendingDelete;
use crate::ui::ACCENT;

const LABEL_W: f32 = 120.0;
const FIELD_W: f32 = 300.0;

/// One labelled form row (mirrors the helper in `ui/advances.rs`).
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

/// Add/edit form state for a meeting.
pub struct MeetingForm {
    pub open: bool,
    /// `Some(id)` when editing; `None` when adding.
    pub editing_id: Option<i64>,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub description: String,
    pub fee: i64,
}

impl Default for MeetingForm {
    fn default() -> Self {
        let today = Local::now().date_naive();
        MeetingForm {
            open: false,
            editing_id: None,
            name: String::new(),
            start_date: today,
            end_date: today,
            description: String::new(),
            fee: 0,
        }
    }
}

impl MeetingForm {
    pub fn for_new() -> Self {
        MeetingForm {
            open: true,
            ..Default::default()
        }
    }

    pub fn for_edit(m: &Meeting) -> Self {
        MeetingForm {
            open: true,
            editing_id: Some(m.id),
            name: m.name.clone(),
            start_date: m.start_date,
            end_date: m.end_date,
            description: m.description.clone(),
            fee: m.fee,
        }
    }
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    if !app.meeting_form.open {
        return;
    }
    let editing = app.meeting_form.editing_id;
    let title = if editing.is_some() {
        "แก้ไขงานประชุม / Edit Meeting"
    } else {
        "เพิ่มงานประชุม / Add Meeting"
    };

    let mut window_open = true;
    let mut save = false;
    let mut cancel = false;
    let mut delete = false;

    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut window_open)
        .show(ctx, |ui| {
            let f = &mut app.meeting_form;
            ui.add_space(4.0);
            field_row(ui, "ชื่องาน *", |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut f.name)
                        .hint_text("เช่น สัมมนา CCS ประจำเดือน")
                        .desired_width(FIELD_W),
                );
            });
            field_row(ui, "วันที่เริ่ม", |ui| {
                ui.add(DatePickerButton::new(&mut f.start_date).id_source("meeting_start_picker"));
            });
            field_row(ui, "วันที่สิ้นสุด", |ui| {
                ui.add(DatePickerButton::new(&mut f.end_date).id_source("meeting_end_picker"));
            });
            field_row(ui, "ค่าเข้างาน (บาท)", |ui| {
                ui.add(egui::DragValue::new(&mut f.fee).range(0..=99_999_999).suffix(" บาท"));
            });
            field_row(ui, "รายละเอียด", |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut f.description)
                        .hint_text("ไม่บังคับ เช่น สถานที่ / วิทยากร")
                        .desired_rows(3)
                        .desired_width(FIELD_W),
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new(egui::RichText::new("💾 บันทึก").strong()).fill(ACCENT))
                    .clicked()
                {
                    save = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
                if editing.is_some() {
                    ui.add_space(20.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("🗑 ลบงาน").color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(0xD3, 0x2F, 0x2F)),
                        )
                        .clicked()
                    {
                        delete = true;
                    }
                }
            });
        });

    if delete {
        if let Some(id) = editing {
            app.pending_delete = Some(PendingDelete::Meeting {
                id,
                name: app.meeting_form.name.clone(),
            });
        }
        app.meeting_form.open = false;
        return;
    }
    if cancel || !window_open {
        app.meeting_form.open = false;
        return;
    }
    if save {
        let f = &app.meeting_form;
        let result = match editing {
            Some(id) => app.db.update_meeting(&Meeting {
                id,
                name: f.name.clone(),
                start_date: f.start_date,
                end_date: f.end_date,
                description: f.description.clone(),
                fee: f.fee,
                created_at: Local::now(), // ignored by update_meeting
            }),
            None => app
                .db
                .add_meeting(&f.name, f.start_date, f.end_date, &f.description, f.fee)
                .map(|_| ()),
        };
        match result {
            Ok(()) => {
                app.set_status(if editing.is_some() { "บันทึกงานแล้ว" } else { "เพิ่มงานแล้ว" });
                app.meeting_form.open = false;
            }
            Err(e) => app.set_error(e),
        }
    }
}
```

- [ ] **Step 3: Register the module in `src/ui/mod.rs`**

Add to the module list (keep alphabetical — after `pub modledger`-style ordering, place it just before `pub mod meetings;` which Task 7 adds; for now add it after `pub mod forms;`):

```rust
pub mod meeting_form;
```

- [ ] **Step 4: Add the `meeting_form` field to `AppState`**

In `src/app.rs`, add the field to the `AppState` struct (after `advance_status_filter`, line 104):

```rust
    /// Add/edit meeting modal state.
    pub meeting_form: crate::ui::meeting_form::MeetingForm,
```

Initialise it in `AppState::new` (after `advance_status_filter: ...`, line 160):

```rust
            meeting_form: crate::ui::meeting_form::MeetingForm::default(),
```

- [ ] **Step 5: Render the modal**

In `src/app.rs` `update`, in the "Modals render on top" block (after `ui::advance_collect::render(self, ctx);`, line 396), add:

```rust
        ui::meeting_form::render(self, ctx);
```

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: compiles. (Transient `dead_code` on `MeetingForm::for_new`/`for_edit` until Task 7.)

- [ ] **Step 7: Commit**

```bash
git add src/ui/meeting_form.rs src/ui/confirm.rs src/ui/mod.rs src/app.rs
git commit -m "$(cat <<'EOF'
Add meeting add/edit modal + Meeting delete confirm variant

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Meetings matrix page + menu + dispatch

**Files:**
- Create: `src/ui/meetings.rs`
- Modify: `src/ui/mod.rs` (register `meetings`; add `View::Meetings`)
- Modify: `src/app.rs` (`meeting_who_filter` + `meeting_show_past` fields + defaults; sidebar item; dispatch)

- [ ] **Step 1: Create `src/ui/meetings.rs`**

```rust
//! งานประชุม (Meetings): a contacts × meetings attendance matrix. Rows are
//! contacts (filterable by name and type); each column is a meeting (by default
//! only those not yet finished). Each cell shows the contact's RSVP status, an
//! entry-fee paid marker, and — once recorded — actual attendance; clicking a
//! cell opens a popup to set them. Clicking a column header edits that meeting.

use std::collections::HashMap;

use egui_extras::{Column, TableBuilder};

use crate::app::AppState;
use crate::db::queries::group_thousands;
use crate::models::contact::Contact;
use crate::models::enums::{AttendeeStatus, ContactType};
use crate::models::meeting::{Meeting, MeetingAttendee};
use crate::ui::meeting_form::MeetingForm;
use crate::ui::{ACCENT, ACCENT_STRONG};

/// Contact-type filter on the Meetings page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MeetingWhoFilter {
    All,
    Type(ContactType),
}

impl MeetingWhoFilter {
    fn label(self) -> &'static str {
        match self {
            MeetingWhoFilter::All => "ทั้งหมด",
            MeetingWhoFilter::Type(t) => t.label_th(),
        }
    }
}

/// A deferred cell edit, applied after the table closure (so it does not borrow
/// `app.db` while the table borrows `app`).
enum CellAction {
    Upsert {
        meeting_id: i64,
        contact_id: i64,
        status: AttendeeStatus,
        paid: bool,
        attended: Option<bool>,
    },
    Remove {
        meeting_id: i64,
        contact_id: i64,
    },
}

fn status_color(s: AttendeeStatus) -> egui::Color32 {
    match s {
        AttendeeStatus::Attending => egui::Color32::from_rgb(0x2E, 0x7D, 0x32), // green
        AttendeeStatus::Undecided => egui::Color32::from_rgb(0x9E, 0x9E, 0x9E), // grey
        AttendeeStatus::NotAttending => egui::Color32::from_rgb(0xD3, 0x2F, 0x2F), // red
    }
}

fn type_color(t: ContactType) -> egui::Color32 {
    match t {
        ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
        ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
        ContactType::Abo => ACCENT_STRONG,
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("งานประชุม / Meetings");
    ui.label(
        egui::RichText::new("ตามรายชื่อเข้าร่วมงาน — คลิกช่องเพื่อตั้งสถานะเข้าร่วม / จ่ายเงิน / ผลหลังงาน")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Toolbar.
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new("➕ เพิ่มงานประชุม").fill(ACCENT)).clicked() {
            app.meeting_form = MeetingForm::for_new();
        }
        ui.separator();
        ui.label("🔍");
        ui.add(
            egui::TextEdit::singleline(&mut app.search)
                .hint_text("ค้นหาชื่อ")
                .desired_width(160.0),
        );
        if ui.button("ล้าง").clicked() {
            app.search.clear();
        }
        ui.separator();
        ui.label("ประเภท:");
        egui::ComboBox::from_id_source("meeting_who_cb")
            .selected_text(app.meeting_who_filter.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.meeting_who_filter,
                    MeetingWhoFilter::All,
                    MeetingWhoFilter::All.label(),
                );
                for t in ContactType::ALL {
                    ui.selectable_value(
                        &mut app.meeting_who_filter,
                        MeetingWhoFilter::Type(t),
                        t.label_th(),
                    );
                }
            });
        ui.separator();
        ui.checkbox(&mut app.meeting_show_past, "แสดงงานที่ผ่านมาแล้ว");
    });
    ui.add_space(8.0);

    // Load data (pre-fetched so the table closure does not borrow app.db).
    let meetings = app.handle(app.db.list_meetings(app.meeting_show_past), Vec::new());
    let contacts = app.handle(app.db.list_contacts(), Vec::new());
    let attendees = app.handle(app.db.attendee_map(), HashMap::new());

    // Filter rows by the shared search box and the contact-type filter.
    let needle = app.search.trim().to_lowercase();
    let who = app.meeting_who_filter;
    let rows: Vec<&Contact> = contacts
        .iter()
        .filter(|c| {
            let name_ok = needle.is_empty()
                || c.name.to_lowercase().contains(&needle)
                || c.nickname.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle));
            let who_ok = match who {
                MeetingWhoFilter::All => true,
                MeetingWhoFilter::Type(t) => c.contact_type == t,
            };
            name_ok && who_ok
        })
        .collect();

    if meetings.is_empty() {
        ui.weak("— ยังไม่มีงานประชุม กดปุ่ม ➕ เพิ่มงานประชุม เพื่อเริ่ม —");
        return;
    }
    if rows.is_empty() {
        ui.weak("— ไม่มีรายชื่อในตัวกรองนี้ —");
        return;
    }

    let mut action: Option<CellAction> = None;
    let mut edit_meeting: Option<i64> = None;

    let mut table = TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(170.0)); // contact name
    for _ in &meetings {
        table = table.column(Column::auto().at_least(96.0)); // one per meeting
    }
    table
        .header(40.0, |mut header| {
            header.col(|ui| {
                ui.strong("รายชื่อ \\ งาน");
            });
            for m in &meetings {
                header.col(|ui| {
                    ui.vertical(|ui| {
                        let title = ui
                            .add(
                                egui::Label::new(
                                    egui::RichText::new(&m.name).strong().color(ACCENT_STRONG),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("คลิกเพื่อแก้ไข / ลบงาน")
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if title.clicked() {
                            edit_meeting = Some(m.id);
                        }
                        let date = if m.start_date == m.end_date {
                            m.start_date.format("%d/%m/%y").to_string()
                        } else {
                            format!("{}–{}", m.start_date.format("%d/%m"), m.end_date.format("%d/%m/%y"))
                        };
                        ui.label(egui::RichText::new(date).small().weak());
                        let fee = if m.fee > 0 {
                            format!("{} บาท", group_thousands(m.fee))
                        } else {
                            "ฟรี".to_string()
                        };
                        ui.label(egui::RichText::new(fee).small().weak());
                    });
                });
            }
        })
        .body(|mut body| {
            for c in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(c.display_name()).color(type_color(c.contact_type)),
                        );
                    });
                    for m in &meetings {
                        tr.col(|ui| {
                            let cell = attendees.get(&(m.id, c.id));
                            cell_widget(ui, m, c.id, cell, &mut action);
                        });
                    }
                });
            }
        });

    // Apply deferred edits.
    if let Some(act) = action {
        let result = match act {
            CellAction::Upsert { meeting_id, contact_id, status, paid, attended } => {
                app.db.upsert_attendee(meeting_id, contact_id, status, paid, attended)
            }
            CellAction::Remove { meeting_id, contact_id } => {
                app.db.remove_attendee(meeting_id, contact_id)
            }
        };
        if let Err(e) = result {
            app.set_error(e);
        }
    }
    if let Some(id) = edit_meeting {
        if let Some(m) = meetings.iter().find(|m| m.id == id) {
            app.meeting_form = MeetingForm::for_edit(m);
        }
    }
}

/// Render one matrix cell: the status/paid/attended markers as a clickable label,
/// plus the edit popup. Records the chosen change into `action`.
fn cell_widget(
    ui: &mut egui::Ui,
    m: &Meeting,
    contact_id: i64,
    cell: Option<&MeetingAttendee>,
    action: &mut Option<CellAction>,
) {
    let popup_id = ui.make_persistent_id(("meeting_cell", m.id, contact_id));
    let cur_status = cell.map(|a| a.status);
    let cur_paid = cell.is_some_and(|a| a.paid);
    let cur_attended = cell.and_then(|a| a.attended);

    let resp = match cell {
        Some(a) => {
            let mut text = String::from("●");
            if m.fee > 0 && a.paid {
                text.push_str(" 💵");
            }
            match a.attended {
                Some(true) => text.push_str(" ✓"),
                Some(false) => text.push_str(" ✗"),
                None => {}
            }
            ui.add(
                egui::Label::new(egui::RichText::new(text).color(status_color(a.status)))
                    .sense(egui::Sense::click()),
            )
        }
        None => ui.add(
            egui::Label::new(egui::RichText::new("·").weak()).sense(egui::Sense::click()),
        ),
    }
    .on_hover_cursor(egui::CursorIcon::PointingHand);

    if resp.clicked() {
        ui.memory_mut(|mm| mm.toggle_popup(popup_id));
    }

    let base_status = cur_status.unwrap_or(AttendeeStatus::Undecided);
    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::popup::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(170.0);
            ui.label(egui::RichText::new("สถานะตอบรับ").small().weak());
            for s in AttendeeStatus::ALL {
                if ui.selectable_label(cur_status == Some(s), s.label_th()).clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: s,
                        paid: cur_paid,
                        attended: cur_attended,
                    });
                }
            }
            if cell.is_some() && ui.selectable_label(false, "เอาออกจากงาน").clicked() {
                *action = Some(CellAction::Remove { meeting_id: m.id, contact_id });
            }

            if m.fee > 0 {
                ui.separator();
                let mut paid = cur_paid;
                if ui.checkbox(&mut paid, "จ่ายค่าเข้างานแล้ว").changed() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid,
                        attended: cur_attended,
                    });
                }
            }

            ui.separator();
            ui.label(egui::RichText::new("ผลหลังงาน").small().weak());
            ui.horizontal(|ui| {
                if ui.selectable_label(cur_attended == Some(true), "มาจริง").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: Some(true),
                    });
                }
                if ui.selectable_label(cur_attended == Some(false), "ไม่มา").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: Some(false),
                    });
                }
                if ui.selectable_label(cur_attended.is_none(), "ล้าง").clicked() {
                    *action = Some(CellAction::Upsert {
                        meeting_id: m.id,
                        contact_id,
                        status: base_status,
                        paid: cur_paid,
                        attended: None,
                    });
                }
            });
        },
    );
}
```

- [ ] **Step 2: Register the module + `View::Meetings` in `src/ui/mod.rs`**

Add to the module list (after `pub mod meeting_form;`):

```rust
pub mod meetings;
```

Add the variant to the `View` enum (after `FollowUp`, to match the sidebar order):

```rust
    Meetings,
```

- [ ] **Step 3: Add the page's filter fields to `AppState`**

In `src/app.rs`, add to the `AppState` struct (right after the `meeting_form` field from Task 6):

```rust
    /// Contact-type filter on the Meetings page.
    pub meeting_who_filter: crate::ui::meetings::MeetingWhoFilter,
    /// Whether the Meetings matrix also shows meetings that have already finished.
    pub meeting_show_past: bool,
```

Initialise them in `AppState::new` (right after the `meeting_form: ...` line):

```rust
            meeting_who_filter: crate::ui::meetings::MeetingWhoFilter::All,
            meeting_show_past: false,
```

- [ ] **Step 4: Add the sidebar item**

In `src/app.rs` `sidebar`, in the `items` array, add the entry after the `FollowUp` line (line 210):

```rust
            (View::Meetings, "🎟  งานประชุม"),
```

- [ ] **Step 5: Add the dispatch arm**

In `src/app.rs` `update`'s central-panel match, add after the `View::FollowUp` arm (line 383):

```rust
            View::Meetings => ui::meetings::render(self, ui),
```

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: compiles cleanly with **no warnings** (everything is now wired to `main`).

- [ ] **Step 7: Run the full suite**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 8: Commit**

```bash
git add src/ui/meetings.rs src/ui/mod.rs src/app.rs
git commit -m "$(cat <<'EOF'
Add the งานประชุม (Meetings) matrix page, menu, and dispatch

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Visual verification & glyph check

**Files:** none (verification only)

This app is verified visually for UI work (there is no egui test harness). See the memory note `egui-app-screenshot-verify` for the screenshot workflow and `egui-emoji-glyph-subset` for the glyph caveat.

- [ ] **Step 1: Full test + clean build**

Run: `cargo test` (expect all PASS) then `cargo build` (expect zero warnings).
**Do NOT run `cargo fmt`.**

- [ ] **Step 2: Launch and navigate to the Meetings page**

Build and run `target/debug/amway_ccs_tracker.exe`. (Optionally, temporarily set the initial `view: View::Meetings` in `AppState::new` to land there directly, and add a couple of meetings + tick some cells. Revert any temporary `view:` change afterwards.)

- [ ] **Step 3: Verify the glyphs render (no tofu boxes)**

Confirm these render as intended, not as missing-glyph boxes:
- Sidebar icon `🎟` on the "งานประชุม" item.
- Cell markers: `●` (coloured by status), `💵` (paid), `✓` (came), `✗` (no-show).

If any glyph shows as tofu, replace it per `egui-emoji-glyph-subset`:
- For `🎟`: substitute a glyph confirmed present in the bundled subset (e.g. one already used in the sidebar such as `📅`/`📋`-style), or draw nothing-special text.
- For `●`/`✓`/`✗`: if absent, swap for a painter-drawn filled circle (like `ui::combo_button` draws its frame) or a confirmed dingbat (`✅` / `✖` are used elsewhere in the app and are known to render).

Re-run `cargo build` after any swap. Commit only if a swap was needed:

```bash
git add -A
git commit -m "$(cat <<'EOF'
Swap meeting glyphs not present in egui's bundled font subset

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 4: Smoke-test the interactions**

- Add a meeting via ➕; confirm it appears as a column (and only if unfinished, unless "แสดงงานที่ผ่านมาแล้ว" is on).
- Click a cell → set "จะเข้าร่วม"; confirm the dot turns green and an activity appears on that contact's history (📝 ประวัติติดต่อ).
- Tick "จ่ายค่าเข้างานแล้ว" (only shown when fee > 0) → `💵` appears.
- Record "มาจริง" → `✓` appears and a second activity is logged; "เอาออกจากงาน" clears the cell.
- Click a column header → edit the meeting; delete it → its column and cells disappear.

---

## Self-Review

**1. Spec coverage**
- Two tables + seeded kinds → Task 1. ✓
- `AttendeeStatus` enum + models → Task 2. ✓
- Meeting CRUD + past filter → Task 3. ✓
- Matrix fetch + upsert (with both logging milestones) + remove → Task 4. ✓
- DbConnection passthroughs → Task 5. ✓
- Add/edit modal + delete via confirm → Task 6. ✓
- Matrix page (cell display + popup, who-filter, show-past, free-meeting hiding, empty states, header-edit) + menu/dispatch → Task 7. ✓
- Activity logging on attending & attended, walk-in Undecided, cascades → Task 4 tests. ✓
- Glyph/visual verification → Task 8. ✓

**2. Placeholder scan** — none; every code step contains complete code.

**3. Type consistency** — `add_meeting(name:&str, start, end, description:&str, fee)` and `upsert_attendee(meeting_id, contact_id, status, paid, attended)` are used identically in queries, passthroughs, tests, and UI. `MeetingForm::for_new`/`for_edit`, `MeetingWhoFilter::{All,Type}`, `CellAction::{Upsert,Remove}`, and `PendingDelete::Meeting { id, name }` match across tasks. `attendee_map` returns `HashMap<(i64,i64), MeetingAttendee>` everywhere it appears.
