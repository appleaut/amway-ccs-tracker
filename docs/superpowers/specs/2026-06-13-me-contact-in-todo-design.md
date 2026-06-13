# Add "ฉัน (Me)" to the Todo "เกี่ยวกับ" Field — Design

**Date:** 2026-06-13
**Status:** Approved

## Problem

On the Todo List page, a task's "เกี่ยวกับ" (about) field offers every contact plus a
"— ไม่ระบุ —" (none) option. There is no way to make a task about **myself** and, when such a
task is completed, record the result into my own activity history.

"Me" already exists conceptually across the app (the network-chart centre node, the "My Rank"
advisor) but has **no row** in the `contacts` table — it is the implicit network root. Because
`activities.contact_id` is `NOT NULL REFERENCES contacts(id)`, logging "my" activity requires a
real contact row to log against.

## Decisions (locked)

- **Storage = A:** Add a hidden "me" contact row, flagged by a new `is_me` column. It carries my
  activities and can be the target of a todo, while staying hidden from every existing list.
- **Completion behaviour = B (record history):** A todo about me logs its result to my history on
  completion — handled automatically by the existing linked-todo path (no special logic).
- **Where to view = B (existing page):** My activities appear in the existing Activity History
  page, with a new "เฉพาะของฉัน" (only mine) filter toggle.

## Constraint / Risk

A me-row with `contact_type = 'ABO'` would leak into every query that lists or counts contacts by
type — most dangerously the **network chart** (`list_abos` → a duplicate "me" node) and
**`me_leg_counts`** (`sponsor_id IS NULL AND contact_type='ABO'` would count the me-row itself as a
downline leg, corrupting the rank assessment). The design therefore **hides the me-row everywhere
by default** (adds an `is_me = 0` guard to the typed list/count queries) and **reveals it only**
in the Todo "เกี่ยวกับ" picker. This is the inverse of a naive approach and is far safer.

## Design

### 1. Database layer (`src/db/schema.rs`, `src/db/queries.rs`, `src/db/mod.rs`)

**Migration v12** (`schema.rs`) — bump `CURRENT_VERSION` to `12`, add the column, seed the row.
The seed uses `INSERT ... SELECT ... WHERE NOT EXISTS` so it never creates a duplicate (covers
fresh install, upgrade-in-place, and restore of a pre-v12 backup — `migrate()` runs on every open).
`schema.rs` must add `use chrono::Local;`.

```rust
if version < 12 {
    // A hidden "me" contact row carries my own activities and can be a todo's
    // target. It is excluded from every typed list/count (see queries.rs guards)
    // and revealed only in the Todo "เกี่ยวกับ" picker. is_me marks it.
    conn.execute_batch("ALTER TABLE contacts ADD COLUMN is_me INTEGER NOT NULL DEFAULT 0;")?;
    conn.execute(
        "INSERT INTO contacts (name, nickname, gender, network_category, contact_type, is_me, created_at)
         SELECT 'ฉัน', 'Me', 'Male', 'Family', 'ABO', 1, ?1
         WHERE NOT EXISTS (SELECT 1 FROM contacts WHERE is_me = 1)",
        params![Local::now().to_rfc3339()],
    )?;
}
```

- The `Contact` column list constant `C` and `row_to_contact` are **unchanged** — `is_me` is never
  selected into a `Contact`, so column indices in the joined row queries (`row.get(17)`, etc.) do
  not shift.

**Guard the me-row out of every typed list/count** in `queries.rs` (add `is_me = 0`):

| Function | Edit |
|---|---|
| `list_contacts` | `... FROM contacts c WHERE c.is_me = 0 ORDER BY c.name` |
| `list_by_type` | `... WHERE c.contact_type = ?1 AND c.is_me = 0 ORDER BY c.name` |
| `me_leg_counts` | `... WHERE sponsor_id IS NULL AND contact_type = 'ABO' AND is_me = 0` |
| `list_prospect_rows` | add `AND c.is_me = 0` to its `WHERE c.contact_type = 'Prospect'` |
| `list_customer_rows` | add `AND c.is_me = 0` to its `WHERE c.contact_type = 'Customer'` |
| `list_abo_rows` | add `AND c.is_me = 0` to its `WHERE c.contact_type = 'ABO'` |
| `count_by_type` | `... WHERE contact_type = ?1 AND is_me = 0` |
| `count_conversions_this_month` | add `AND is_me = 0` to its `WHERE contact_type IN (...)` |

> `abo_leg_counts` (`WHERE sponsor_id = ?1`) and the upline `LEFT JOIN ... ON up.id = c.sponsor_id`
> need no guard: the me-row has a NULL `sponsor_id`, so it never matches a real ABO id and is never
> anyone's resolved upline.

**New query + passthrough** — fetch the me-row's id, used to prepend the picker option and to
filter the "mine" view:

