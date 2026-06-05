# Meetings & Attendance Tracking — Design

**Date:** 2026-06-05
**Status:** Approved (design)

## Goal

Add a **"งานประชุม" (Meetings)** menu that lets the user set up meetings/events and
track which contacts (ผู้มุ่งหวัง / ลูกค้า VIP / นักธุรกิจ) they are following to
attend each one, recording per-person **RSVP status**, **entry-fee payment**, and
**actual post-event attendance** in a single matrix view.

## Overview / Architecture

A new sidebar entry **"🎟 งานประชุม"** (placed right after "✅ ติดตามผล") opens one
page built around a **matrix table**:

- **Rows (Y):** contacts — all three types, filterable by search text and contact type.
- **Columns (X):** meetings — by default only meetings that have not finished
  (`end_date >= today`), with a toggle to also show past meetings.
- **Cell (row × column):** that contact's RSVP status for that meeting, whether they have
  paid the entry fee, and (after the event) whether they actually showed up. An empty cell
  means "not yet followed / not invited".

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
| `attended`   | INTEGER | NULLABLE — actual attendance: NULL = not recorded, 1 = came, 0 = no-show |
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

`from_db` falls back to `Undecided` for forward compatibility. Actual attendance
(`attended`) is a nullable boolean rather than an enum: NULL = ยังไม่บันทึก, true = มาจริง,
false = ไม่มา.

### Seeded activity kinds

Migration v10 seeds two new activity kinds into `activity_kinds` (same mechanism as the
advance-collected kind), exposed as constants in `db/queries.rs`:

- `MEETING_RSVP_KIND` = **"ตอบรับเข้างานประชุม"** — logged when a person is set to
  "จะเข้าร่วม".
- `MEETING_ATTENDED_KIND` = **"เข้าร่วมงานประชุม"** — logged when a person is recorded as
  "มาจริง".

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
- Each cell renders, left to right:
  - the RSVP marker — 🟢 จะเข้าร่วม · ⚪ รอตัดสินใจ · 🔴 ไม่เข้า · (blank) = not invited;
  - `💵` when `paid = 1` (hidden for free meetings, `fee = 0`);
  - the actual-attendance marker, **only when `attended` is recorded** — ✔ (green) = มาจริง,
    ✘ (red) = ไม่มา. Not shown while `attended` is NULL, so unfinished-meeting columns stay
    clean. (Exact glyphs are verified against egui's bundled font subset during
    implementation; equivalent fallbacks are used if any is missing.)
  - Example: `🟢 💵 ✔` = said attending, paid, actually came; `🟢 ✘` = said attending but
    did not show; `⚪` = undecided, nothing else recorded.
- Clicking a cell opens a small popup:
  - **สถานะตอบรับ:** จะเข้าร่วม / รอตัดสินใจ / ไม่เข้า / **เอาออกจากงาน**;
  - `☐ จ่ายค่าเข้างานแล้ว` checkbox (hidden when `fee = 0`);
  - **ผลหลังงาน:** มาจริง / ไม่มา / ล้าง (sets `attended` to true / false / NULL).
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

- **Cell writes:** the popup edits the cell's full state `{status, paid, attended}` and the
  app applies it after the table via a single `upsert_attendee` (or `remove_attendee` when
  "เอาออกจากงาน" / status cleared). The upsert reads the prior row, writes the new values,
  and within the same transaction logs activities for the two milestone transitions:
  - status becomes `Attending` and the prior state was not Attending → log one
    `MEETING_RSVP_KIND` activity, note `"ตอบรับเข้าร่วม: ‹meeting name›"`;
  - `attended` becomes `true` and the prior value was not true → log one
    `MEETING_ATTENDED_KIND` activity, note `"เข้าร่วมงานจริง: ‹meeting name›"`.

  (mirrors `collect_advance`, which writes a row and logs an activity in one transaction).
  Other transitions — undecided/not-attending, toggling `paid`, recording `ไม่มา`, or
  clearing — log nothing.
- **Recording attendance on an untracked person:** if a cell has no row yet and the user
  records มาจริง/ไม่มา directly, the row is created with status `Undecided` (we never got an
  RSVP) and the actual value set — covering walk-ins. Recording attendance is available for
  any column but is intended for meetings that have occurred (reached via the
  "แสดงงานที่ผ่านมาแล้ว" toggle); it is not date-gated.
- **Empty states:** if no meetings match the current column filter, the page shows a
  prompt to add one instead of an empty grid. If there are no contacts at all, it shows a
  "no contacts yet" note.
- **Free meetings:** `fee = 0` hides all payment UI for that column.

## File Structure

- **Create** `src/models/meeting.rs` — `Meeting` and `MeetingAttendee` structs
  (`MeetingAttendee` carries `status: AttendeeStatus`, `paid: bool`, `attended: Option<bool>`).
- **Modify** `src/models/mod.rs` — register `meeting`.
- **Modify** `src/models/enums.rs` — add `AttendeeStatus`.
- **Modify** `src/db/schema.rs` — migration v10 (two tables + two seeded kinds), bump
  `CURRENT_VERSION` to 10.
- **Modify** `src/db/queries.rs` — `MEETING_RSVP_KIND` + `MEETING_ATTENDED_KIND` constants;
  meeting CRUD; attendee matrix fetch (rows for the shown meetings keyed by
  `(meeting_id, contact_id)`); `upsert_attendee` (writes status + paid + attended, logging
  the two milestone transitions) and `remove_attendee`.
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

- Migration v10 creates both tables and seeds `MEETING_RSVP_KIND` + `MEETING_ATTENDED_KIND`.
- Meeting CRUD round-trips (create, list, update, delete).
- `upsert_attendee` inserts on first call and updates on the second for the same
  `(meeting_id, contact_id)` pair (no duplicate row); `status`, `paid`, and `attended`
  all round-trip, including `attended = NULL`.
- Setting status to `Attending` from a non-attending state logs exactly one RSVP activity;
  re-setting `Attending` again logs none; undecided/not-attending and `paid` toggles log none.
- Recording `attended = true` from a non-true state logs exactly one attended activity;
  re-recording `true` logs none; recording `false`/clearing logs none.
- Recording attendance on a `(meeting, contact)` with no prior row creates it with status
  `Undecided`.
- `remove_attendee` deletes the row (cell back to empty).
- Deleting a meeting cascades its attendee rows; deleting a contact cascades that
  contact's attendee rows.
- The "unfinished only" column filter excludes meetings whose `end_date < today` and
  includes them when past meetings are requested.
