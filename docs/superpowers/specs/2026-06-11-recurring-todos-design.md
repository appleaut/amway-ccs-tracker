# Recurring (Scheduled) Todos — Design

**Date:** 2026-06-11

## Goal

Let the user define recurring task *schedules* on a dedicated page. When a
schedule's cycle date arrives or has passed, the app automatically creates a
normal Todo on the existing Todo List page — no manual re-entry.

## Decisions (locked during brainstorming)

- **Recurrence kinds:** "every N days" + "monthly on a chosen day-of-month".
  (No daily / weekly-by-weekday — those are out of scope.)
- **Missed cycles collapse:** if several cycles passed since the app was last
  open, create **one** Todo for the most recent passed occurrence — never a
  backlog of duplicates.
- **No end condition:** a schedule recurs forever until the user deletes it.
- **Separate management page:** schedules are managed on a new page (like
  Meetings / Advances). Generated Todos appear in the normal Todo List.
- **Generated Todo due date:** the most recent passed occurrence date, so an
  overdue cycle shows red ("เลยกำหนด") immediately.
- **Generation timing:** check on app start **and** whenever the calendar date
  changes while the app stays open (one mechanism covers both).
- **Start date:** user-chosen, default today. If `start_date <= today` the
  first Todo appears at once.
- **Monthly clamping:** day 29–31 in a month that lacks that day clamps to the
  last day of the month (e.g. day 31 in February → 28/29).

## 1. Data Model

### New table `todo_schedules` (migration v11)

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | |
| `contact_id` | INTEGER, nullable, `REFERENCES contacts(id) ON DELETE SET NULL` | optional contact link (mirrors `todos`) |
| `task` | TEXT NOT NULL | task text to recreate each cycle |
| `freq_kind` | TEXT NOT NULL | `'EveryNDays'` or `'MonthlyDay'` |
| `freq_value` | INTEGER NOT NULL | EveryNDays → number of days (≥1); MonthlyDay → day-of-month (1–31) |
| `start_date` | TEXT NOT NULL | cycle anchor; first occurrence appears once `start_date <= today` |
| `last_generated` | TEXT, nullable | occurrence date of the last Todo created (NULL = none yet) |
| `created_at` | TEXT NOT NULL | |

Index: `idx_todo_schedules_contact ON todo_schedules(contact_id)`.

Deleting a contact nulls the link (the schedule survives), matching `todos`.

### Rust model (`src/models/todo_schedule.rs`)

```rust
pub enum Recurrence {
    EveryNDays(u32),   // n >= 1
    MonthlyDay(u32),   // 1..=31, clamped to month end
}

pub struct TodoSchedule {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub task: String,
    pub recurrence: Recurrence,
    pub start_date: NaiveDate,
    pub last_generated: Option<NaiveDate>,
    pub created_at: DateTime<Local>,
}
```

`freq_kind` + `freq_value` map to/from the `Recurrence` enum at the DB boundary
(invalid kind string → error). A human label helper renders "ทุก 7 วัน" /
"ทุกวันที่ 1".

## 2. Generation Logic

### Pure function on `Recurrence`

`latest_occurrence_on_or_before(start: NaiveDate, today: NaiveDate) -> Option<NaiveDate>`

- **EveryNDays(n):** if `today < start` → `None`; else
  `start + ((today - start).num_days() / n as i64) * n` days — the most recent
  occurrence on or before today.