```rust
// queries.rs
pub fn me_contact_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT id FROM contacts WHERE is_me = 1 LIMIT 1",
        [],
        |r| r.get(0),
    )?)
}
```

```rust
// db/mod.rs
pub fn me_contact_id(&self) -> Result<i64> {
    queries::me_contact_id(&self.conn)
}
```

**Unit tests** (`queries.rs` `#[cfg(test)]`):
- `me_contact_id_returns_seeded_row`: a fresh `mem()` DB (migrations run) has a me-row;
  `me_contact_id()` returns its id, and `get_contact(id)` shows `name == "ฉัน"`.
- `me_row_hidden_from_lists`: after migrations, `list_contacts()` is empty (no contacts added) and
  `count_by_type(Abo) == 0`, i.e. the me-row does not appear despite `contact_type = 'Abo'`.
- `me_leg_counts_excludes_me`: with no real downline ABOs, `me_leg_counts() == (0, 0, 0)` (the
  me-row, which has `sponsor_id IS NULL AND contact_type = 'ABO'`, is not counted as a leg).

### 2. Todo "เกี่ยวกับ" picker + completion (`src/ui/todo.rs`, `src/ui/todo_done.rs`)

**Picker (`todo.rs`):** `list_contacts` now hides me, so fetch the me id separately and prepend it
as the first option. `filter_combo` derives its displayed text from the selected id, so a todo
whose `contact_id == me_id` shows "ฉัน (Me)" both when adding and when editing.

```rust
let me_id = app.db.me_contact_id().ok();
let contacts = app.db.list_contacts().unwrap_or_default(); // excludes me
let mut contact_options: Vec<(i64, String)> = contacts
    .iter()
    .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
    .collect();
if let Some(mid) = me_id {
    contact_options.insert(0, (mid, "ฉัน (Me)".to_string()));
}
```

**Completion — no new logic.** A todo about me has `contact_id = Some(me_id)`, so it flows through
the existing linked-todo branch: the Log Result dialog shows "ของ: ฉัน (Me)" read-only and
`complete_todo` logs the result to the me-row's history. `todo_done.rs`'s save/branch logic is
**unchanged**.

**`todo_done.rs` contactless picker:** for consistency (so a contactless todo's result can be
logged to my own history), prepend the same "ฉัน (Me)" option to that picker's `contact_options`.
This is the only change to `todo_done.rs` — fetch `me_contact_id()` and `insert(0, ...)` exactly as
above. The success-toast name lookup already searches the fetched `contacts`; since me is prepended
to `contact_options` but not present in `contacts`, the toast for a me-target falls back to the
generic "ทำเครื่องหมายเสร็จแล้ว" — acceptable (the result is still logged to my history). No further
change.

**Todo table "เกี่ยวกับ" column:** a me-todo resolves its name via the existing join and renders
"ฉัน" in the ABO colour. No change.

### 3. Activity History "เฉพาะของฉัน" filter (`src/ui/activities.rs`, `src/app.rs`)

My activities already appear in this page automatically (me is a contact; `list_all_activities`
joins contacts). Add a toggle to narrow the view to only mine.

**`app.rs`** — new field on `AppState`, initialised `false` in the constructor:

```rust
/// Activity History: when true, show only the me-row's activities.
pub history_mine_only: bool,
```

**`activities.rs`** — fetch the me id, add a checkbox after the existing kind dropdown, and filter
after the existing kind filter:

```rust
let me_id = app.db.me_contact_id().ok();
// ...in the filter row, after the kind popup:
ui.separator();
ui.checkbox(&mut app.history_mine_only, "เฉพาะของฉัน");

// ...after the existing `rows.retain(... kind ...)`:
if app.history_mine_only {
    if let Some(mid) = me_id {
        rows.retain(|row| row.contact_id == mid);
    }
}
```

Works together with the existing text search and kind filter (all three compose). `ActivityRow`
already exposes `contact_id`.

## Out of scope

- Changing the todo-completion logic (it already does the right thing for a linked me-todo).
- Adding "ฉัน (Me)" to the Advances / Meetings / Todo-schedule pickers (only the Todo "เกี่ยวกับ"
  field was requested).
- A dedicated "My activity log" page or sidebar entry (the existing Activity History page covers
  viewing, per decision B).
- A todo "ของ" (who) filter option for me on the Todo page (a me-todo filters under ABO; adding a
  dedicated option is unnecessary scope).
- Editing or deleting the me-row from the UI (it appears in no contact list, so it has no
  edit/delete affordance; this is intentional).

## Testing

- **Query layer:** the three unit tests above, plus existing tests must still pass (the added
  `is_me = 0` guards are no-ops for DBs whose only `is_me` row is the hidden me-row).
- **UI:** a successful `cargo build` plus a startup screenshot confirming the Todo "เกี่ยวกับ"
  picker lists "ฉัน (Me)" first and the app renders without missing glyphs (all text reused; no new
  emoji). Run `cargo test` and `cargo build`. Do **not** run `cargo fmt`.
