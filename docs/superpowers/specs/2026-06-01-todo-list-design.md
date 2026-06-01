# Todo List — Design Spec

**Date:** 2026-06-01
**Feature:** A "สิ่งที่ต้องทำ / Todo List" menu with full CRUD. Each todo records what
needs to be done, optionally for a specific contact (ผู้มุ่งหวัง / ลูกค้า VIP / นักธุรกิจ),
with a due date and a done/not-done state.

## Goal

Let the user track follow-up tasks tied to people in their network. Open the app,
see overdue / pending work, add a task ("โทรนัดดูสินค้า Nutrilite") for a chosen
contact with a due date, mark it done when finished.

## Decisions (from brainstorming)

1. **What to do** — free text (single field), not a fixed type.
2. **Status** — `done` / not-done checkbox only; "overdue" is derived automatically
   (`due_date < today AND NOT done`), highlighted in red. No separate in-progress state.
3. **Contact link** — optional. Most todos target one contact, but a todo may have
   none ("ไม่ระบุ"). Deleting a contact keeps its todos (they become unassigned).
4. **List view** — sorted by due date; filter by status (default "ยังไม่เสร็จ") and by
   contact type; text search.
5. **Dashboard** — add an "เลยกำหนด / Overdue" count card that navigates to the Todo
   view (overdue filter) when clicked.
6. **Due date input** — use `egui_extras` `datepicker` feature (calendar popup).
7. **Due date required?** — optional; the add form defaults it to today, and it can be
   cleared to "ไม่มีกำหนด".

## Architecture

Follows the existing layering exactly (mirrors the `activities` / `activity_kinds`
feature): `models` → `db::schema` (migration) → `db::queries` (typed SQL + in-memory
tests) → `db::mod` (`DbConnection` pass-throughs) → `ui::todo` (the `render(app, ui)`
view) → wired into `app.rs` (`View` enum, sidebar, state) and the shared confirm modal.

A standalone `todos` table is used (not an extension of `activities`) to keep the
immutable past-interaction log separate from forward-looking tasks.

## 1. Data model & schema

### Migration (schema v7)

Bump `CURRENT_VERSION` 6 → 7 in `src/db/schema.rs` and add:

```sql
CREATE TABLE IF NOT EXISTS todos (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
    task       TEXT    NOT NULL,
    due_date   TEXT,                       -- 'YYYY-MM-DD' or NULL
    done       INTEGER NOT NULL DEFAULT 0,
    created_at TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_todos_contact ON todos(contact_id);
CREATE INDEX IF NOT EXISTS idx_todos_due     ON todos(due_date);
```

`ON DELETE SET NULL` implements decision #3 (todos survive contact deletion). Note the
schema relies on `PRAGMA foreign_keys = ON` (already set per-connection in `db::init`)
for the SET NULL to fire.

### Model (`src/models/todo.rs`, registered in `src/models/mod.rs`)

```rust
pub struct Todo {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub task: String,
    pub due_date: Option<NaiveDate>,   // chrono::NaiveDate
    pub done: bool,
    pub created_at: DateTime<Local>,
}
```

- `due_date` stored as `'%Y-%m-%d'` text (consistent with sponsor-flow step dates),
  parsed via `NaiveDate::parse_from_str`.
- `created_at` stored as RFC3339 (consistent with `activities`).
- An `is_overdue(today: NaiveDate)` helper: `due_date.map_or(false, |d| d < today) && !done`.

## 2. DB layer

### `src/db/queries.rs` (with in-memory unit tests)

A joined row type for the list view:

```rust
pub struct TodoRow {
    pub todo: Todo,
    pub contact_name: Option<String>,   // resolved display name (name + nickname), None if unassigned
    pub contact_type: Option<ContactType>,
}
```

Functions (each takes `&Connection`, returns `Result<…>`):

- `add_todo(conn, contact_id: Option<i64>, task: &str, due_date: Option<NaiveDate>) -> Result<i64>`
  — trims `task`, rejects empty with `AppError::validation("กรุณากรอกสิ่งที่ต้องทำ")`.
