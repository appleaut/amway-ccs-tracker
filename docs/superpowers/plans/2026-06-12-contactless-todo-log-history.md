# Log Contact History from a Contactless Todo — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a contactless todo is ticked done, open the "Log Result" dialog with a contact picker so the user can record the result into a chosen contact's activity history.

**Architecture:** A new DB query marks a todo done and logs a `TODO_DONE_KIND` activity against a *caller-supplied* contact (the existing `complete_todo` only logs against the todo's own contact). The existing completion dialog is generalized: `PendingTodoDone.contact_name` becomes `Option<String>` — `Some` = linked task (read-only contact), `None` = contactless (show a `filter_combo` picker). Both tick handlers (Todo page and Dashboard) open this one dialog.

**Tech Stack:** Rust, eframe/egui 0.28, egui_extras, rusqlite (bundled SQLite), chrono.

**Conventions (must follow):**
- Do **NOT** run `cargo fmt` (repo is hand-formatted, no rustfmt.toml). Verify with `cargo build` / `cargo test` only.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Use only `git add` / `git commit` (never checkout/switch/reset — shared working dir).
- egui emoji must exist in the bundled font subset; this plan adds no new emoji (reuses 💾).

**Spec:** `docs/superpowers/specs/2026-06-12-contactless-todo-log-history-design.md`

---

### Task 1: DB query `complete_todo_to_contact` + passthrough + tests

**Files:**
- Modify: `src/db/queries.rs` (add fn after `complete_todo`, ~line 901; add tests in the `#[cfg(test)]` module after the existing `complete_todo_*` tests, ~line 2196)
- Modify: `src/db/mod.rs` (add passthrough after `complete_todo`, ~line 190)

- [ ] **Step 1: Write the failing tests**

Add these two tests in `src/db/queries.rs` inside the `#[cfg(test)] mod tests` block, immediately after the existing `complete_todo_twice_logs_two_activities` test. They use the existing helpers `mem()`, `insert_contact`, `sample_prospect`, `add_todo`, `list_todos`, `list_activities`.

```rust
    #[test]
    fn complete_todo_to_contact_logs_to_chosen_contact() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ปรีชา")).unwrap();
        // contactless todo (contact_id = None)
        let tid = add_todo(&conn, None, "โทรนัด", None).unwrap();

        complete_todo_to_contact(&conn, tid, cid, "ลูกค้าตอบรับ").unwrap();

        // todo is marked done
        assert!(list_todos(&conn, "").unwrap().iter().find(|r| r.todo.id == tid).unwrap().todo.done);

        // exactly one activity logged on the chosen contact, same format as complete_todo
        let acts = list_activities(&conn, cid).unwrap();
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, TODO_DONE_KIND);
        assert_eq!(acts[0].note, "โทรนัด — ผล: ลูกค้าตอบรับ");
    }

    #[test]
    fn complete_todo_to_contact_leaves_task_contactless() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ปรีชา")).unwrap();
        let tid = add_todo(&conn, None, "งาน", None).unwrap();

        complete_todo_to_contact(&conn, tid, cid, "").unwrap();

        // the task's own contact_id is untouched (still contactless)
        let rows = list_todos(&conn, "").unwrap();
        assert_eq!(rows.iter().find(|r| r.todo.id == tid).unwrap().todo.contact_id, None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib complete_todo_to_contact`
Expected: FAIL — compile error `cannot find function complete_todo_to_contact in this scope`.

- [ ] **Step 3: Implement `complete_todo_to_contact`**

In `src/db/queries.rs`, add this function immediately after the existing `complete_todo` function (after its closing `}` near line 901):

```rust
/// Mark a todo done AND log a `TODO_DONE_KIND` activity against the GIVEN
/// contact — both in one transaction. Used when a *contactless* todo is
/// completed with a contact picked in the Log Result dialog. The todo's own
/// `contact_id` is left unchanged (the task stays contactless); only the chosen
/// contact's history gains an entry. Distinct from `complete_todo`, which logs
/// against the todo's own `contact_id`.
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

- [ ] **Step 4: Add the `DbConnection` passthrough**

In `src/db/mod.rs`, add this method immediately after the existing `complete_todo` wrapper (after line ~190):

```rust
    pub fn complete_todo_to_contact(&self, id: i64, contact_id: i64, result: &str) -> Result<()> {
        queries::complete_todo_to_contact(&self.conn, id, contact_id, result)
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib complete_todo_to_contact`
Expected: PASS (2 tests).

- [ ] **Step 6: Run the full test suite**

Run: `cargo test`
Expected: all tests pass (existing `complete_todo_*` tests unaffected).

- [ ] **Step 7: Commit**

```bash
git add src/db/queries.rs src/db/mod.rs
git commit -m "$(cat <<'EOF'
Add complete_todo_to_contact: log todo result to a chosen contact

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Generalize the Log Result dialog + open it for contactless todos

**Files:**
- Modify: `src/app.rs` (add two picker fields ~after line 96; initialize them ~after line 178)
- Modify: `src/ui/todo_done.rs` (whole file: `PendingTodoDone.contact_name` → `Option<String>`; render picker; branch the save call)
- Modify: `src/ui/todo.rs` (tick-done handler, ~lines 383–404)
- Modify: `src/ui/dashboard.rs` (complete-todo action, ~lines 268–285)

There is no automatable unit test for egui dialog rendering; this task is verified by `cargo build` + `cargo test` (existing tests still green) and the manual screenshot in Task 3.

- [ ] **Step 1: Add picker state fields to `AppState`**

In `src/app.rs`, add these fields immediately after `pub todo_done_result: String,` (line 96):

```rust
    /// Contact picked in the todo-completion dialog when the todo is contactless;
    /// the result is logged to this contact. `None` = log no history.
    pub todo_done_contact_id: Option<i64>,
    /// Search-filter buffer for the contactless todo-completion contact picker.
    pub todo_done_contact_filter: String,
```

- [ ] **Step 2: Initialize the new fields in the constructor**

In `src/app.rs`, add these immediately after `todo_done_result: String::new(),` (line 178):

```rust
            todo_done_contact_id: None,
            todo_done_contact_filter: String::new(),
```

- [ ] **Step 3: Rewrite `src/ui/todo_done.rs`**

Replace the entire contents of `src/ui/todo_done.rs` with:

```rust
//! "Log result" modal shown when a todo is ticked complete.
//!
//! Ticking a todo sets `AppState.pending_todo_done` instead of marking it done
//! immediately; this modal collects a free-text result and, on "บันทึก",
//! completes the todo. For a contact-linked todo it logs the result to that
//! contact's history; for a contactless todo it shows a contact picker so the
//! user may choose a contact to log against (or none). Cancelling leaves the
//! todo unfinished.

use crate::app::AppState;

/// A todo whose done-toggle is awaiting its result text. Set by ticking a todo
/// done; consumed by [`render`]. `contact_name` is `Some` for a contact-linked
/// todo (shown read-only) and `None` for a contactless todo (a contact picker
/// is shown so the result can be logged against a chosen contact).
#[derive(Clone)]
pub struct PendingTodoDone {
    pub id: i64,
    pub task: String,
    pub contact_name: Option<String>,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_todo_done.clone() else {
        return;
    };

    // Contact options for the contactless picker, fetched once so the
    // filter_combo closure doesn't borrow app.db while mutating picker state.
    // Empty for a contact-linked todo (no picker shown).
    let contact_options: Vec<(i64, String)> = if pending.contact_name.is_none() {
        app.db
            .list_contacts()
            .unwrap_or_default()
            .iter()
            .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
            .collect()
    } else {
        Vec::new()
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
            match &pending.contact_name {
                Some(name) => {
                    ui.label(
                        egui::RichText::new(format!("ของ: {}", name))
                            .small()
                            .weak(),
                    );
                }
                None => {
                    ui.add_space(8.0);
                    ui.label("บันทึกประวัติของ:");
                    crate::ui::filter_combo(
                        ui,
                        "todo_done_contact_cb",
                        &mut app.todo_done_contact_id,
                        &mut app.todo_done_contact_filter,
                        Some("— ไม่บันทึกประวัติ —"),
                        &contact_options,
                        360.0,
                    );
                }
            }
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
        // Linked todo  -> log to its own contact (complete_todo).
        // Contactless + picked -> log to the chosen contact.
        // Contactless + none   -> just mark done, no history.
        let result = match &pending.contact_name {
            Some(_) => app.db.complete_todo(pending.id, &app.todo_done_result),
            None => match app.todo_done_contact_id {
                Some(cid) => {
                    app.db.complete_todo_to_contact(pending.id, cid, &app.todo_done_result)
                }
                None => app.db.set_todo_done(pending.id, true),
            },
        };
        match result {
            Ok(()) => {
                // Name for the success toast: the linked contact, or the picked one.
                let logged_name: Option<String> = match &pending.contact_name {
                    Some(name) => Some(name.clone()),
                    None => app.todo_done_contact_id.and_then(|cid| {
                        contact_options
                            .iter()
                            .find(|(id, _)| *id == cid)
                            .map(|(_, label)| label.clone())
                    }),
                };
                match logged_name {
                    Some(name) => app.set_status(format!("บันทึกลงประวัติของ {} แล้ว", name)),
                    None => app.set_status("ทำเครื่องหมายเสร็จแล้ว"),
                }
                // Clear only on success — on error the dialog stays open with
                // input preserved so the user can retry.
                app.pending_todo_done = None;
                app.todo_done_result.clear();
                app.todo_done_contact_id = None;
                app.todo_done_contact_filter.clear();
            }
            Err(e) => app.set_error(e),
        }
    } else if cancel || !open {
        // Cancelling aborts completion: the done flag was never persisted, so
        // the todo simply stays "ยังไม่เสร็จ".
        app.pending_todo_done = None;
        app.todo_done_result.clear();
        app.todo_done_contact_id = None;
        app.todo_done_contact_filter.clear();
    }
}
```

- [ ] **Step 4: Update the Todo-page tick handler in `src/ui/todo.rs`**

Replace the `if let Some((id, done)) = toggle { ... }` block (currently lines ~377–405, the `else if let Some(row) = ...` arm that opens the dialog or marks done) so the contactless branch opens the dialog too. The new block:

```rust
        // --- apply deferred row actions ---
        if let Some((id, done)) = toggle {
            if !done {
                // un-tick: back to pending, history untouched
                if let Err(e) = app.db.set_todo_done(id, false) {
                    app.set_error(e);
                }
            } else if let Some(row) = rows.iter().find(|r| r.todo.id == id) {
                // Open the Log Result dialog. A contact-linked todo shows its
                // contact read-only; a contactless todo shows a contact picker.
                // Completion is deferred until "บันทึก".
                let contact_name = match (row.todo.contact_id, &row.contact_name) {
                    (Some(_), Some(name)) => Some(name.clone()),
                    _ => None,
                };
                app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone {
                    id,
                    task: row.todo.task.clone(),
                    contact_name,
                });
                app.todo_done_result.clear();
                app.todo_done_contact_id = None;
                app.todo_done_contact_filter.clear();
            }
        }
```

Leave the `edit_req` and `delete_req` blocks that follow it unchanged.

- [ ] **Step 5: Update the Dashboard complete-todo action in `src/ui/dashboard.rs`**

Replace the action arm at lines ~268–285 (the `} => match (contact_id, contact_name) { ... }` block) with one that opens the dialog in both cases:

```rust
            id,
            task,
            contact_id,
            contact_name,
        } => {
            // Open the Log Result dialog. Linked -> read-only contact;
            // contactless -> contact picker. (Mirrors the Todo page.)
            let contact_name = match (contact_id, contact_name) {
                (Some(_), Some(name)) => Some(name),
                _ => None,
            };
            app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone {
                id,
                task,
                contact_name,
            });
            app.todo_done_result.clear();
            app.todo_done_contact_id = None;
            app.todo_done_contact_filter.clear();
        }
```

Note: this removes the dashboard's old instant `set_todo_done` + `"ทำงานเสร็จแล้ว"` path for contactless todos, so completing a task from the Dashboard's attention panel now behaves identically to the Todo page.

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: compiles with no errors. (No new `dead_code` warnings — both new `AppState` fields are read by `todo_done.rs`.)

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/ui/todo_done.rs src/ui/todo.rs src/ui/dashboard.rs
git commit -m "$(cat <<'EOF'
Let contactless todos log history via a contact picker on completion

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Visual verification + finish

**Files:** none (verification only).

- [ ] **Step 1: Build the release/debug binary**

Run: `cargo build`
Expected: success.

- [ ] **Step 2: Launch the app and capture the contactless done dialog**

Launch the built exe, go to the **สิ่งที่ต้องทำ / Todo List** page, create a task with `เกี่ยวกับ = — ไม่ระบุ —`, then tick it done. Capture the window via the project's screenshot method (Windows `powershell.exe` + `PrintWindow` against the `MainWindowHandle` — NOT `pwsh`, which lacks `System.Drawing.Bitmap`; see memory `egui-app-screenshot-verify`).

Verify in the screenshot:
- The "บันทึกผลลัพธ์ / Log Result" dialog opens (it no longer marks done instantly).
- A contact picker labelled **"บันทึกประวัติของ:"** is shown with the **"— ไม่บันทึกประวัติ —"** option.
- The result text box and 💾 บันทึก / ยกเลิก buttons render with no tofu glyphs.

- [ ] **Step 3: Functional spot-check**

- Pick a contact + type a result + บันทึก → status shows "บันทึกลงประวัติของ … แล้ว"; open that contact's activity history and confirm a `ทำงานที่ต้องทำเสร็จ` entry with the result text; confirm the task's `เกี่ยวกับ` is still `—` (stayed contactless).
- Tick another contactless task done, leave the picker on "— ไม่บันทึกประวัติ —", บันทึก → status "ทำเครื่องหมายเสร็จแล้ว", no history entry created.
- Tick a contactless task done then ยกเลิก → task stays ยังไม่เสร็จ.
- Tick a contact-linked task done → dialog shows read-only "ของ: …" (no picker), บันทึก logs to that contact as before.

- [ ] **Step 4: Finish the branch**

Use the **superpowers:finishing-a-development-branch** skill to verify tests and present merge/PR options.

---

## Self-Review

**Spec coverage:**
- Spec §1 (DB layer: `complete_todo_to_contact`, passthrough, tests, reuse `done_note`/`TODO_DONE_KIND`, no-pick reuses `set_todo_done`) → Task 1. ✅
- Spec §2 (dialog: `contact_name: Option<String>`, picker with "— ไม่บันทึกประวัติ —", three save branches, clear-on-success, cancel aborts, options fetched once) → Task 2 Step 3. ✅
- Spec §3 (AppState picker fields + init) → Task 2 Steps 1–2. ✅
- Spec §4 (Todo-page tick handler opens dialog for contactless; un-tick unchanged; old message removed) → Task 2 Step 4. ✅
- Spec "Out of scope" (task stays contactless) → enforced by `complete_todo_to_contact` (no UPDATE of contact_id) and asserted in Task 1 test 2. ✅
- Dashboard call site: not in the spec, but `PendingTodoDone`'s shape change forces an update there; Task 2 Step 5 makes it consistent with the Todo page (flagged to the user).

**Placeholder scan:** No TBD/TODO/"handle edge cases"/uncoded steps — every code step has full code. ✅

**Type consistency:** `PendingTodoDone { id: i64, task: String, contact_name: Option<String> }` is constructed identically in `todo_done.rs` (definition), `todo.rs`, and `dashboard.rs`. `complete_todo_to_contact(id, contact_id, result)` signature matches between queries.rs, mod.rs passthrough, and the `todo_done.rs` call. `filter_combo(ui, id_source, &mut Option<i64>, &mut String, Option<&str>, &[(i64,String)], f32)` call matches its definition in `ui/mod.rs`. ✅
