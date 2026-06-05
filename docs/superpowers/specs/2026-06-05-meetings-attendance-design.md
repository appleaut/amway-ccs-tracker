# Meetings & Attendance Tracking — Design

**Date:** 2026-06-05
**Status:** Approved (design)

## Goal

Add a **"งานประชุม" (Meetings)** menu that lets the user set up meetings/events and
track which contacts (ผู้มุ่งหวัง / ลูกค้า VIP / นักธุรกิจ) they are following to
attend each one, recording per-person **RSVP status** and **entry-fee payment** in a
single matrix view.

## Overview / Architecture

A new sidebar entry **"🎟 งานประชุม"** (placed right after "✅ ติดตามผล") opens one
page built around a **matrix table**:

- **Rows (Y):** contacts — all three types, filterable by search text and contact type.
- **Columns (X):** meetings — by default only meetings that have not finished
  (`end_date >= today`), with a toggle to also show past meetings.
- **Cell (row × column):** that contact's RSVP status for that meeting, plus whether
  they have paid the entry fee. An empty cell means "not yet followed / not invited".

The matrix is rendered with `egui_extras::TableBuilder` (consistent with every other
table in the app), generating one `Column` per shown meeting dynamically. `egui::Grid`
and a custom frozen-first-column split were considered and rejected: TableBuilder
matches the codebase and gives a sticky header + striping for free, and because the
default view shows only unfinished meetings the column count stays small enough to fit
the window without horizontal scrolling.

## Tech Stack

Rust, eframe/egui 0.28, egui_extras 0.28 (`TableBuilder`), rusqlite 0.31, chrono.
No new dependencies.

## Data Model (migration v10)

### Table `meetings`

| Column        | Type    | Notes                                            |
|---------------|---------|--------------------------------------------------|
| `id`          | INTEGER | PK AUTOINCREMENT                                 |
| `name`        | TEXT    | NOT NULL                                         |
| `start_date`  | TEXT    | NOT NULL — ISO `YYYY-MM-DD` (`NaiveDate`)        |
| `end_date`    | TEXT    | NOT NULL — `>= start_date`; form defaults to start|
| `description` | TEXT    | NOT NULL DEFAULT ''                              |
| `fee`         | INTEGER | NOT NULL DEFAULT 0 — entry fee in baht, `>= 0`   |
| `created_at`  | TEXT    | NOT NULL — RFC3339                               |

### Table `meeting_attendees`

| Column       | Type    | Notes                                                       |
|--------------|---------|-------------------------------------------------------------|
| `id`         | INTEGER | PK AUTOINCREMENT                                            |
| `meeting_id` | INTEGER | NOT NULL REFERENCES meetings(id) ON DELETE CASCADE          |
| `contact_id` | INTEGER | NOT NULL REFERENCES contacts(id) ON DELETE CASCADE          |
| `status`     | TEXT    | NOT NULL — `Attending` / `Undecided` / `NotAttending`       |
| `paid`       | INTEGER | NOT NULL DEFAULT 0 — entry-fee paid flag (0/1)              |
| `created_at` | TEXT    | NOT NULL — RFC3339                                          |
| `updated_at` | TEXT    | NOT NULL — RFC3339                                          |

- `UNIQUE(meeting_id, contact_id)` — one row per person per meeting.
- **A row exists only when the contact has been added to that meeting.** No row = empty
  cell. "เอาออกจากงาน" deletes the row (cell returns to empty).
- Indexes on `meeting_id` and `contact_id`.
- Both FKs `ON DELETE CASCADE`: deleting a meeting removes its attendee rows; deleting a
  contact removes that contact's attendee rows. (Unlike advances/todos, an attendance
  cell is meaningless without its meeting and contact, so it is not preserved.)

### Enum `AttendeeStatus` (in `models/enums.rs`)

Closed enum following the existing convention (`as_str` / `label_th` / `from_db`):

| Variant        | `as_str`       | `label_th`   | colour      |
|----------------|----------------|--------------|-------------|
| `Attending`    | `Attending`    | จะเข้าร่วม    | green       |
| `Undecided`    | `Undecided`    | รอตัดสินใจ    | grey/muted  |
| `NotAttending` | `NotAttending` | ไม่เข้า       | red         |

`from_db` falls back to `Undecided` for forward compatibility.

### Seeded activity kind

Migration v10 also seeds a new activity kind **"ตอบรับเข้างานประชุม"** into
`activity_kinds` (same mechanism as the advance-collected kind), exposed as a constant
`MEETING_RSVP_KIND` in `db/queries.rs`.

## UI — the Meetings page

### Toolbar (top)

- `➕ เพิ่มงานประชุม` button → opens the meeting add/edit modal in "new" mode.
- 🔍 search box → filters contact **rows** by name.
- Contact-type filter (ทั้งหมด / ผู้มุ่งหวัง / ลูกค้า VIP / นักธุรกิจ) → filters rows.
- `แสดงงานที่ผ่านมาแล้ว` toggle → when on, columns include meetings whose `end_date`
  is before today; when off (default), only unfinished meetings are shown.

