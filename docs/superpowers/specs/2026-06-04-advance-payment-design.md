# Advance Payments (สำรองจ่าย) — Design Spec

**Date:** 2026-06-04
**Status:** Approved (brainstorming complete)

## Goal

เพิ่มเมนู **"สำรองจ่าย" (Advance Payments)** เพื่อบันทึกการที่ผู้ใช้ (ABO) จ่ายเงิน
ล่วงหน้าซื้อสินค้าให้รายชื่อหนึ่ง (ลูกค้า / ผู้มุ่งหวัง / นักธุรกิจ) แล้วรอเก็บเงินคืนภายหลัง
เมื่อทำเครื่องหมาย "เก็บเงินแล้ว" ระบบจะบันทึกเหตุการณ์นั้นลง **ประวัติการติดต่อ**
ของรายชื่อนั้นโดยอัตโนมัติ

## Architecture

ฟีเจอร์นี้สร้างตามแพตเทิร์นของเมนู **Todo** (`todos` table → `complete_todo`) ที่เพิ่งทำเสร็จ
และรีวิวผ่านแล้ว: ตารางใหม่ตารางเดียว, query layer แบบ typed + `#[cfg(test)]`,
DbConnection passthrough, view + modal ใน `src/ui/`. การ "เก็บเงิน" ใช้ transaction
เดียวที่ทั้ง `UPDATE` สถานะและ `INSERT` activity — เหมือน `complete_todo` ทุกประการ.

**Tech stack:** Rust, egui/eframe 0.28, rusqlite 0.31 (bundled), chrono 0.4.

## Decisions (จาก brainstorming)

1. **Contact linkage** — บังคับเลือกรายชื่อตอนสร้าง, เลือกได้ทุกประเภท (Prospect/Customer/Abo).
2. **Fields** — รายการสินค้า (item), จำนวนเงิน (amount, จำนวนเต็มบาท), วันที่จ่ายล่วงหน้า
   (advance_date, ดีฟอลต์ = วันนี้), หมายเหตุ (note, ไม่บังคับ).
3. **Statuses** — สองสถานะ: `รอเก็บเงิน` → `เก็บเงินแล้ว` (เก็บครั้งเดียวจบ ไม่มี partial).
4. **Collect action** — เปิดกล่อง dialog ให้กรอก *วันที่เก็บจริง* (ดีฟอลต์วันนี้) + *หมายเหตุ
   (ไม่บังคับ)* แล้วกดบันทึก จึงค่อยลงประวัติ.
5. **Activity kind** — ชนิดใหม่ `"เก็บเงินค่าสินค้า (สำรองจ่าย)"` seed ผ่าน migration;
   รายละเอียด = `"<item> — <amount> บาท"` ต่อท้ายด้วย `" — <note>"` เมื่อมีหมายเหตุ.
6. **Menu** — `"💵 สำรองจ่าย"` วางถัดจาก `"📅 สิ่งที่ต้องทำ"` (ก่อน `"🌳 เครือข่าย"`).
7. **Data model** — ตาราง `advances` ตารางเดียว (Approach A).
8. **contact_id nullable + `ON DELETE SET NULL`** — บังคับใน UI ตอนสร้าง แต่ถ้ารายชื่อถูกลบ
   ภายหลัง รายการสำรองจ่ายจะไม่หาย (กลายเป็น "ไม่มีรายชื่อ") — ป้องกันการสูญหายของ
   ข้อมูลเงินค้าง.
9. **Activity timestamp = วันที่เก็บที่กรอก** (ไม่ใช่เวลาที่กดปุ่ม) เพื่อให้รายการไปอยู่ถูกวันใน
   timeline ของประวัติ.
10. **ไม่มี un-collect** — ถ้าทำเครื่องหมายเก็บผิด ให้ลบรายการทิ้ง (และลบ activity ที่หน้า
    ประวัติได้) เพื่อเลี่ยงสถานะกำกวม.

## Data model

### Migration v9 (`src/db/schema.rs`)

Bump `CURRENT_VERSION` 8 → 9. Add:

```sql
CREATE TABLE IF NOT EXISTS advances (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id     INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
    item           TEXT    NOT NULL,
    amount         INTEGER NOT NULL,
    advance_date   TEXT    NOT NULL,           -- YYYY-MM-DD
    note           TEXT    NOT NULL DEFAULT '', -- optional note entered at creation
    collected      INTEGER NOT NULL DEFAULT 0,
    collected_at   TEXT,                        -- YYYY-MM-DD, NULL while outstanding
    collected_note TEXT,                        -- optional note entered at collection
    created_at     TEXT    NOT NULL             -- rfc3339
);
CREATE INDEX IF NOT EXISTS idx_advances_contact ON advances(contact_id);
```

Same migration block also seeds the activity kind:

```sql
INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1) -- ADVANCE_COLLECTED_KIND
```

(`activity_kinds` exists since v5; `params!` already imported in schema.rs since v8.)

### Model (`src/models/advance.rs`, new)