- **MonthlyDay(d):** walk back from `today`'s month to find the first month whose
  occurrence (day `d` clamped to the month's last day) is `<= today` **and**
  `>= start`; if none qualifies → `None`.

`next_occurrence_after(start, today)` is the same idea forward — used only to
display "รอบถัดไป" in the schedule table (not for generation).

### Query `generate_due_todos(conn, today) -> Result<usize>`

Returns the count of Todos created.

1. Load all schedules.
2. For each: `occ = recurrence.latest_occurrence_on_or_before(start, today)`.
3. If `occ` is `Some` **and** (`last_generated` is `None` **or** `occ >
   last_generated`):
   - In **one transaction**:
     - `INSERT INTO todos (contact_id, task, due_date=occ, done=0, created_at=now)`
     - `UPDATE todo_schedules SET last_generated = occ WHERE id = ?`
   - The transaction guards against creating a Todo without recording it (which
     would duplicate on the next check).

### Call site (`src/app.rs`)

- New field `last_gen_check: NaiveDate` on `AppState`.
- Run generation once in `AppState::new` (covers app start).
- Each frame in `update`: `let today = Local::now().date_naive(); if today !=
  self.last_gen_check { self.last_gen_check = today; generate_due_todos(...); }`
  — covers a date change while the app stays open. Errors route through the
  existing `app.handle` / status-bar mechanism.

Generated Todos are ordinary rows: editing, deleting, or completing one does not
touch its schedule, which keeps producing future cycles.

## 3. UI (`src/ui/todo_schedules.rs`)

- **Menu / navigation:** add a "🔁 ตารางงานประจำ" entry to the sidebar, a new
  `View` variant, and a dispatch arm in `app.rs` (mirrors Meetings / Advances).
  If 🔁 renders as tofu in egui's bundled font subset, fall back to a text-only
  label (verify the cmap first per project memory).
- **Layout** mirrors the Todo page (two cards above a table):
  - **Left card — add/edit form** (`TodoScheduleForm` in `AppState`):
    - `สิ่งที่ต้องทำ` — TextEdit
    - `เกี่ยวกับ` — `filter_combo` contact picker ("— ไม่ระบุ —")
    - `รอบ` — ComboBox: "ทุก N วัน" / "รายเดือน (วันที่กำหนด)"
    - value field by kind: EveryNDays → DragValue days (≥1); MonthlyDay →
      DragValue day 1–31
    - `วันเริ่ม` — DatePickerButton (default today)
    - buttons: ➕ เพิ่ม / 💾 บันทึก + ยกเลิก
  - **Table** (`TableBuilder`): `สิ่งที่ต้องทำ` · `เกี่ยวกับ` · `รอบ` (label) ·
    `รอบถัดไป` (next occurrence after today) · `จัดการ` (✏ / 🗑).
- **Delete via confirm modal (always):** add `PendingDelete::TodoSchedule { id,
  name }` to `src/ui/confirm.rs` (name/detail arm + dispatch to
  `app.db.delete_todo_schedule`). Use the existing deferred-delete pattern
  (collect `delete_req` inside the table loop, set `app.pending_delete` after).
- **Footer note:** a weak line explaining that Todos are created automatically on
  the "สิ่งที่ต้องทำ" page when a cycle is due, so the user knows where the real
  tasks appear.

## 4. Testing

### Unit tests (`models/todo_schedule.rs`, no DB)

- EveryNDays: `today < start` → `None`; exactly on start → start; mid-cycle →
  last passed occurrence (start 1, N=7, today 23 → 22); exactly on a cycle →
  that day.
- MonthlyDay: today after day `d` this month → this month; before day `d` →
  previous month; day 31 in February → clamped to 28/29; `today < start` →
  `None`.

### Integration tests (in-memory SQLite, existing pattern)

- `generate_due_todos` creates one Todo when due, with `due_date` = occurrence.
- Calling again the same day creates nothing (`last_generated` guard).
- Several cycles passed → only one Todo (latest occurrence).
- Schedule whose `start_date` is in the future → nothing created.
- Deleting a contact leaves the schedule (its `contact_id` becomes NULL).
- Migration v11: opening an older DB creates the new table.

### Verification before completion

`cargo build` + `cargo test` (do **not** run `cargo fmt` — repo is
hand-formatted). Launch the real app to confirm the new page and menu render and
the 🔁 glyph is not tofu.
