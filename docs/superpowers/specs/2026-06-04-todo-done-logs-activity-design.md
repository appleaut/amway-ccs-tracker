# Todo Completion → Activity Log — Design Spec

**Date:** 2026-06-04
**Feature:** When a todo on the "สิ่งที่ต้องทำ / Todo List" page is ticked complete, prompt
for a free-text **result** ("ผลลัพธ์") and log it as an entry in that contact's activity
history ("ประวัติการติดต่อ"), so completed work shows up on the Activity History page.

## Goal

Close the loop between forward-looking tasks (todos) and the past-interaction log
(activities). Finishing a task for a contact should leave a permanent, searchable record
of *what was done and how it turned out*, without the user re-typing it into the activity
log by hand.

## Decisions (from brainstorming)

1. **Contactless todos** — an activity row requires a contact (`activities.contact_id` is
   `NOT NULL`), but a todo's contact is optional. Completing a todo with **no** contact
   just marks it done and shows a status note explaining it was not logged; **no result
   dialog** appears (there is nowhere to file the result).
2. **Activity kind** — a new dedicated, user-manageable type `ทำงานที่ต้องทำเสร็จ`, seeded
   into `activity_kinds` so it appears in the Activity History kind filter and the
   activity-kinds management page.
3. **Result entry** — ticking a contact-linked todo opens a modal asking for the result
   before the todo is marked done.
4. **Activity detail (note)** — task text **plus** the result:
   `"<task> — ผล: <result>"`. When the result is left blank, the note is just `"<task>"`.
5. **Cancel** — cancelling / closing the result dialog **aborts completion**: the todo
   stays "ยังไม่เสร็จ" (the checkbox does not stick).
6. **Result optional** — the result may be left empty; "บันทึก" is always enabled.
7. **Re-completion** — every transition to done logs a new activity (un-ticking never
   removes history; over-logged rows are deleted manually with 🗑 on the history page).

## Architecture

Follows the existing layering (mirrors the Todo List and activities features):
`db::schema` (migration) → `db::queries` (typed SQL + in-memory tests) → `db::mod`
(`DbConnection` pass-through) → a new `ui::todo_done` modal (parallel to `ui::confirm`) →
`ui::todo` (opens the modal from the done toggle) → wired into `app.rs` (state + modal
dispatch).

The completion logic and the note formatting live in `db::queries` so they are unit-tested
the same way the rest of the DB layer is; the UI only collects the result text and shows
the dialog. Marking done and inserting the activity happen in **one transaction**, so a
todo is never left "done" without its log entry (or vice-versa).

## 1. Data model & schema

No new table or column. `activities` already stores `kind` as free text, so the seeded
kind only needs to exist for the filter/dropdown; logged rows keep their text even if the
kind is later renamed or deleted (same guarantee as every other activity kind).

### Activity-kind constant — `src/db/queries.rs`

```rust
/// Activity kind logged when a Todo is ticked complete. Seeded into
/// `activity_kinds` by migration v8 so it appears in the history filter and the
/// activity-kinds manager; stored as text on each activity row regardless.
pub const TODO_DONE_KIND: &str = "ทำงานที่ต้องทำเสร็จ";
```

### Migration (schema v8) — `src/db/schema.rs`

Bump `CURRENT_VERSION` 7 → 8 and add (using a bound parameter, not string interpolation;
needs `use rusqlite::params;` in `schema.rs`):

```rust
if version < 8 {
    // Seed the activity kind used when a Todo is ticked complete.
    conn.execute(
        "INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1)",
        params![crate::db::queries::TODO_DONE_KIND],
    )?;
}
```

`INSERT OR IGNORE` keeps the migration a no-op if a kind with that name already exists
(the `name` column is `UNIQUE`).

## 2. DB layer

### `src/db/queries.rs` (with in-memory unit tests)

A pure note-formatting helper (decision #4 / #6), kept separate so it is trivially tested:

```rust
/// Build the activity note for a completed todo: the task text, plus
/// "— ผล: <result>" when a result was entered. A blank result → task only.
pub fn done_note(task: &str, result: &str) -> String {
    let result = result.trim();
    if result.is_empty() {
        task.to_string()
    } else {
        format!("{task} — ผล: {result}")
    }
}
```

The completion function — sets `done`, reads the todo's contact, and (only when a contact
is set) inserts the activity, all in one transaction:

```rust
/// Mark a todo done and, if it is tied to a contact, log a "ทำงานที่ต้องทำเสร็จ"
/// activity with `done_note(task, result)` as its detail. No-ops the activity
/// insert for a contactless todo (it still gets marked done).
pub fn complete_todo(conn: &Connection, id: i64, result: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;   // &Connection → unchecked_transaction
    tx.execute("UPDATE todos SET done = 1 WHERE id = ?1", [id])?;
    let row: Option<(Option<i64>, String)> = tx
        .query_row(
            "SELECT contact_id, task FROM todos WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((Some(contact_id), task)) = row {
        tx.execute(
            "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![contact_id, TODO_DONE_KIND, done_note(&task, result), Local::now().to_rfc3339()],
        )?;
    }
    tx.commit()?;
    Ok(())
}
```

(`.optional()` needs `rusqlite::OptionalExtension` in scope — already used elsewhere in
`queries.rs`.) The existing `set_todo_done` is kept and still used for **un-ticking** and
for completing a contactless todo.

### `src/db/mod.rs`

One pass-through in the `// --- todos ---` section:

```rust
pub fn complete_todo(&self, id: i64, result: &str) -> Result<()> {
    queries::complete_todo(&self.conn, id, result)
}
```

### Tests (in the `queries.rs` `#[cfg(test)]` module, reusing `mem()` / `insert_contact` / `sample_prospect`)