```rust
pub struct Advance {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub item: String,
    pub amount: i64,
    pub advance_date: NaiveDate,
    pub note: String,
    pub collected: bool,
    pub collected_at: Option<NaiveDate>,
    pub collected_note: Option<String>,
    pub created_at: DateTime<Local>,
}
```

Register `pub mod advance;` in `src/models/mod.rs`.

**Two distinct notes:** `note` is the optional remark captured when the advance is *created*
(e.g. "รับของที่ร้านแล้ว"); `collected_note` is the optional remark captured in the *collect
dialog* (e.g. "โอนผ่านพร้อมเพย์"). Only `collected_note` is appended to the logged activity
(Decision 5); `note` is stored on the record and shown in the list (hover on the item cell).

## Query layer (`src/db/queries.rs`)

- `pub const ADVANCE_COLLECTED_KIND: &str = "เก็บเงินค่าสินค้า (สำรองจ่าย)";`
- `pub fn group_thousands(n: i64) -> String` — format an integer with comma thousands
  separators (e.g. `1740 → "1,740"`, negative handled). Used by `advance_note` and the
  table's amount column.
- `pub fn advance_note(item: &str, amount: i64, note: &str) -> String` — returns
  `"<item> — <grouped amount> บาท"`, plus `" — <note>"` when `note.trim()` is non-empty.
  (Mirror of `done_note`.)
- `pub struct AdvanceRow { pub advance: Advance, pub contact_name: Option<String>,
  pub contact_type: Option<ContactType> }` — advance joined with its contact, for the list.
- `pub fn add_advance(conn, contact_id: Option<i64>, item: &str, amount: i64,
  advance_date: NaiveDate, note: &str) -> Result<i64>` — trims/validates `item` non-empty
  ("กรุณากรอกรายการสินค้า") and `amount > 0` ("จำนวนเงินต้องมากกว่า 0"); stores the trimmed
  create-time `note`; inserts with `collected = 0`, `created_at = now`.
- `pub fn update_advance(conn, a: &Advance) -> Result<()>` — updates only
  `contact_id / item / amount / advance_date / note` (same validation); never touches the
  collected fields or `created_at`.
- `pub fn collect_advance(conn, id: i64, collected_date: NaiveDate, note: &str)
  -> Result<()>` — in one `unchecked_transaction`: `UPDATE advances SET collected = 1,
  collected_at = ?, collected_note = ? WHERE id = ?`; then read back
  `(contact_id, item, amount)`; if `contact_id` is `Some`, `INSERT INTO activities
  (contact_id, kind, note, created_at)` with `kind = ADVANCE_COLLECTED_KIND`,
  `note = advance_note(item, amount, note)`, and `created_at` derived from
  `collected_date` (at the current local time-of-day) so it lands on the right day.
  A contactless advance is still marked collected, with no activity (no panic).
- `pub fn delete_advance(conn, id: i64) -> Result<()>`.
- `pub fn list_advances(conn, query: &str, collected_filter: Option<bool>)
  -> Result<Vec<AdvanceRow>>` — `LEFT JOIN contacts`; filter by substring of `item`
  or contact name/nickname, and by `collected` when `collected_filter` is `Some`.
  Order: outstanding first, then oldest `advance_date` first, `id DESC` tiebreak
  (`ORDER BY collected ASC, advance_date ASC, id DESC`).
- `pub fn outstanding_total(conn) -> Result<i64>` — `SELECT IFNULL(SUM(amount),0)
  FROM advances WHERE collected = 0`.

## DbConnection passthroughs (`src/db/mod.rs`)

Thin delegations: `add_advance`, `update_advance`, `collect_advance`, `delete_advance`,
`list_advances`, `outstanding_total`.

## UI

### Navigation
- Add `Advances` to the `View` enum (`src/ui/mod.rs`).
- Sidebar `items` (`src/app.rs`): insert `(View::Advances, "💵  สำรองจ่าย")` right after
  `(View::Todos, …)`.
- `update()` dispatch: `View::Advances => ui::advances::render(self, ui)`.

> **Glyph check:** verify `💵` exists in egui's bundled font subset; if it renders as tofu,
> swap for a confirmed-working glyph (per the known egui emoji-subset limitation).

### List page (`src/ui/advances.rs`, new) — mirrors `todo.rs`
- `pub struct AdvanceForm { editing_id: Option<i64>, contact_id: Option<i64>,
  contact_filter: String, item: String, amount: i64, advance_date: NaiveDate,
  note: String }` (default: `advance_date = today`, `amount = 0`), stored on `AppState`.
- `enum AdvanceStatusFilter { Outstanding, Collected, All }` with Thai labels.
- Two-card layout via `ui.columns(2, …)` and the `field_row` helper (copied/shared from
  todo.rs's local helper): **left** = add/edit form (รายชื่อ via `filter_combo`, **required**;
  item; amount via `DragValue` ≥ 0; advance_date via `DatePickerButton`; note); **right** =
  search box + status filter (`ComboBox`).
  - On submit (add): if `contact_id` is `None`, show error "กรุณาเลือกรายชื่อ" and do not insert.
