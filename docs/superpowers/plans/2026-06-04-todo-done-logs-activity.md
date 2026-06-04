# Todo Completion → Activity Log Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a contact-linked todo is ticked complete, prompt for a free-text result and log it to that contact's activity history as a new "ทำงานที่ต้องทำเสร็จ" activity.

**Architecture:** Follows the existing layering. The completion logic and note formatting live in `db::queries` (unit-tested like the rest of the DB layer): a `complete_todo` function marks the todo done and, in one transaction, inserts the activity when the todo has a contact. A new `ui::todo_done` modal (parallel to `ui::confirm`) collects the result text; `ui::todo`'s done-toggle opens it. Marking done and logging happen atomically so a todo is never "done" without its log entry.

**Tech Stack:** Rust, egui/eframe 0.28, rusqlite 0.31 (bundled SQLite), chrono 0.4.

**Spec:** `docs/superpowers/specs/2026-06-04-todo-done-logs-activity-design.md`

**Conventions:** This repo is hand-formatted — **do NOT run `cargo fmt`**. Verify only with `cargo build` / `cargo test`. Match the surrounding code's style (4-space indent, Thai UI strings).

---

## File Structure

- **Modify** `src/db/queries.rs` — `TODO_DONE_KIND` const, `done_note` helper, `complete_todo` fn (+ tests).
- **Modify** `src/db/schema.rs` — migration v8 seeding the kind; bump `CURRENT_VERSION` 7 → 8; import `params`.
- **Modify** `src/db/mod.rs` — `DbConnection::complete_todo` pass-through.
- **Create** `src/ui/todo_done.rs` — `PendingTodoDone` struct + `render(app, ctx)` modal.
- **Modify** `src/ui/mod.rs` — register `pub mod todo_done;`.
- **Modify** `src/ui/todo.rs` — done-toggle opens the dialog (contact) or marks done + status (contactless).
- **Modify** `src/app.rs` — `AppState` fields + initialisers, modal dispatch.

Tasks are ordered so each one compiles and is committable on its own.

---

### Task 1: Seed the `ทำงานที่ต้องทำเสร็จ` activity kind (schema v8) + kind constant

**Files:**
- Modify: `src/db/queries.rs` (const + test)
- Modify: `src/db/schema.rs` (import, version bump, migration block)
- Test: `src/db/queries.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test** — add to the `#[cfg(test)] mod tests` block in `src/db/queries.rs` (alongside the other migration/activity tests):

```rust
#[test]
fn migration_seeds_todo_done_kind() {
    let conn = mem();
    let kinds = list_activity_kinds(&conn).unwrap();
    assert!(kinds.iter().any(|k| k.name == TODO_DONE_KIND));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib db::queries::tests::migration_seeds_todo_done_kind`
Expected: FAIL — compile error, `cannot find value TODO_DONE_KIND in this scope`.

- [ ] **Step 3: Add the kind constant** — in `src/db/queries.rs`, just above `pub fn add_activity` (around line 310), add:

```rust
/// Activity kind logged when a Todo is ticked complete. Seeded into
/// `activity_kinds` by migration v8 so it appears in the history filter and the
/// activity-kinds manager; stored as text on each activity row regardless.
pub const TODO_DONE_KIND: &str = "ทำงานที่ต้องทำเสร็จ";
```

- [ ] **Step 4: Add migration v8** — three edits in `src/db/schema.rs`:

1. Change the import (line 6) from:

```rust
use rusqlite::Connection;
```

to:

```rust
use rusqlite::{params, Connection};
```

2. Bump the version constant (line 11):

```rust
const CURRENT_VERSION: i64 = 8;
```

3. Insert this block immediately after the closing `}` of the `if version < 7 { … }` block (the todos-table migration) and before the `if version != CURRENT_VERSION {` line:

```rust
    if version < 8 {
        // Seed the activity kind logged when a Todo is ticked complete.
        conn.execute(
            "INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1)",
            params![crate::db::queries::TODO_DONE_KIND],
        )?;
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --lib db::queries::tests::migration_seeds_todo_done_kind`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/db/queries.rs src/db/schema.rs
git commit -m "Seed the todo-completion activity kind (schema v8)"
```

---

### Task 2: `done_note` + `complete_todo` (DB completion logic) with tests

**Files:**
- Modify: `src/db/queries.rs` (helper + fn + tests)
- Modify: `src/db/mod.rs` (pass-through method)
- Test: `src/db/queries.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing `done_note` test** — add to the tests module in `src/db/queries.rs`:

```rust
#[test]
fn done_note_combines_task_and_result() {
    assert_eq!(done_note("โทรนัด", "ลูกค้าตอบรับ"), "โทรนัด — ผล: ลูกค้าตอบรับ");
    assert_eq!(done_note("โทรนัด", "   "), "โทรนัด"); // blank result → task only
    assert_eq!(done_note("โทรนัด", ""), "โทรนัด");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --lib db::queries::tests::done_note_combines_task_and_result`