- `done_note_combines_task_and_result` — `("โทรหา","รับสาย") → "โทรหา — ผล: รับสาย"`;
  blank / whitespace result → `"โทรหา"`.
- `complete_todo_logs_activity_for_contact` — todo with a contact: after `complete_todo`,
  `done == true`, exactly one activity for that contact with `kind == TODO_DONE_KIND` and
  `note == "<task> — ผล: <result>"`.
- `complete_todo_without_contact_does_not_log` — contactless todo: `done == true`, zero
  activities created.
- `complete_todo_twice_logs_two_activities` — re-completing logs a second activity
  (decision #7).
- `migration_seeds_todo_done_kind` — after `init`, `list_activity_kinds` contains
  `TODO_DONE_KIND`.

## 3. UI — result dialog & done toggle

### New modal — `src/ui/todo_done.rs` (registered in `src/ui/mod.rs`: `pub mod todo_done;`)

State-driven modal in the style of `ui::confirm`:

```rust
/// A pending todo completion awaiting its result text. Set by ticking a
/// contact-linked todo done; consumed by this modal.
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: String,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_todo_done.clone() else { return; };
    let (mut save, mut cancel, mut open) = (false, false, true);

    egui::Window::new("บันทึกผลลัพธ์ / Log Result")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&pending.task).strong());
            ui.label(egui::RichText::new(format!("ของ: {}", pending.contact_name)).small().weak());
            ui.add_space(8.0);
            ui.label("ผลลัพธ์:");
            ui.add(
                egui::TextEdit::multiline(&mut app.todo_done_result)
                    .hint_text("ผลลัพธ์ของงานนี้ (ไม่บังคับ)")
                    .desired_width(360.0)
                    .desired_rows(3),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("💾 บันทึก").fill(crate::ui::ACCENT)).clicked() {
                    save = true;
                }
                if ui.button("ยกเลิก").clicked() {
                    cancel = true;
                }
            });
        });

    if save {
        match app.db.complete_todo(pending.id, &app.todo_done_result) {
            Ok(()) => app.set_status(format!("บันทึกลงประวัติของ {} แล้ว", pending.contact_name)),
            Err(e) => app.set_error(e),
        }
        app.pending_todo_done = None;
        app.todo_done_result.clear();
    } else if cancel || !open {
        // Decision #5: cancelling aborts completion. The done flag was never
        // persisted, so the todo simply stays "ยังไม่เสร็จ".
        app.pending_todo_done = None;
        app.todo_done_result.clear();
    }
}
```

### Done toggle — `src/ui/todo.rs`

The table's done checkbox still records `toggle = Some((id, done))` during render. The
deferred-apply block (currently a single `set_todo_done` call) becomes:

```rust
if let Some((id, done)) = toggle {
    if !done {
        // un-tick: revert to pending, history untouched
        if let Err(e) = app.db.set_todo_done(id, false) {
            app.set_error(e);
        }
    } else if let Some(row) = rows.iter().find(|r| r.todo.id == id) {
        match (row.todo.contact_id, &row.contact_name) {
            (Some(_), Some(name)) => {
                // open the result dialog; completion is deferred until "บันทึก"
                app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone {
                    id,
                    task: row.todo.task.clone(),
                    contact_name: name.clone(),
                });
                app.todo_done_result.clear();
            }
            _ => {
                // no contact: mark done now, nothing to log
                if let Err(e) = app.db.set_todo_done(id, true) {
                    app.set_error(e);
                }
                app.set_status("ทำเครื่องหมายเสร็จแล้ว — งานนี้ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ");
            }
        }
    }
}
```

`rows` is an owned `Vec<TodoRow>` (it does not borrow `app`), so iterating it while
mutating `app` compiles — the same pattern the existing `edit_req` block already uses.

**UX note:** while the dialog is open the todo has not been persisted as done, so its
checkbox reads unchecked behind the modal — consistent with "completion pending a result".
On "บันทึก" the next frame reloads it as done; on "ยกเลิก" it stays unchecked.

## 4. App wiring — `src/app.rs`

- New `AppState` fields (with initialisers in `AppState::new`):
  ```rust
  /// A todo whose done-toggle is awaiting its result text (drives the
  /// `ui::todo_done` modal); `None` when no completion dialog is open.
  pub pending_todo_done: Option<ui::todo_done::PendingTodoDone>,   // init: None
  /// Result-text buffer for the todo-completion dialog.
  pub todo_done_result: String,                                    // init: String::new()
  ```
- Modal dispatch: add `ui::todo_done::render(self, ctx);` in `update`, next to the other
  modal renders (after `ui::confirm::render(self, ctx);`).

## Out of scope (YAGNI)

- Editing or back-dating the logged activity's timestamp (always "now").
- A `todo_id`/`activity_id` link column or dedup of repeated completions (decision #7
  accepts duplicates).
- Re-pointing future auto-logs if the user renames the seeded `ทำงานที่ต้องทำเสร็จ` kind
  (existing rows are relabelled by `rename_activity_kind`; new logs use the constant).
- Removing the logged activity when a completed todo is un-ticked.
- A result prompt for contactless todos.

## Testing & verification

- `cargo test` — new `queries.rs` tests (above) pass; existing tests still pass.
- `cargo build` — compiles clean (no new dependency).
- **Do not run `cargo fmt`** (repo is hand-formatted).
- Manual smoke: tick a contact-linked todo → dialog appears → enter a result → it shows on
  the Activity History page as `ทำงานที่ต้องทำเสร็จ` with note `"<task> — ผล: <result>"` at
  the current time; leave the result blank → note is just `"<task>"`; cancel the dialog →
  the todo stays pending; tick a contactless todo → marked done with the status note and no
  history row; un-tick a done todo → history entry remains.