- `update_todo(conn, todo: &Todo) -> Result<()>` — updates `contact_id`, `task`, `due_date`
  (same empty-task validation). Does not change `done` or `created_at`.
- `set_todo_done(conn, id: i64, done: bool) -> Result<()>` — toggles the checkbox.
- `delete_todo(conn, id: i64) -> Result<()>`.
- `list_todos(conn, query: &str) -> Result<Vec<TodoRow>>` — `LEFT JOIN contacts`, filters
  by `task LIKE %q% OR contact name/nickname LIKE %q%`, orders
  `done ASC, due_date IS NULL ASC, due_date ASC, id DESC` (pending first; within that,
  soonest due first; no-due-date last; newest tiebreak). Status and contact-type filtering
  happen in the UI (mirrors how `activities.rs` filters `kind` in memory).
- `count_overdue_todos(conn) -> Result<i64>` — `done = 0 AND due_date IS NOT NULL AND due_date < :today`.
- `count_due_soon_todos(conn, days: i64) -> Result<i64>` — `done = 0 AND due_date BETWEEN :today AND :today+days`
  (reserved for a possible second card; the dashboard ships the overdue card first).

`today` is computed as `Local::now().date_naive()` formatted `'%Y-%m-%d'` and compared as
text (ISO dates sort/compare correctly as strings).

### `src/db/mod.rs`

Thin pass-through methods on `DbConnection` for each of the above, in a new
`// --- todos ---` section.

### Tests (in the `queries.rs` `#[cfg(test)]` module)

- `todo_add_list_update_delete` — round-trip CRUD; empty task rejected.
- `todo_list_orders_pending_then_due_date` — ordering: pending before done, soonest due
  first, NULL due last.
- `todo_done_toggle_persists`.
- `todo_contact_set_null_on_delete` — create todo for a contact, delete the contact,
  assert the todo remains with `contact_id = NULL` (verifies the FK + `display_name`/type
  resolve to `None`).
- `overdue_and_due_soon_counts` — seed dates around "today" and assert the counts.

## 3. UI view — `src/ui/todo.rs`

Registered in `src/ui/mod.rs` (`pub mod todo;`). Layout mirrors `activity_kinds.rs`
(inline add/edit form + `TableBuilder`, deferred actions applied after render).

### Form state — `TodoForm` (held on `AppState`)