Expected: FAIL — `cannot find function done_note in this scope`.

- [ ] **Step 3: Implement `done_note`** — in `src/db/queries.rs`, just above `pub fn add_todo` (around line 775), add:

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

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --lib db::queries::tests::done_note_combines_task_and_result`
Expected: PASS.

- [ ] **Step 5: Write the failing `complete_todo` tests** — add to the tests module:

```rust
#[test]
fn complete_todo_logs_activity_for_contact() {
    let conn = mem();
    let cid = insert_contact(&conn, &sample_prospect("ธนา")).unwrap();
    let tid = add_todo(&conn, Some(cid), "โทรนัด", None).unwrap();

    complete_todo(&conn, tid, "ลูกค้าตอบรับ").unwrap();

    let rows = list_todos(&conn, "").unwrap();
    assert!(rows.iter().find(|r| r.todo.id == tid).unwrap().todo.done);

    let acts = list_activities(&conn, cid).unwrap();
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].kind, TODO_DONE_KIND);
    assert_eq!(acts[0].note, "โทรนัด — ผล: ลูกค้าตอบรับ");
}

#[test]
fn complete_todo_without_contact_does_not_log() {
    let conn = mem();
    let tid = add_todo(&conn, None, "งานส่วนตัว", None).unwrap();

    complete_todo(&conn, tid, "เสร็จแล้ว").unwrap();

    assert!(list_todos(&conn, "").unwrap()[0].todo.done);
    assert_eq!(list_all_activities(&conn, "").unwrap().len(), 0);
}

#[test]
fn complete_todo_twice_logs_two_activities() {
    let conn = mem();
    let cid = insert_contact(&conn, &sample_prospect("ธนา")).unwrap();
    let tid = add_todo(&conn, Some(cid), "โทรนัด", None).unwrap();

    complete_todo(&conn, tid, "ครั้งที่หนึ่ง").unwrap();
    complete_todo(&conn, tid, "ครั้งที่สอง").unwrap();

    assert_eq!(list_activities(&conn, cid).unwrap().len(), 2);
}
```

- [ ] **Step 6: Run to verify they fail**

Run: `cargo test --lib db::queries::tests::complete_todo`
Expected: FAIL — `cannot find function complete_todo in this scope`.

- [ ] **Step 7: Implement `complete_todo` + the pass-through.**

In `src/db/queries.rs`, immediately after `set_todo_done` (around line 812), add:

```rust
/// Mark a todo done and, when it is tied to a contact, log a `TODO_DONE_KIND`
/// activity with `done_note(task, result)` as its detail — both in one
/// transaction. A contactless todo is still marked done, with no activity.
pub fn complete_todo(conn: &Connection, id: i64, result: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
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
            params![
                contact_id,
                TODO_DONE_KIND,
                done_note(&task, result),
                Local::now().to_rfc3339()
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}
```

(`params`, `Local`, `OptionalExtension` (for `.optional()`), and `Connection` are already imported at the top of `queries.rs` — no new imports needed.)

In `src/db/mod.rs`, in the `// --- todos ---` section, right after the `set_todo_done` method (lines 159–161), add:

```rust
    pub fn complete_todo(&self, id: i64, result: &str) -> Result<()> {
        queries::complete_todo(&self.conn, id, result)
    }
```

- [ ] **Step 8: Run to verify they pass**

Run: `cargo test --lib db::queries::tests::complete_todo`
Expected: PASS (all three).

- [ ] **Step 9: Run the whole DB module to confirm no regressions**

Run: `cargo test --lib db::`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add src/db/queries.rs src/db/mod.rs
git commit -m "Add complete_todo: mark done and log the result as an activity"
```

---

### Task 3: Result-entry dialog modal (`ui/todo_done.rs`) + app wiring

**Files:**
- Create: `src/ui/todo_done.rs`
- Modify: `src/ui/mod.rs` (register module)
- Modify: `src/app.rs` (fields + initialisers + dispatch)

No unit test (egui views aren't unit-tested in this repo); verified by `cargo build`. After this task the dialog compiles and is dispatched but nothing opens it yet (Task 4 wires the toggle), so app behaviour is unchanged.

- [ ] **Step 1: Create `src/ui/todo_done.rs`** with the full module:

```rust
//! "Log result" modal shown when a contact-linked todo is ticked complete.
//!
//! Ticking such a todo sets `AppState.pending_todo_done` instead of marking it
//! done immediately; this modal collects a free-text result and, on "บันทึก",
//! calls `complete_todo` (which marks the todo done and logs the activity).
//! Cancelling leaves the todo unfinished.

use crate::app::AppState;

/// A todo whose done-toggle is awaiting its result text. Set by ticking a
/// contact-linked todo done; consumed by [`render`].
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: String,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_todo_done.clone() else {
        return;
    };

    let mut save = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("บันทึกผลลัพธ์ / Log Result")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&pending.task).strong());
            ui.label(
                egui::RichText::new(format!("ของ: {}", pending.contact_name))
                    .small()
                    .weak(),
            );
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
                if ui
                    .add(egui::Button::new("💾 บันทึก").fill(crate::ui::ACCENT))
                    .clicked()
                {
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
        // Cancelling aborts completion: the done flag was never persisted, so
        // the todo simply stays "ยังไม่เสร็จ".
        app.pending_todo_done = None;
        app.todo_done_result.clear();
    }
}
```

- [ ] **Step 2: Register the module** — in `src/ui/mod.rs`, after the line `pub mod todo;` (line 18), add:

```rust
pub mod todo_done;
```

- [ ] **Step 3: Add the `AppState` fields** — in `src/app.rs`, after line 91 (`pub todo_who_filter: crate::ui::todo::TodoWhoFilter,`), before the struct's closing `}`, add:

```rust
    /// A todo whose done-toggle is awaiting its result text (drives the
    /// `ui::todo_done` modal); `None` when no completion dialog is open.
    pub pending_todo_done: Option<crate::ui::todo_done::PendingTodoDone>,
    /// Result-text buffer for the todo-completion dialog.
    pub todo_done_result: String,