### Matrix table

- First column: contact name + a small contact-type badge (coloured like elsewhere).
- One column per shown meeting, ordered by `start_date`. Header shows the meeting name,
  date range, and fee. Clicking a header opens that meeting in the edit modal.
- Each cell renders the status marker + payment icon:
  - 🟢 จะเข้าร่วม · ⚪ รอตัดสินใจ · 🔴 ไม่เข้า · (blank) = not invited.
  - `💵` appended when `paid = 1`. For a **free meeting (`fee = 0`)** the payment icon
    and the "paid" control are hidden.
- Clicking a cell opens a small popup:
  - status choices: จะเข้าร่วม / รอตัดสินใจ / ไม่เข้า / **เอาออกจากงาน**;
  - a `☐ จ่ายค่าเข้างานแล้ว` checkbox (hidden when `fee = 0`).
- Cell edits are collected during the table closure and applied after it (the established
  pattern in `activities.rs`, which collects `open_contact` / `delete_id` and acts after
  the table), so the `app.db` borrow does not clash with the table borrow.

### Meeting add/edit modal

A modal (new `ui/meeting_form.rs`, dispatched from `app.rs` like the other modals) with:
ชื่องาน, วันที่เริ่ม, วันที่สิ้นสุด (defaults to start), รายละเอียด, ค่าเข้างาน, plus a
**ลบงาน** button. Validation: name non-empty; `end_date >= start_date`; fee parses to an
integer `>= 0` (blank = 0). Errors surface through `app.set_error` like the contact and
advance forms. Deleting a meeting routes through the existing confirm dialog
(`ui/confirm.rs`, extended with a `Meeting` variant) and cascades its attendee rows.

## Behaviour & Edge Cases

- **Activity logging:** when a cell's status becomes `Attending` *and the previous state
  was not Attending* (no prior row, or a different status), one activity is logged for
  that contact — kind `MEETING_RSVP_KIND`, note `"ตอบรับเข้าร่วม: ‹meeting name›"`,
  inside the same transaction that writes the attendee row (mirrors `collect_advance`).
  Switching to other statuses, removing from the meeting, or toggling `paid` logs nothing.
- **Empty states:** if no meetings match the current column filter, the page shows a
  prompt to add one instead of an empty grid. If there are no contacts at all, it shows a
  "no contacts yet" note.
- **Free meetings:** `fee = 0` hides all payment UI for that column.
- **Out of scope:** actual post-event attendance ("came / did not come") is not tracked —
  only the three forward-looking RSVP statuses. Can be added later if needed.

## File Structure

- **Create** `src/models/meeting.rs` — `Meeting` and `MeetingAttendee` structs.
- **Modify** `src/models/mod.rs` — register `meeting`.
- **Modify** `src/models/enums.rs` — add `AttendeeStatus`.
- **Modify** `src/db/schema.rs` — migration v10 (two tables + seeded kind), bump
  `CURRENT_VERSION` to 10.
- **Modify** `src/db/queries.rs` — `MEETING_RSVP_KIND` constant; meeting CRUD; attendee
  matrix fetch; `set_attendee_status` (with activity logging) and `remove_attendee`.
- **Modify** `src/db/mod.rs` — `DbConnection` passthroughs for the new queries.
- **Create** `src/ui/meetings.rs` — the matrix page (`render`), the contact-type filter
  enum (`MeetingWhoFilter`), and the cell popup. Row filtering reuses the existing shared
  `app.search` box (as the contact lists and activity history do).
- **Create** `src/ui/meeting_form.rs` — the `MeetingForm` state struct + the add/edit
  meeting modal render (mirrors how `forms.rs` pairs `ContactForm` with its modal).
- **Modify** `src/ui/mod.rs` — register `meetings` + `meeting_form`; add `View::Meetings`.
- **Modify** `src/ui/confirm.rs` — add a `Meeting` delete variant.
- **Modify** `src/app.rs` — `AppState` fields: `meeting_form`, `meeting_who_filter`,
  `meeting_show_past`, and a pending cell-action; the row search reuses the existing
  `app.search`. Add the `View::Meetings` sidebar item + central-panel dispatch, and render
  the meeting-form modal alongside the other modals.

## Testing

Follow the existing rusqlite unit-test style (in-memory DB, like the advance tests):

- Migration v10 creates both tables and seeds `MEETING_RSVP_KIND`.
- Meeting CRUD round-trips (create, list, update, delete).
- `set_attendee_status` upserts on the `(meeting_id, contact_id)` unique key (second call
  for the same pair updates, not duplicates).
- Setting status to `Attending` from a non-attending state logs exactly one activity;
  re-setting `Attending` again logs none; other transitions and `paid` toggles log none.
- `remove_attendee` deletes the row (cell back to empty).
- Deleting a meeting cascades its attendee rows; deleting a contact cascades that
  contact's attendee rows.
- The "unfinished only" column filter excludes meetings whose `end_date < today` and
  includes them when past meetings are requested.
```
