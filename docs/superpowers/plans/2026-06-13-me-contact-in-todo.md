# Add "ฉัน (Me)" to the Todo "เกี่ยวกับ" Field — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a Todo be about "myself" and log its result to my own activity history, via a hidden "me" contact row that stays out of every existing list.

**Architecture:** A schema migration adds an `is_me` flag column and seeds one hidden me-contact. Every typed list/count query gains an `is_me = 0` guard so the me-row never leaks (network chart, rank legs, dashboards, other pickers). The Todo "เกี่ยวกับ" picker prepends "ฉัน (Me)"; completion reuses the existing linked-todo path (no new logic). The Activity History page gains an "เฉพาะของฉัน" toggle.

**Tech Stack:** Rust, rusqlite 0.31 (bundled SQLite), eframe/egui 0.28, chrono.

**Standing constraints:** NEVER run `cargo fmt` (the repo is hand-formatted). Commit messages end with the `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>` trailer. `ContactType::Abo` serialises to the string `"ABO"` (uppercase) — the me-row's `contact_type` must be `'ABO'`.

---

### Task 1: Database layer — migration v12, query guards, `me_contact_id`, tests

**Files:**
- Modify: `src/db/schema.rs` (add `use chrono::Local;`, bump version, add v12 block)
- Modify: `src/db/queries.rs` (add `is_me = 0` guards to 8 queries; add `me_contact_id`; add tests)
- Modify: `src/db/mod.rs` (add `me_contact_id` passthrough)

- [ ] **Step 1: Bump the schema version and add the v12 migration block**

In `src/db/schema.rs`, change the version constant (line 11):

```rust
const CURRENT_VERSION: i64 = 12;
```

Add `use chrono::Local;` to the imports at the top (after the existing `use rusqlite::{params, Connection};`):

```rust
use chrono::Local;
use rusqlite::{params, Connection};

use crate::error::Result;
```

Add this block immediately after the `if version < 11 { ... }` block and **before** the
`if version != CURRENT_VERSION { ... }` block:

```rust
    if version < 12 {
        // A hidden "me" contact row carries my own activities and can be a todo's
        // target ("เกี่ยวกับ ฉัน"). It is excluded from every typed list/count by an
        // `is_me = 0` guard (see queries.rs) and revealed only in the Todo picker,
        // so it never pollutes the network chart, rank legs, or dashboards. The seed
        // is guarded by NOT EXISTS so it is created exactly once across a fresh
        // install, an upgrade-in-place, or a restore of a pre-v12 backup.
        conn.execute_batch("ALTER TABLE contacts ADD COLUMN is_me INTEGER NOT NULL DEFAULT 0;")?;
        conn.execute(
            "INSERT INTO contacts (name, nickname, gender, network_category, contact_type, is_me, created_at)
             SELECT 'ฉัน', 'Me', 'Male', 'Family', 'ABO', 1, ?1
             WHERE NOT EXISTS (SELECT 1 FROM contacts WHERE is_me = 1)",
            params![Local::now().to_rfc3339()],
        )?;
    }
```

- [ ] **Step 2: Add the `is_me = 0` guards to the typed list/count queries**

In `src/db/queries.rs`, make these exact edits.

`list_contacts` (line ~207) — add a WHERE clause:

```rust
pub fn list_contacts(conn: &Connection) -> Result<Vec<Contact>> {
    let sql = format!("SELECT {C} FROM contacts c WHERE c.is_me = 0 ORDER BY c.name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_contact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
```

`list_by_type` (line ~214) — add `AND c.is_me = 0`:

```rust
pub fn list_by_type(conn: &Connection, ty: ContactType) -> Result<Vec<Contact>> {
    let sql = format!("SELECT {C} FROM contacts c WHERE c.contact_type = ?1 AND c.is_me = 0 ORDER BY c.name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([ty.as_str()], row_to_contact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
```

`me_leg_counts` (line ~267) — add `AND is_me = 0` so the me-row is not counted as its own leg:

```rust
    let mut stmt = conn
        .prepare("SELECT rank FROM contacts WHERE sponsor_id IS NULL AND contact_type = 'ABO' AND is_me = 0")?;
```

`count_by_type` (line ~1577):

```rust
pub fn count_by_type(conn: &Connection, ty: ContactType) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM contacts WHERE contact_type = ?1 AND is_me = 0",
        [ty.as_str()],
        |r| r.get(0),
    )?)
}
```

`count_conversions_this_month` (line ~1586):

```rust
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM contacts
         WHERE contact_type IN ('Customer', 'ABO') AND is_me = 0 AND substr(created_at, 1, 7) = ?1",
        [ym],
        |r| r.get(0),
    )?)
```

`list_prospect_rows` (line ~1612) — add `AND c.is_me = 0` after the contact_type line:

```rust
         WHERE c.contact_type = 'Prospect' AND c.is_me = 0
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
```

`list_customer_rows` (line ~1645):