```

- [ ] **Step 4: Add the initialisers** — in `src/app.rs`, after line 140 (`todo_who_filter: crate::ui::todo::TodoWhoFilter::All,`), add:

```rust
            pending_todo_done: None,
            todo_done_result: String::new(),
```

- [ ] **Step 5: Add the modal dispatch** — in `src/app.rs`, after line 468 (`ui::confirm::render(self, ctx);`), add:

```rust
        ui::todo_done::render(self, ctx);
```

- [ ] **Step 6: Build to verify it compiles**

Run: `cargo build`
Expected: compiles clean (no warnings).

- [ ] **Step 7: Commit**

```bash
git add src/ui/todo_done.rs src/ui/mod.rs src/app.rs
git commit -m "Add the todo-completion result dialog (not yet wired)"
```

---

### Task 4: Open the dialog from the done toggle (`ui/todo.rs`)

**Files:**
- Modify: `src/ui/todo.rs` (deferred toggle-apply block)

- [ ] **Step 1: Replace the toggle-apply block.** In `src/ui/todo.rs`, find this block (just after the `// --- apply deferred row actions ---` comment, around line 377):

```rust
        if let Some((id, done)) = toggle {
            if let Err(e) = app.db.set_todo_done(id, done) {
                app.set_error(e);
            }
        }
```

Replace it with:

```rust
        if let Some((id, done)) = toggle {
            if !done {
                // un-tick: back to pending, history untouched
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
                        app.set_status(
                            "ทำเครื่องหมายเสร็จแล้ว — งานนี้ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ",
                        );
                    }
                }
            }
        }
```

(`rows` is an owned `Vec<TodoRow>` that does not borrow `app`, so iterating it while mutating `app` compiles — the same pattern the existing `edit_req` block uses.)

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: compiles clean.

- [ ] **Step 3: Full test suite**

Run: `cargo test`
Expected: PASS (all existing tests + the new DB tests).

- [ ] **Step 4: Commit**

```bash
git add src/ui/todo.rs
git commit -m "Open the result dialog when completing a contact-linked todo"
```

---

### Task 5: Verify end-to-end

**Files:** none (verification only).

- [ ] **Step 1: Build + test** (do NOT run `cargo fmt`)

Run: `cargo build` then `cargo test`
Expected: build clean; all tests pass.

- [ ] **Step 2: Manual smoke test** — run `target\debug\amway_ccs_tracker.exe`, open "📅 สิ่งที่ต้องทำ", and confirm:

- [ ] Tick a todo that HAS a contact → the "บันทึกผลลัพธ์" dialog appears showing the task text and contact name.
- [ ] Type a result → "💾 บันทึก" → open "📝 ประวัติติดต่อ": a new row for that contact, กิจกรรม = `ทำงานที่ต้องทำเสร็จ`, รายละเอียด = `<task> — ผล: <result>`, วันเวลา = now.
- [ ] Tick another contact-linked todo, leave the result blank → บันทึก → the history รายละเอียด is just `<task>` (no "— ผล:").
- [ ] Tick a contact-linked todo → ยกเลิก → the todo stays unchecked / "ยังไม่เสร็จ"; no history row added.
- [ ] Tick a todo with NO contact ("เกี่ยวกับ" = —) → no dialog; it is marked done; the status bar shows it was not logged.
- [ ] Un-tick a previously-completed todo → its history entry remains.
- [ ] The "ประเภทกิจกรรม" filter on the history page lists `ทำงานที่ต้องทำเสร็จ`.

- [ ] **Step 3: Hand off.** No further code changes expected. The feature branch `todo-done-logs-activity` (which already holds the spec commit) is ready — hand to the user to test interactively, then commit/push any remaining work and open a PR per the project workflow (merge to `main` only after explicit approval).