- **Summary line** above the table: `"ยอดรอเก็บรวม: {grouped} บาท • รอเก็บ {n} รายการ"`
  (from `outstanding_total` + count of outstanding rows).
- **Table** columns: วันที่จ่าย | ชื่อ | รายการสินค้า | จำนวนเงิน | สถานะ | จัดการ.
  - สถานะ: `🟠 รอเก็บเงิน`, or `✅ เก็บแล้ว (collected_at)`.
  - จัดการ for **outstanding**: `💵 เก็บเงิน` (opens collect dialog) + `✏` edit + `🗑` delete.
    For **collected**: `🗑` delete only (no edit).
  - Amount shown via `group_thousands`. The item cell shows `on_hover_text` with the
    create-time `note` when present.
  - Deferred-action pattern (collect `Option`, edit_req, delete_req) exactly like todo.rs.

### Collect dialog (`src/ui/advance_collect.rs`, new) — mirrors `todo_done.rs`
- `pub struct PendingAdvanceCollect { id: i64, item: String, amount: i64,
  contact_name: String }` (`#[derive(Clone)]`).
- `AppState` fields: `pending_advance_collect: Option<PendingAdvanceCollect>`,
  `advance_collect_date: NaiveDate`, `advance_collect_note: String`.
- `render(app, ctx)`: window "เก็บเงินค่าสินค้า / Collect Payment" showing item + amount +
  contact; a `DatePickerButton` (default today) for วันที่เก็บ; a multiline note
  (hint "หมายเหตุ (ไม่บังคับ)"); buttons `💾 บันทึก` (`fill(ACCENT)`) + `ยกเลิก`.
- On save → `app.db.collect_advance(id, date, note)`. **On `Ok`** set a status message and
  clear the pending state + inputs; **on `Err`** keep the dialog open with inputs preserved
  (same fix applied to `todo_done.rs`). Cancel/close → clear pending state (no DB change).
- Dispatch `ui::advance_collect::render(self, ctx)` in `update()` after
  `ui::todo_done::render`.

### Delete confirmation (`src/ui/confirm.rs`)
- Add `PendingDelete::Advance { id: i64, item: String }`; wire its confirm arm to
  `app.db.delete_advance(id)` with a status message, following the existing variants.

## Error handling

Validation lives in the query layer (`add_advance` / `update_advance`); errors surface in the
status bar via `app.set_error` / `app.handle` (existing pattern). `collect_advance` runs in a
single atomic transaction. Deleting a contact nulls `contact_id` (SET NULL); collecting such an
orphaned advance still succeeds and simply logs no activity.

## Testing (`#[cfg(test)]` in `queries.rs`, reusing `mem()` / `insert_contact` / `sample_prospect`)

- `migration_seeds_advance_collected_kind` — after `init`, `list_activity_kinds` contains
  `ADVANCE_COLLECTED_KIND`.
- `group_thousands_formats_with_commas` — `0`, `740`, `1740`, `1234567` → expected strings.
- `advance_note_formats_item_amount_and_note` — with and without a note; amount grouped.
- `add_advance_validates_item_and_amount` — empty item and `amount <= 0` are rejected.
- `add_then_list_round_trips_fields` — a saved advance reads back via `list_advances` with the
  same item, amount, advance_date, and create-time note.
- `collect_advance_logs_activity_for_contact` — outstanding → collected; one activity inserted
  with the right kind and note; `collected`/`collected_at` set.
- `collect_advance_without_contact_does_not_log` — `contact_id` NULL → marked collected, no
  activity row.
- `list_advances_filters_and_orders` — outstanding-first then oldest-first ordering; status
  filter (`Some(false)` / `Some(true)` / `None`); substring search on item and contact name.
- `outstanding_total_sums_uncollected` — sum excludes collected rows; `0` when none.

## Files touched

| File | Change |
|---|---|
| `src/db/schema.rs` | migration v9: `advances` table + seed kind; bump `CURRENT_VERSION` |
| `src/db/queries.rs` | const, `group_thousands`, `advance_note`, `AdvanceRow`, CRUD + `collect_advance` + `outstanding_total` + tests |
| `src/db/mod.rs` | passthroughs |
| `src/models/advance.rs` | new — `Advance` struct |
| `src/models/mod.rs` | `pub mod advance;` |
| `src/ui/advances.rs` | new — list page + `AdvanceForm` + `AdvanceStatusFilter` |
| `src/ui/advance_collect.rs` | new — collect dialog + `PendingAdvanceCollect` |
| `src/ui/confirm.rs` | `PendingDelete::Advance` variant |
| `src/ui/mod.rs` | `View::Advances` + `pub mod advances; pub mod advance_collect;` |
| `src/app.rs` | sidebar item, view dispatch, `AppState` fields + initializers, modal dispatch |

## YAGNI — explicitly out of scope

Partial / multiple collections · a "cancelled" status · a collection due-date with overdue
highlighting · editing collected records · un-collect/revert · a product catalog (item is free
text) · dashboard card (page-header summary only).