```rust
         WHERE c.contact_type = 'Customer' AND c.is_me = 0
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
```

`list_abo_rows` (line ~1687):

```rust
         WHERE c.contact_type = 'ABO' AND c.is_me = 0
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
```

- [ ] **Step 3: Add the `me_contact_id` query**

In `src/db/queries.rs`, add this function immediately after `list_abos` (line ~222):

```rust
/// The id of the hidden "me" contact row (seeded by migration v12). Used to
/// prepend the "ฉัน (Me)" option to the Todo picker and to filter the
/// "เฉพาะของฉัน" view in Activity History.
pub fn me_contact_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT id FROM contacts WHERE is_me = 1 LIMIT 1",
        [],
        |r| r.get(0),
    )?)
}
```

- [ ] **Step 4: Write the failing tests**

In `src/db/queries.rs`, add these three tests inside the `#[cfg(test)] mod tests` block (after the
existing `delete_cascades_to_scores_and_follow_up` test, around line 1794):

```rust
    #[test]
    fn me_contact_id_returns_seeded_row() {
        let conn = mem();
        let id = me_contact_id(&conn).unwrap();
        let me = get_contact(&conn, id).unwrap();
        assert_eq!(me.name, "ฉัน");
        assert_eq!(me.contact_type, ContactType::Abo);
    }

    #[test]
    fn me_row_hidden_from_lists() {
        let conn = mem();
        // No contacts added; only the seeded me-row exists. It must not appear in
        // the general list or the ABO count despite its contact_type = 'ABO'.
        assert!(list_contacts(&conn).unwrap().is_empty());
        assert_eq!(count_by_type(&conn, ContactType::Abo).unwrap(), 0);
        assert!(list_abos(&conn).unwrap().is_empty());
    }

    #[test]
    fn me_leg_counts_excludes_me() {
        let conn = mem();
        // The me-row has sponsor_id IS NULL AND contact_type = 'ABO', which would
        // otherwise match me_leg_counts' filter and inflate the leg tally.
        assert_eq!(me_leg_counts(&conn).unwrap(), (0, 0, 0));
    }
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test --lib me_contact_id_returns_seeded_row me_row_hidden_from_lists me_leg_counts_excludes_me`
Expected: compile error or assertion failures (the functions/guards don't exist yet if steps were reordered). If steps 1–3 are already applied, they should PASS — that is also acceptable; the goal is green at step 6.

- [ ] **Step 6: Add the `DbConnection::me_contact_id` passthrough**

In `src/db/mod.rs`, add this method immediately after the `list_abos` passthrough (line ~84):

```rust
    pub fn me_contact_id(&self) -> Result<i64> {
        queries::me_contact_id(&self.conn)
    }
```

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: all tests PASS (the three new ones plus every pre-existing test — the `is_me = 0` guards are no-ops for any DB whose only `is_me` row is the hidden me-row).

- [ ] **Step 8: Commit**

```bash
git add src/db/schema.rs src/db/queries.rs src/db/mod.rs
git commit -m "Add hidden me-contact: migration v12, query guards, me_contact_id

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Todo "เกี่ยวกับ" pickers prepend "ฉัน (Me)"

**Files:**
- Modify: `src/ui/todo.rs:117-123` (add-form picker options)
- Modify: `src/ui/todo_done.rs:33-41` (contactless completion picker options)

- [ ] **Step 1: Prepend "ฉัน (Me)" in the Todo add/edit form picker**

In `src/ui/todo.rs`, replace the contact-options block (currently lines 117–123):

```rust
    // Contacts for the picker (all types), pre-fetched so the combo closure does
    // not borrow app.db while mutating app.todo_form.
    let contacts = app.db.list_contacts().unwrap_or_default();
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();
```

with this version that prepends the hidden me-row as the first option:

```rust
    // Contacts for the picker (all types), pre-fetched so the combo closure does
    // not borrow app.db while mutating app.todo_form. `list_contacts` hides the
    // me-row, so prepend "ฉัน (Me)" explicitly as the first option.
    let me_id = app.db.me_contact_id().ok();
    let contacts = app.db.list_contacts().unwrap_or_default();
    let mut contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();
    if let Some(mid) = me_id {
        contact_options.insert(0, (mid, "ฉัน (Me)".to_string()));
    }
```

- [ ] **Step 2: Prepend "ฉัน (Me)" in the contactless-completion picker**

In `src/ui/todo_done.rs`, replace the options block (currently lines 33–41):

```rust
    let contacts = if pending.contact_name.is_none() {
        app.db.list_contacts().unwrap_or_default()
    } else {
        Vec::new()
    };
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();
```

with this version that prepends "ฉัน (Me)" so a contactless todo's result can be logged to my own
history (a me-target falls back to the generic success toast, which is acceptable):

```rust
    let contacts = if pending.contact_name.is_none() {
        app.db.list_contacts().unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();
    if pending.contact_name.is_none() {
        if let Ok(mid) = app.db.me_contact_id() {
            contact_options.insert(0, (mid, "ฉัน (Me)".to_string()));
        }
    }
```

- [ ] **Step 3: Verify the build compiles**

Run: `cargo build`
Expected: compiles with no errors (warnings about the existing codebase are fine).

- [ ] **Step 4: Commit**

```bash
git add src/ui/todo.rs src/ui/todo_done.rs
git commit -m "Prepend ฉัน (Me) to the Todo เกี่ยวกับ pickers

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Activity History "เฉพาะของฉัน" filter

**Files:**
- Modify: `src/app.rs` (struct field + constructor init)
- Modify: `src/ui/activities.rs` (checkbox + filter)

- [ ] **Step 1: Add the `history_mine_only` field to `AppState`**

In `src/app.rs`, add the field immediately after the `history_kind` field (line 72):

```rust
    /// Kind filter on the aggregate Activity History page (`None` = all kinds).
    pub history_kind: Option<String>,
    /// Activity History: when true, show only the me-row's activities.
    pub history_mine_only: bool,
```

- [ ] **Step 2: Initialise the field in the constructor**

In `src/app.rs`, add the init immediately after `history_kind: None,` (line 172):

```rust
            history_kind: None,
            history_mine_only: false,
```

- [ ] **Step 3: Add the checkbox and filter in the Activity History page**

In `src/ui/activities.rs`, fetch the me id near the top of `render`, right after the `kinds` line
(line 17):

```rust
    let kinds = app.db.list_activity_kinds().unwrap_or_default();
    let me_id = app.db.me_contact_id().ok();
```

Add the checkbox to the filter row, immediately after the kind-popup `egui::popup::popup_below_widget(...)`
call closes (after line 71, still inside the `ui.horizontal(|ui| { ... })` closure):

```rust
        );
        ui.separator();
        ui.checkbox(&mut app.history_mine_only, "เฉพาะของฉัน");
    });
```

(The `);` and `});` above are the existing close of the popup call and the horizontal closure — insert
the `ui.separator();` and `ui.checkbox(...)` lines between them.)

Add the row filter immediately after the existing kind `retain` (lines 78–80):

```rust
    if let Some(k) = &app.history_kind {
        rows.retain(|row| &row.activity.kind == k);
    }
    if app.history_mine_only {
        if let Some(mid) = me_id {
            rows.retain(|row| row.contact_id == mid);
        }
    }
```

- [ ] **Step 4: Verify the build compiles**

Run: `cargo build`
Expected: compiles with no errors.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/ui/activities.rs
git commit -m "Add เฉพาะของฉัน filter to Activity History

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Full build, test, and visual/glyph verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test`
Expected: all tests PASS.

- [ ] **Step 2: Build the release/run binary**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 3: Launch and screenshot to verify rendering**

Launch the app, navigate to the Todo List page, open the "เกี่ยวกับ" picker, and confirm
"ฉัน (Me)" appears as the first option and renders without missing glyphs (no tofu boxes). Use the
repo's established Windows screenshot approach (run the exe, capture its window via
`MainWindowHandle` with `powershell.exe` + `PrintWindow`). All text reuses existing Thai glyphs and
no new emoji are introduced, so glyph risk is nil; this step is a sanity check.

Also confirm the Network (เครือข่าย) page still shows exactly one central "ฉัน (ME)" node (no
duplicate) and the Activity History page shows the "เฉพาะของฉัน" checkbox.

- [ ] **Step 4: Report results**

Summarise: tests pass, build clean, screenshots confirm the picker option and no network-chart
duplication. If any check fails, stop and report rather than committing.

---

## Self-Review

**Spec coverage:**
- Migration v12 + `is_me` + seed → Task 1 Step 1. ✅
- Guards on the 8 typed list/count queries → Task 1 Step 2. ✅
- `me_contact_id` query + passthrough → Task 1 Steps 3, 6. ✅
- Query-layer unit tests → Task 1 Step 4. ✅
- Todo "เกี่ยวกับ" picker prepend → Task 2 Step 1. ✅
- todo_done contactless picker prepend → Task 2 Step 2. ✅
- Completion reuses linked-todo path (no logic change) → no task needed (intentional non-change), noted in Task 2. ✅
- `history_mine_only` field → Task 3 Steps 1–2. ✅
- Activity History checkbox + filter → Task 3 Step 3. ✅
- Build/test/visual verify → Task 4. ✅

**Placeholder scan:** none — every code step shows full code.

**Type consistency:** `me_contact_id` returns `Result<i64>` in queries.rs (Task 1 Step 3), passthrough returns `Result<i64>` (Step 6), callers use `.ok()` / `.ok()`-style handling (Tasks 2, 3). The me-row's `contact_type` is `'ABO'` everywhere (seed + matches `ContactType::Abo.as_str()`). `history_mine_only: bool` defined (Task 3 Step 1) and used (Step 3). Consistent.