```rust
pub struct TodoForm {
    pub editing_id: Option<i64>,   // Some = edit mode, None = add mode
    pub task: String,
    pub contact_id: Option<i64>,   // None = "ไม่ระบุ"
    pub contact_filter: String,    // search text for filter_combo
    pub due_date: Option<NaiveDate>,
}
```
Default: empty task, no contact, `due_date = Some(today)` (decision #7). `editing_id = None`.

### Inline form (top of page)

A wrapped horizontal row:
- "สิ่งที่ต้องทำ": `TextEdit::singleline` (wide).
- "เกี่ยวกับ": `filter_combo` over **all** contacts (`list_contacts`), each labelled
  `"<display_name> · <type label>"`, with `none_label = Some("— ไม่ระบุ —")`.
- "กำหนดส่ง": `egui_extras::DatePickerButton` bound to a `NaiveDate`, plus a small
  "ไม่มีกำหนด" toggle/✖ to clear it to `None` (DatePickerButton needs a concrete date,
  so the "no due date" state is tracked by the `Option` and a checkbox controls it).
- Buttons: add mode → "➕ เพิ่ม"; edit mode → "💾 บันทึก" + "ยกเลิก" (clears edit state).

### Filter row

- 🔍 text search → reuses `app.search` (same convention as the other list pages); "ล้าง" button.
- Status `ComboBox` → `TodoStatusFilter { Pending, Overdue, Done, All }` on `AppState`
  (`app.todo_status_filter`, default `Pending`). Labels: ยังไม่เสร็จ / เลยกำหนด / เสร็จแล้ว / ทั้งหมด.
- Contact-type `ComboBox` → `TodoWhoFilter { All, Type(ContactType), Unassigned }` on
  `AppState` (`app.todo_who_filter`, default `All`). Labels: ทั้งหมด / ผู้มุ่งหวัง / ลูกค้า VIP /
  นักธุรกิจ / ไม่ระบุ.

Rows from `list_todos(&app.search)` are filtered in memory by status (using `today`) and
by who, then a count line "ทั้งหมด N รายการ" is shown (matches `activities.rs`).

### Table columns

`[✓]  [กำหนดส่ง]  [สิ่งที่ต้องทำ]  [เกี่ยวกับ]  [จัดการ]`

- **✓** — `Checkbox` bound to `done`; on change → `set_todo_done` immediately (persist-on-toggle,
  like the follow-up sheet). Done rows render the task text weak/struck-through.
- **กำหนดส่ง** — `due_date` as `%Y-%m-%d`, or "—" if none. Overdue (pending & past) → red text.
- **สิ่งที่ต้องทำ** — the task text.
- **เกี่ยวกับ** — `contact_name` coloured by `contact_type` (same palette as
  `activities.rs`: Prospect amber, Customer green, ABO teal); "—" weak if unassigned.
- **จัดการ** — ✏ (load row into the inline form for edit) and 🗑 (→ `PendingDelete::Todo`).

### Deferred actions

Collected during render, applied after (matches the codebase pattern): `add`, `save`,
`cancel_edit`, `edit_req(id)`, `delete_req(id, label)`, `toggle(id, done)`. The edit-load
reads the existing `TodoRow` to populate `TodoForm`.

## 4. App wiring — `src/app.rs` & `src/ui/mod.rs`

- `View` enum: add `Todos`. Dispatch `View::Todos => ui::todo::render(self, ui)` in the
  central panel match.
- Sidebar: add `(View::Todos, "🗒️  สิ่งที่ต้องทำ")` after the "ติดตามผล" entry.
- `AppState` new fields (with initialisers in `AppState::new`): `todo_form: TodoForm`,
  `todo_status_filter: TodoStatusFilter` (= `Pending`), `todo_who_filter: TodoWhoFilter`
  (= `All`).
- `src/ui/confirm.rs`: add `PendingDelete::Todo { id, name }` variant; its delete branch
  calls `app.db.delete_todo(id)`; warning detail e.g. "งานนี้จะถูกลบถาวร".

## 5. Dashboard card — `src/ui/dashboard.rs` & `src/ui/mod.rs`

- Add `metric_card_clickable(ui, title, value, accent) -> egui::Response` to `ui/mod.rs`
  (a clickable sibling of `metric_card`; existing `metric_card` and its 4 call sites are
  unchanged).
- In `dashboard.rs`, fetch `count_overdue_todos()` and render a red card
  ("เลยกำหนด / Overdue"). On click: `app.view = View::Todos; app.todo_status_filter = Overdue;`.
- If overdue count is 0, the card still shows "0" (consistent with the other count cards).

## 6. Dependency change — `Cargo.toml`

Enable the datepicker feature:

```toml
egui_extras = { version = "0.28", features = ["datepicker"] }
```

`chrono` is already a dependency, so this only turns on the existing widget.

## Out of scope (YAGNI)

- Recurring / repeating todos.
- Reminders / notifications outside the app.
- Sub-tasks, priorities, tags.
- A "due soon" dashboard card (the query is provided but only the overdue card ships).
- Editing `created_at` or a separate `done_at` timestamp.

## Testing & verification

- `cargo test` — new `queries.rs` todo tests pass; existing tests still pass.
- `cargo build` — compiles with the new `datepicker` feature.
- Manual smoke (per the existing app flow): add a todo for a prospect with a due date in
  the past → appears red under "เลยกำหนด" and the dashboard overdue card increments;
  mark done → moves out of the pending filter; delete a linked contact → todo remains as
  "ไม่ระบุ"; edit a todo's task/contact/due date round-trips.
