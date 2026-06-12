# Log Contact History from a Contactless Todo — Design

**Date:** 2026-06-12
**Status:** Approved

## Problem

On the Todo List page a task may be created without a contact (`เกี่ยวกับ = — ไม่ระบุ —`).
Today, ticking such a contactless task **done** marks it complete immediately and shows
*"ทำเครื่องหมายเสร็จแล้ว — งานนี้ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ"* — no activity history is
recorded. A contact-linked task, by contrast, opens a "Log Result" dialog and writes the
result into that contact's activity history.

The user wants to be able to record contact history when completing a contactless task too:
at completion time, pick which contact the result should be logged against.

## Constraint

`activities.contact_id` is `NOT NULL REFERENCES contacts(id)`. Every history entry must belong
to a contact — there is no contactless activity log. Therefore "record history" for a
contactless task means **choosing a contact at completion time** and logging against it. The
task itself stays contactless (we do not write the chosen contact back onto the task).

## Decisions (locked)

- **Q1 = A:** Ticking a contactless task done opens the Log Result dialog with an added
  **contact picker**. Picking a contact is optional. If none is picked, the task is just
  marked done with no history.
- **Q2 = A:** Dialog buttons mirror the existing contact-linked dialog exactly —
  **บันทึก** marks done (and logs if a contact was picked); **ยกเลิก**/close aborts and the
  task stays *ยังไม่เสร็จ*.

## Design

### 1. Database layer (`src/db/queries.rs`, `src/db/mod.rs`)

Add one new query. It is distinct from `complete_todo`, which logs against the todo's *own*
`contact_id`; this one logs against a **caller-supplied** contact.

```rust
/// Mark a todo done AND log a TODO_DONE_KIND activity against the GIVEN contact,
/// both in one transaction. Used when a contactless todo is completed with a
/// contact picked in the Log Result dialog. The task's own contact_id is left
/// unchanged (it stays contactless).
pub fn complete_todo_to_contact(
    conn: &Connection,
    id: i64,
    contact_id: i64,
    result: &str,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE todos SET done = 1 WHERE id = ?1", [id])?;
    let task: String =
        tx.query_row("SELECT task FROM todos WHERE id = ?1", [id], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![contact_id, TODO_DONE_KIND, done_note(&task, result), Local::now().to_rfc3339()],
    )?;
    tx.commit()?;
    Ok(())
}
```

- Reuses `TODO_DONE_KIND` and `done_note(task, result)` — identical formatting to the
  linked-task path, so history entries look the same regardless of how the contact was set.
- The **no-contact-picked** path reuses the existing `set_todo_done(id, true)` — no new query.
- `complete_todo` (linked tasks) is **unchanged**.
- Add a `DbConnection::complete_todo_to_contact` passthrough in `src/db/mod.rs` mirroring the
  existing `complete_todo` wrapper.

**Unit tests** (in `queries.rs` `#[cfg(test)]`):
- `complete_todo_to_contact_logs_to_chosen_contact`: create a contactless todo + a contact,
  call the new fn, assert the todo is `done` and exactly one activity exists on the chosen
  contact with `kind == TODO_DONE_KIND` and the expected `done_note` text.
- `complete_todo_to_contact_leaves_task_contactless`: after the call, the todo row's
  `contact_id` is still `NULL`.

### 2. The dialog (`src/ui/todo_done.rs`)

Make the contact optional so one dialog serves both cases:

```rust
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: Option<String>, // Some = linked (fixed, read-only); None = contactless (show picker)
}
```

Rendering:
- **Linked** (`Some(name)`): unchanged — shows read-only *"ของ: {name}"*.
- **Contactless** (`None`): shows a contact picker using the existing `filter_combo` widget
  (same one used in the add-form), bound to new `AppState` fields, with a
  *"— ไม่บันทึกประวัติ —"* none-option. The free-text result field is shown in both cases.

On **Save**:
- contactless + a contact picked → `app.db.complete_todo_to_contact(id, contact_id, &result)`;
  status *"บันทึกลงประวัติของ {name} แล้ว"*.
- contactless + no contact picked → `app.db.set_todo_done(id, true)`; status
  *"ทำเครื่องหมายเสร็จแล้ว"*.
- linked → `app.db.complete_todo(id, &result)` (existing); status unchanged.

Clear the dialog state (`pending_todo_done`, `todo_done_result`, picker fields) only on
success — on error the dialog stays open with input preserved, matching current behavior.

On **Cancel / window close** → abort: clear the same state, task stays not-done.

The picker's contact options are fetched once via `app.db.list_contacts()` at the top of
`render` (owned `Vec`), so the `filter_combo` closure does not borrow `app.db` while mutating
the picker fields — same pattern as `ui/todo.rs`.

### 3. Picker state on `AppState` (`src/app.rs`)

The dialog clones `pending_todo_done` each frame, so picker selection cannot persist through
the clone. Store it on `AppState` next to the existing `todo_done_result`:

```rust
pub todo_done_contact_id: Option<i64>,
pub todo_done_contact_filter: String,
```

Initialize both to default (`None` / empty) in the `AppState` constructor.

### 4. Tick handler (`src/ui/todo.rs`)

In the done-toggle handler, the contactless branch changes from "mark done immediately" to
"open the dialog":

```rust
match (row.todo.contact_id, &row.contact_name) {
    (Some(_), Some(name)) => {
        // linked — unchanged
        app.pending_todo_done = Some(PendingTodoDone {
            id, task: row.todo.task.clone(), contact_name: Some(name.clone()),
        });
        app.todo_done_result.clear();
        app.todo_done_contact_id = None;
        app.todo_done_contact_filter.clear();
    }
    _ => {
        // contactless — open the SAME dialog with no fixed contact (picker shown)
        app.pending_todo_done = Some(PendingTodoDone {
            id, task: row.todo.task.clone(), contact_name: None,
        });
        app.todo_done_result.clear();
        app.todo_done_contact_id = None;
        app.todo_done_contact_filter.clear();
    }
}
```

- Un-ticking (done → pending) is unchanged.
- The old instant-done branch and its *"ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ"* status message are
  removed (replaced by the dialog).
- Resetting the picker fields when opening the dialog (both arms) keeps the `Some(name)` arm
  setting them too, so a stale picker selection never leaks into a later contactless dialog.

## Out of scope

- Writing the chosen contact back onto the task (that was option B, rejected).
- A contactless / general activity log not tied to any contact (schema forbids it).
- Changing the contact of an already-linked task from the done dialog.

## Testing

- **Query layer:** the two unit tests above. Existing `complete_todo_*` tests remain valid
  (that path is unchanged).
- **UI:** verified by a successful build plus a screenshot check of the contactless done
  dialog (picker visible, glyphs render), consistent with how this repo verifies egui work.
- Run `cargo test` (all must pass) and `cargo build`. Do **not** run `cargo fmt`.
