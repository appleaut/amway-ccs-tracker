# Advance Payments (สำรองจ่าย) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "สำรองจ่าย" (Advance Payments) menu to record money fronted to buy products for a contact and collect later; collecting logs an entry into that contact's activity history.

**Architecture:** Mirror the existing Todo feature. One new `advances` table; typed queries in `db/queries.rs` with in-memory tests; a `DbConnection` passthrough layer; a list page (`ui/advances.rs`) and a collect dialog (`ui/advance_collect.rs`). Collecting runs `UPDATE` + activity `INSERT` in one transaction, exactly like `complete_todo`.

**Tech Stack:** Rust, egui/eframe 0.28, egui_extras (DatePickerButton/TableBuilder), rusqlite 0.31 (bundled), chrono 0.4.

**Spec:** `docs/superpowers/specs/2026-06-04-advance-payment-design.md`

---

## ⚠️ Project conventions (read first)

- **NEVER run `cargo fmt`** — this repo is hand-formatted (no rustfmt.toml); `cargo fmt` reformats every file. Verify only with `cargo build` / `cargo check` / `cargo test`.
- **Commit messages must end with:** `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- The crate is a **binary**, so run tests with `cargo test <name>` (not `--lib`).
- Indentation is **4 spaces**. Match the surrounding style; Thai UI strings are normal.

## File structure

| File | Responsibility | Change |
|---|---|---|
| `src/db/schema.rs` | migrations | v9: create `advances`, seed kind; bump `CURRENT_VERSION` 8→9 |
| `src/db/queries.rs` | typed SQL + tests | const, `group_thousands`, `advance_note`, `AdvanceRow`, CRUD, `collect_advance`, `outstanding_total`, tests |
| `src/db/mod.rs` | `DbConnection` facade | passthroughs |
| `src/models/advance.rs` | `Advance` struct (new) | data definition |
| `src/models/mod.rs` | model registry | `pub mod advance;` |
| `src/ui/advances.rs` | list page (new) | form, filter, table, row actions |
| `src/ui/advance_collect.rs` | collect dialog (new) | `PendingAdvanceCollect` + render |
| `src/ui/confirm.rs` | shared delete modal | `PendingDelete::Advance` |
| `src/ui/mod.rs` | view registry | `View::Advances`, `pub mod` lines |
| `src/app.rs` | app state + main loop | fields, initializers, sidebar item, dispatches |

---

## Task 1: Migration v9 — `advances` table + seeded activity kind

**Files:**
- Modify: `src/db/queries.rs` (add constant; add test in the `#[cfg(test)] mod tests` block)
- Modify: `src/db/schema.rs` (bump version; add migration block)

- [ ] **Step 1: Add the activity-kind constant**

In `src/db/queries.rs`, immediately after the existing `pub const TODO_DONE_KIND: &str = "ทำงานที่ต้องทำเสร็จ";` line, add:

```rust
/// Activity kind logged when an advance payment is collected. Seeded by the v9
/// migration; stored as text on each activity row (like all kinds).
pub const ADVANCE_COLLECTED_KIND: &str = "เก็บเงินค่าสินค้า (สำรองจ่าย)";
```

- [ ] **Step 2: Write the failing test**

In `src/db/queries.rs`, inside `mod tests`, next to `migration_seeds_todo_done_kind`, add:

```rust
    #[test]
    fn migration_seeds_advance_collected_kind() {
        let conn = mem();
        let kinds = list_activity_kinds(&conn).unwrap();
        assert!(kinds.iter().any(|k| k.name == ADVANCE_COLLECTED_KIND));
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test migration_seeds_advance_collected_kind`
Expected: FAIL — the kind is not seeded yet (`CURRENT_VERSION` is still 8).

- [ ] **Step 4: Bump the version and add the migration**

In `src/db/schema.rs`, change:

```rust
const CURRENT_VERSION: i64 = 8;
```

to:

```rust
const CURRENT_VERSION: i64 = 9;
```

Then add this block immediately after the `if version < 8 { … }` block and before the `if version != CURRENT_VERSION {` line:

```rust
    if version < 9 {
        // Advance payments: money fronted to buy products for a contact, to be
        // collected later. contact_id is nullable + SET NULL so the money record
        // survives if the contact is deleted (the UI requires a contact at entry).
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS advances (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_id     INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
                item           TEXT    NOT NULL,
                amount         INTEGER NOT NULL,
                advance_date   TEXT    NOT NULL,
                note           TEXT    NOT NULL DEFAULT '',
                collected      INTEGER NOT NULL DEFAULT 0,
                collected_at   TEXT,
                collected_note TEXT,
                created_at     TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_advances_contact ON advances(contact_id);",
        )?;
        // Seed the activity kind logged when an advance is collected
        // (activity_kinds is created by the v5 migration above).
        conn.execute(
            "INSERT OR IGNORE INTO activity_kinds (name) VALUES (?1)",
            params![crate::db::queries::ADVANCE_COLLECTED_KIND],
        )?;
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test migration_seeds_advance_collected_kind`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/db/schema.rs src/db/queries.rs
git commit -m "Add advances table and seeded activity kind (migration v9)"
```

---

## Task 2: `Advance` model + `group_thousands` + `advance_note`

**Files:**
- Create: `src/models/advance.rs`
- Modify: `src/models/mod.rs`
- Modify: `src/db/queries.rs` (helpers + tests)

- [ ] **Step 1: Create the model**

Create `src/models/advance.rs`:

```rust
//! An advance payment: money fronted to buy products for a contact, to be
//! collected back later. `collected` is the only status; the collection date
//! and an optional note are recorded when it is collected.

use chrono::{DateTime, Local, NaiveDate};

/// One advance-payment record. `contact_id` is optional at the storage layer
/// (`ON DELETE SET NULL` preserves the money record if the contact is deleted),
/// though the UI requires a contact when creating one. `note` is an optional
/// remark entered at creation; `collected_note` is entered when collecting.
#[derive(Debug, Clone, PartialEq, Eq)]
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

- [ ] **Step 2: Register the module**

In `src/models/mod.rs`, add `pub mod advance;` directly after the `pub mod activity;` line:

```rust
pub mod activity;
pub mod advance;
pub mod contact;
```

- [ ] **Step 3: Write the failing helper tests**

In `src/db/queries.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn group_thousands_formats_with_commas() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(740), "740");
        assert_eq!(group_thousands(1740), "1,740");
        assert_eq!(group_thousands(1234567), "1,234,567");
    }

    #[test]
    fn advance_note_formats_item_amount_and_note() {
        assert_eq!(
            advance_note("Nutrilite โปรตีน", 1740, "โอนผ่านพร้อมเพย์"),
            "Nutrilite โปรตีน — 1,740 บาท — โอนผ่านพร้อมเพย์"
        );
        assert_eq!(advance_note("ของ", 500, "   "), "ของ — 500 บาท");
        assert_eq!(advance_note("ของ", 500, ""), "ของ — 500 บาท");
    }
```

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test group_thousands_formats_with_commas advance_note_formats_item_amount_and_note`
Expected: FAIL to compile — `group_thousands` / `advance_note` are not defined.

- [ ] **Step 5: Implement the helpers**

In `src/db/queries.rs`, near the existing `done_note` function, add:

```rust
/// Format a non-negative integer with comma thousands separators
/// (e.g. `1740 → "1,740"`). Negative numbers keep a leading `-`.
pub fn group_thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Build the activity note for a collected advance: `"<item> — <amount> บาท"`,
/// plus `" — <note>"` when a (trimmed) collection note was entered.
pub fn advance_note(item: &str, amount: i64, note: &str) -> String {
    let base = format!("{item} — {} บาท", group_thousands(amount));
    let note = note.trim();
    if note.is_empty() {
        base
    } else {
        format!("{base} — {note}")
    }
}
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test group_thousands_formats_with_commas advance_note_formats_item_amount_and_note`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/models/advance.rs src/models/mod.rs src/db/queries.rs
git commit -m "Add Advance model and group_thousands/advance_note helpers"
```

---

## Task 3: CRUD queries — `add_advance`, `update_advance`, `delete_advance`, `list_advances`, `outstanding_total`

**Files:**
- Modify: `src/db/queries.rs` (import, `AdvanceRow`, functions, row mapper, tests)

- [ ] **Step 1: Import the model**

In `src/db/queries.rs`, add to the imports block (after `use crate::models::activity::Activity;`):

```rust
use crate::models::advance::Advance;
```

- [ ] **Step 2: Write the failing tests**

In `src/db/queries.rs`, inside `mod tests`, add (note: these use only outstanding rows — the collected dimension is covered in Task 4):

```rust
    #[test]
    fn add_advance_validates_item_and_amount() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ลูกค้า")).unwrap();
        assert!(add_advance(&conn, Some(cid), "   ", 100, d("2026-06-04"), "").is_err());
        assert!(add_advance(&conn, Some(cid), "ของ", 0, d("2026-06-04"), "").is_err());
        assert!(add_advance(&conn, Some(cid), "ของ", -5, d("2026-06-04"), "").is_err());
        assert!(add_advance(&conn, Some(cid), "ของ", 100, d("2026-06-04"), "").is_ok());
    }

    #[test]
    fn add_then_list_round_trips_fields() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ตูน")).unwrap();
        add_advance(&conn, Some(cid), "  Nutrilite  ", 1740, d("2026-06-04"), "  รับของแล้ว  ")
            .unwrap();
        let rows = list_advances(&conn, "", None).unwrap();
        assert_eq!(rows.len(), 1);
        let a = &rows[0].advance;
        assert_eq!(a.item, "Nutrilite"); // trimmed
        assert_eq!(a.amount, 1740);
        assert_eq!(a.advance_date, d("2026-06-04"));
        assert_eq!(a.note, "รับของแล้ว"); // trimmed
        assert!(!a.collected);
        assert_eq!(rows[0].contact_name.as_deref(), Some("ตูน"));
        assert_eq!(rows[0].contact_type, Some(ContactType::Prospect));
    }

    #[test]
    fn list_advances_orders_outstanding_oldest_first() {
        let conn = mem();
        add_advance(&conn, None, "ใหม่กว่า", 200, d("2026-03-01"), "").unwrap();
        add_advance(&conn, None, "เก่าสุด", 100, d("2026-01-01"), "").unwrap();
        add_advance(&conn, None, "กลาง", 150, d("2026-02-01"), "").unwrap();
        let items: Vec<String> =
            list_advances(&conn, "", None).unwrap().into_iter().map(|r| r.advance.item).collect();
        assert_eq!(items, vec!["เก่าสุด", "กลาง", "ใหม่กว่า"]);

        // Substring search on the item text.
        let found = list_advances(&conn, "เก่า", None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].advance.item, "เก่าสุด");
    }

    #[test]
    fn update_and_delete_advance() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("เอ")).unwrap();
        let id = add_advance(&conn, Some(cid), "ของเดิม", 100, d("2026-06-01"), "x").unwrap();

        let mut a = list_advances(&conn, "", None).unwrap()[0].advance.clone();
        a.item = "ของใหม่".into();
        a.amount = 250;
        a.advance_date = d("2026-06-02");
        a.note = "แก้แล้ว".into();
        a.contact_id = None;
        update_advance(&conn, &a).unwrap();

        let rows = list_advances(&conn, "", None).unwrap();
        assert_eq!(rows[0].advance.item, "ของใหม่");
        assert_eq!(rows[0].advance.amount, 250);
        assert_eq!(rows[0].advance.advance_date, d("2026-06-02"));
        assert_eq!(rows[0].advance.note, "แก้แล้ว");
        assert_eq!(rows[0].advance.contact_id, None);

        // Blank item / non-positive amount are rejected on update too.
        let mut bad = rows[0].advance.clone();
        bad.item = "   ".into();
        assert!(update_advance(&conn, &bad).is_err());

        delete_advance(&conn, id).unwrap();
        assert!(list_advances(&conn, "", None).unwrap().is_empty());
    }

    #[test]
    fn outstanding_total_sums_outstanding() {
        let conn = mem();
        assert_eq!(outstanding_total(&conn).unwrap(), 0);
        add_advance(&conn, None, "a", 100, d("2026-06-01"), "").unwrap();
        add_advance(&conn, None, "b", 250, d("2026-06-02"), "").unwrap();
        assert_eq!(outstanding_total(&conn).unwrap(), 350);
    }
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test add_advance_validates_item_and_amount add_then_list_round_trips_fields list_advances_orders_outstanding_oldest_first update_and_delete_advance outstanding_total_sums_outstanding`
Expected: FAIL to compile — the functions / `AdvanceRow` are not defined.

- [ ] **Step 4: Implement `AdvanceRow`, the row mapper, and the functions**

In `src/db/queries.rs`, add (a natural spot is just after the todo query block):

```rust
/// An advance joined with its contact (name + type), for the list view.
pub struct AdvanceRow {
    pub advance: Advance,
    pub contact_name: Option<String>,
    pub contact_type: Option<ContactType>,
}

/// Add an advance; returns the new id. `item` is trimmed and must be non-empty;
/// `amount` must be positive. `note` is the optional create-time remark.
pub fn add_advance(
    conn: &Connection,
    contact_id: Option<i64>,
    item: &str,
    amount: i64,
    advance_date: NaiveDate,
    note: &str,
) -> Result<i64> {
    let item = item.trim();
    if item.is_empty() {
        return Err(AppError::validation("กรุณากรอกรายการสินค้า"));
    }
    if amount <= 0 {
        return Err(AppError::validation("จำนวนเงินต้องมากกว่า 0"));
    }
    conn.execute(
        "INSERT INTO advances (contact_id, item, amount, advance_date, note, collected, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![
            contact_id,
            item,
            amount,
            advance_date.format("%Y-%m-%d").to_string(),
            note.trim(),
            Local::now().to_rfc3339()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update an advance's contact / item / amount / date / note (not the collected
/// fields or `created_at`).
pub fn update_advance(conn: &Connection, a: &Advance) -> Result<()> {
    let item = a.item.trim();
    if item.is_empty() {
        return Err(AppError::validation("กรุณากรอกรายการสินค้า"));
    }
    if a.amount <= 0 {
        return Err(AppError::validation("จำนวนเงินต้องมากกว่า 0"));
    }
    conn.execute(
        "UPDATE advances SET contact_id = ?1, item = ?2, amount = ?3, advance_date = ?4, note = ?5
         WHERE id = ?6",
        params![
            a.contact_id,
            item,
            a.amount,
            a.advance_date.format("%Y-%m-%d").to_string(),
            a.note.trim(),
            a.id
        ],
    )?;
    Ok(())
}

/// Delete an advance.
pub fn delete_advance(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM advances WHERE id = ?1", [id])?;
    Ok(())
}

fn row_to_advance_row(row: &Row) -> rusqlite::Result<AdvanceRow> {
    let advance_date: String = row.get(4)?;
    let collected_at: Option<String> = row.get(7)?;
    let created: String = row.get(9)?;
    let name: Option<String> = row.get(10)?;
    let nickname: Option<String> = row.get(11)?;
    let ctype: Option<String> = row.get(12)?;
    let contact_name = name.map(|n| match nickname {
        Some(nk) if !nk.is_empty() => format!("{n} ({nk})"),
        _ => n,
    });
    Ok(AdvanceRow {
        advance: Advance {
            id: row.get(0)?,
            contact_id: row.get(1)?,
            item: row.get(2)?,
            amount: row.get(3)?,
            advance_date: NaiveDate::parse_from_str(&advance_date, "%Y-%m-%d")
                .unwrap_or_else(|_| Local::now().date_naive()),
            note: row.get(5)?,
            collected: row.get::<_, i64>(6)? != 0,
            collected_at: collected_at
                .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            collected_note: row.get(8)?,
            created_at: parse_dt(&created),
        },
        contact_name,
        contact_type: ctype.map(|s| ContactType::from_db(&s)),
    })
}

/// All advances, joined with their contact, filtered by a substring of the item
/// or contact name/nickname, and by collected status when `collected_filter` is
/// `Some`. Order: outstanding first, then oldest advance date, newest id last.
pub fn list_advances(
    conn: &Connection,
    query: &str,
    collected_filter: Option<bool>,
) -> Result<Vec<AdvanceRow>> {
    let like = format!("%{query}%");
    let mut sql = String::from(
        "SELECT a.id, a.contact_id, a.item, a.amount, a.advance_date, a.note,
                a.collected, a.collected_at, a.collected_note, a.created_at,
                c.name, c.nickname, c.contact_type
         FROM advances a
         LEFT JOIN contacts c ON c.id = a.contact_id
         WHERE (a.item LIKE ?1 OR IFNULL(c.name,'') LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1)",
    );
    if let Some(c) = collected_filter {
        sql.push_str(if c { " AND a.collected = 1" } else { " AND a.collected = 0" });
    }
    sql.push_str(" ORDER BY a.collected ASC, a.advance_date ASC, a.id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![like], |row| row_to_advance_row(row))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Total baht of all outstanding (uncollected) advances.
pub fn outstanding_total(conn: &Connection) -> Result<i64> {
    let total: i64 = conn.query_row(
        "SELECT IFNULL(SUM(amount), 0) FROM advances WHERE collected = 0",
        [],
        |r| r.get(0),
    )?;
    Ok(total)
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test add_advance_validates_item_and_amount add_then_list_round_trips_fields list_advances_orders_outstanding_oldest_first update_and_delete_advance outstanding_total_sums_outstanding`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add src/db/queries.rs
git commit -m "Add advance CRUD queries (add/update/delete/list/outstanding_total)"
```

---

## Task 4: `collect_advance` — mark collected and log the activity

**Files:**
- Modify: `src/db/queries.rs` (function + tests)

- [ ] **Step 1: Write the failing tests**

In `src/db/queries.rs`, inside `mod tests`, add:

```rust
    #[test]
    fn collect_advance_logs_activity_for_contact() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ธนา")).unwrap();
        let aid =
            add_advance(&conn, Some(cid), "Nutrilite โปรตีน", 1740, d("2026-06-01"), "").unwrap();

        collect_advance(&conn, aid, d("2026-06-05"), "โอนผ่านพร้อมเพย์").unwrap();

        let rows = list_advances(&conn, "", None).unwrap();
        let a = &rows.iter().find(|r| r.advance.id == aid).unwrap().advance;
        assert!(a.collected);
        assert_eq!(a.collected_at, Some(d("2026-06-05")));

        let acts = list_activities(&conn, cid).unwrap();
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, ADVANCE_COLLECTED_KIND);
        assert_eq!(acts[0].note, "Nutrilite โปรตีน — 1,740 บาท — โอนผ่านพร้อมเพย์");
    }

    #[test]
    fn collect_advance_without_contact_does_not_log() {
        let conn = mem();
        let aid = add_advance(&conn, None, "ของส่วนตัว", 500, d("2026-06-01"), "").unwrap();

        collect_advance(&conn, aid, d("2026-06-02"), "").unwrap();

        assert!(list_advances(&conn, "", None).unwrap()[0].advance.collected);
        assert_eq!(list_all_activities(&conn, "").unwrap().len(), 0);
    }

    #[test]
    fn collect_advance_excluded_from_outstanding() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("มานี")).unwrap();
        add_advance(&conn, Some(cid), "ค้าง", 200, d("2026-06-01"), "").unwrap();
        let paid = add_advance(&conn, Some(cid), "จ่ายแล้ว", 800, d("2026-06-01"), "").unwrap();
        collect_advance(&conn, paid, d("2026-06-03"), "").unwrap();

        assert_eq!(outstanding_total(&conn).unwrap(), 200);
        assert_eq!(list_advances(&conn, "", Some(false)).unwrap().len(), 1);
        assert_eq!(list_advances(&conn, "", Some(true)).unwrap().len(), 1);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test collect_advance`
Expected: FAIL to compile — `collect_advance` is not defined.

- [ ] **Step 3: Implement `collect_advance`**

In `src/db/queries.rs`, add after `outstanding_total`:

```rust
/// Mark an advance collected and, when it is tied to a contact, log an
/// `ADVANCE_COLLECTED_KIND` activity with `advance_note(item, amount, note)` as
/// its detail — both in one transaction. The activity timestamp uses
/// `collected_date` (at the current local time) so it lands on the right day in
/// the history. A contactless advance is still marked collected, with no activity.
pub fn collect_advance(
    conn: &Connection,
    id: i64,
    collected_date: NaiveDate,
    note: &str,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let note = note.trim();
    tx.execute(
        "UPDATE advances SET collected = 1, collected_at = ?1, collected_note = ?2 WHERE id = ?3",
        params![collected_date.format("%Y-%m-%d").to_string(), note, id],
    )?;
    let row: Option<(Option<i64>, String, i64)> = tx
        .query_row(
            "SELECT contact_id, item, amount FROM advances WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((Some(contact_id), item, amount)) = row {
        let created_at = collected_date
            .and_time(Local::now().time())
            .and_local_timezone(Local)
            .single()
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| Local::now().to_rfc3339());
        tx.execute(
            "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                contact_id,
                ADVANCE_COLLECTED_KIND,
                advance_note(&item, amount, note),
                created_at
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test collect_advance`
Expected: PASS (3 tests).

- [ ] **Step 5: Run the whole suite**

Run: `cargo test`
Expected: all pass (39 existing + 11 new advance tests = 50).

- [ ] **Step 6: Commit**

```bash
git add src/db/queries.rs
git commit -m "Add collect_advance: mark collected and log the activity in one transaction"
```

---

## Task 5: `DbConnection` passthroughs

**Files:**
- Modify: `src/db/mod.rs`

- [ ] **Step 1: Import `Advance` and `AdvanceRow`**

In `src/db/mod.rs`, add after `use crate::models::activity::Activity;`:

```rust
use crate::models::advance::Advance;
```

And add `AdvanceRow` to the existing `use queries::{…}` line so it reads:

```rust
use queries::{AboRow, ActivityKindRow, ActivityLogRow, AdvanceRow, CustomerRow, ProspectRow, TodoRow};
```

- [ ] **Step 2: Add the passthrough methods**

In `src/db/mod.rs`, immediately after the `count_due_soon_todos` method (the end of the todos section), add:

```rust
    // --- advances ---------------------------------------------------------

    pub fn add_advance(
        &self,
        contact_id: Option<i64>,
        item: &str,
        amount: i64,
        advance_date: NaiveDate,
        note: &str,
    ) -> Result<i64> {
        queries::add_advance(&self.conn, contact_id, item, amount, advance_date, note)
    }
    pub fn update_advance(&self, a: &Advance) -> Result<()> {
        queries::update_advance(&self.conn, a)
    }
    pub fn collect_advance(&self, id: i64, collected_date: NaiveDate, note: &str) -> Result<()> {
        queries::collect_advance(&self.conn, id, collected_date, note)
    }
    pub fn delete_advance(&self, id: i64) -> Result<()> {
        queries::delete_advance(&self.conn, id)
    }
    pub fn list_advances(
        &self,
        query: &str,
        collected_filter: Option<bool>,
    ) -> Result<Vec<AdvanceRow>> {
        queries::list_advances(&self.conn, query, collected_filter)
    }
    pub fn outstanding_total(&self) -> Result<i64> {
        queries::outstanding_total(&self.conn)
    }
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build`
Expected: compiles with no errors and no warnings.

- [ ] **Step 4: Commit**

```bash
git add src/db/mod.rs
git commit -m "Expose advance operations on DbConnection"
```

---

## Task 6: Collect dialog + delete wiring + collect/delete app state

This task leaves the build green even though nothing opens the dialog yet (the list page in Task 7 sets `pending_advance_collect`).

**Files:**
- Create: `src/ui/advance_collect.rs`
- Modify: `src/ui/mod.rs` (`pub mod advance_collect;`)
- Modify: `src/ui/confirm.rs` (`PendingDelete::Advance` + arms)
- Modify: `src/app.rs` (state fields, initializers, modal dispatch)

- [ ] **Step 1: Create the collect dialog**

Create `src/ui/advance_collect.rs`:

```rust
//! "Collect payment" modal shown when an advance is marked collected.
//!
//! Clicking "เก็บเงิน" on an outstanding advance sets
//! `AppState.pending_advance_collect` instead of collecting immediately; this
//! modal collects the real collection date and an optional note and, on
//! "บันทึก", calls `collect_advance` (which marks it collected and logs the
//! activity). Cancelling leaves the advance outstanding.

use egui_extras::DatePickerButton;

use crate::app::AppState;
use crate::db::queries::group_thousands;

/// An advance awaiting its collection date + note. Set by clicking "เก็บเงิน";
/// consumed by [`render`].
#[derive(Clone)]
pub struct PendingAdvanceCollect {
    pub id: i64,
    pub item: String,
    pub amount: i64,
    pub contact_name: String,
}

pub fn render(app: &mut AppState, ctx: &egui::Context) {
    let Some(pending) = app.pending_advance_collect.clone() else {
        return;
    };

    let mut save = false;
    let mut cancel = false;
    let mut open = true;

    egui::Window::new("เก็บเงินค่าสินค้า / Collect Payment")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&pending.item).strong());
            ui.label(
                egui::RichText::new(format!(
                    "{} บาท · ของ: {}",
                    group_thousands(pending.amount),
                    pending.contact_name
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("วันที่เก็บ:");
                ui.add(
                    DatePickerButton::new(&mut app.advance_collect_date)
                        .id_source("advance_collect_picker"),
                );
            });
            ui.add_space(6.0);
            ui.label("หมายเหตุ:");
            ui.add(
                egui::TextEdit::multiline(&mut app.advance_collect_note)
                    .hint_text("เช่น โอนผ่านพร้อมเพย์ (ไม่บังคับ)")
                    .desired_width(360.0)
                    .desired_rows(2),
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
        match app
            .db
            .collect_advance(pending.id, app.advance_collect_date, &app.advance_collect_note)
        {
            Ok(()) => {
                app.set_status(format!("บันทึกการเก็บเงินจาก {} แล้ว", pending.contact_name));
                // Clear only on success — on error keep the dialog open with the
                // typed values preserved so the user can retry.
                app.pending_advance_collect = None;
                app.advance_collect_note.clear();
            }
            Err(e) => app.set_error(e),
        }
    } else if cancel || !open {
        app.pending_advance_collect = None;
        app.advance_collect_note.clear();
    }
}
```

- [ ] **Step 2: Register the module**

In `src/ui/mod.rs`, add `pub mod advance_collect;` directly after `pub mod activity_log;`:

```rust
pub mod activity_log;
pub mod advance_collect;
pub mod confirm;
```

- [ ] **Step 3: Add the `PendingDelete::Advance` variant**

In `src/ui/confirm.rs`, add the variant to the enum:

```rust
    Todo { id: i64, name: String },
    Advance { id: i64, item: String },
}
```

Add its `(name, detail)` arm (after the `PendingDelete::Todo` arm):

```rust
        PendingDelete::Advance { item, .. } => {
            (item.clone(), "รายการสำรองจ่ายนี้จะถูกลบถาวร".to_string())
        }
```

Add its delete arm in the `result` match (after the `PendingDelete::Todo` arm):

```rust
            PendingDelete::Advance { id, .. } => app.db.delete_advance(*id),
```

- [ ] **Step 4: Add app-state fields**

In `src/app.rs`, in the `AppState` struct, add directly after the `pub todo_done_result: String,` field:

```rust
    /// An advance whose collect-action is awaiting its date + note (drives the
    /// `ui::advance_collect` modal); `None` when no collect dialog is open.
    pub pending_advance_collect: Option<crate::ui::advance_collect::PendingAdvanceCollect>,
    /// Collection-date and note buffers for the advance-collect dialog.
    pub advance_collect_date: chrono::NaiveDate,
    pub advance_collect_note: String,
```

- [ ] **Step 5: Add the initializers**

In `src/app.rs`, in `AppState::new`, add directly after the `todo_done_result: String::new(),` line:

```rust
            pending_advance_collect: None,
            advance_collect_date: chrono::Local::now().date_naive(),
            advance_collect_note: String::new(),
```

- [ ] **Step 6: Dispatch the modal**

In `src/app.rs`, in the `update` method's modal section, add directly after the `ui::todo_done::render(self, ctx);` line:

```rust
        ui::advance_collect::render(self, ctx);
```

- [ ] **Step 7: Verify it builds**

Run: `cargo build`
Expected: compiles, no errors, no warnings.

- [ ] **Step 8: Commit**

```bash
git add src/ui/advance_collect.rs src/ui/mod.rs src/ui/confirm.rs src/app.rs
git commit -m "Add advance collect dialog and delete wiring"
```

---

## Task 7: List page + menu + view dispatch (feature complete)

**Files:**
- Create: `src/ui/advances.rs`
- Modify: `src/ui/mod.rs` (`pub mod advances;` + `View::Advances`)
- Modify: `src/app.rs` (form/filter fields, initializers, sidebar item, view dispatch)

- [ ] **Step 1: Create the list page**

Create `src/ui/advances.rs`:

```rust
//! Advance Payments (สำรองจ่าย): money fronted to buy products for a contact, to
//! be collected later. Add/edit via a form group on the left, search/filter on
//! the right; the table shows outstanding/collected status, a per-row "เก็บเงิน"
//! action (which logs to the contact's activity history), and edit/delete.
//! Outstanding rows are listed first, oldest advance date first.

use chrono::{Local, NaiveDate};
use egui_extras::{Column, DatePickerButton, TableBuilder};

use crate::app::AppState;
use crate::db::queries::group_thousands;
use crate::models::advance::Advance;
use crate::models::enums::ContactType;
use crate::ui::advance_collect::PendingAdvanceCollect;
use crate::ui::confirm::PendingDelete;
use crate::ui::{filter_combo, ACCENT, ACCENT_STRONG};

/// Width of the fixed label column in each form/filter row.
const LABEL_W: f32 = 110.0;

/// One labelled form row: a fixed-width label cell, then the field widget.
/// (Mirrors the helper of the same name in `ui/todo.rs`.)
fn field_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_W, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(label);
            },
        );
        add(ui);
    });
}

/// Add/edit form state for the Advance Payments page.
pub struct AdvanceForm {
    /// `Some(id)` when editing an existing advance; `None` when adding.
    pub editing_id: Option<i64>,
    pub contact_id: Option<i64>,
    pub contact_filter: String,
    pub item: String,
    pub amount: i64,
    pub advance_date: NaiveDate,
    pub note: String,
}

impl Default for AdvanceForm {
    fn default() -> Self {
        AdvanceForm {
            editing_id: None,
            contact_id: None,
            contact_filter: String::new(),
            item: String::new(),
            amount: 0,
            advance_date: Local::now().date_naive(),
            note: String::new(),
        }
    }
}

impl AdvanceForm {
    fn reset(&mut self) {
        *self = AdvanceForm::default();
    }
}

/// Status filter on the Advance Payments page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AdvanceStatusFilter {
    Outstanding,
    Collected,
    All,
}

impl AdvanceStatusFilter {
    const ALL: [AdvanceStatusFilter; 3] = [
        AdvanceStatusFilter::Outstanding,
        AdvanceStatusFilter::Collected,
        AdvanceStatusFilter::All,
    ];
    fn label(self) -> &'static str {
        match self {
            AdvanceStatusFilter::Outstanding => "รอเก็บเงิน",
            AdvanceStatusFilter::Collected => "เก็บเงินแล้ว",
            AdvanceStatusFilter::All => "ทั้งหมด",
        }
    }
    /// The `collected_filter` argument for `list_advances`.
    fn as_filter(self) -> Option<bool> {
        match self {
            AdvanceStatusFilter::Outstanding => Some(false),
            AdvanceStatusFilter::Collected => Some(true),
            AdvanceStatusFilter::All => None,
        }
    }
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("สำรองจ่าย / Advance Payments");
    ui.label(
        egui::RichText::new("เงินที่จ่ายล่วงหน้าซื้อสินค้าให้รายชื่อ แล้วรอเก็บคืนภายหลัง")
            .weak()
            .small(),
    );
    ui.add_space(8.0);

    // Contacts for the picker (all types), pre-fetched so the combo closure does
    // not borrow app.db while mutating app.advance_form.
    let contacts = app.db.list_contacts().unwrap_or_default();
    let contact_options: Vec<(i64, String)> = contacts
        .iter()
        .map(|c| (c.id, format!("{} · {}", c.display_name(), c.contact_type.label_th())))
        .collect();

    let mut submit = false;
    let mut cancel_edit = false;
    let editing = app.advance_form.editing_id.is_some();

    ui.columns(2, |cols| {
        let field_w = (cols[0].available_width() - LABEL_W - 40.0).max(60.0);
        let search_field_w = (field_w - 60.0).max(60.0);

        // Left card: add / edit form.
        let c0 = &mut cols[0];
        egui::Frame::group(c0.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c0, |ui| {
                let f = &mut app.advance_form;
                ui.label(
                    egui::RichText::new(if editing {
                        "✏ แก้ไขรายการ"
                    } else {
                        "➕ เพิ่มรายการสำรองจ่าย"
                    })
                    .color(ACCENT_STRONG)
                    .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "รายชื่อ", |ui| {
                    filter_combo(
                        ui,
                        "advance_contact_cb",
                        &mut f.contact_id,
                        &mut f.contact_filter,
                        Some("— เลือกรายชื่อ —"),
                        &contact_options,
                        field_w,
                    );
                });
                field_row(ui, "รายการสินค้า", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.item)
                            .hint_text("เช่น Nutrilite โปรตีน 2 กระปุก")
                            .desired_width(field_w),
                    );
                });
                field_row(ui, "จำนวนเงิน (บาท)", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut f.amount)
                            .range(0..=99_999_999)
                            .suffix(" บาท"),
                    );
                });
                field_row(ui, "วันที่จ่าย", |ui| {
                    ui.add(
                        DatePickerButton::new(&mut f.advance_date)
                            .id_source("advance_date_picker"),
                    );
                });
                field_row(ui, "หมายเหตุ", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut f.note)
                            .hint_text("ไม่บังคับ เช่น รับของที่ร้านแล้ว")
                            .desired_width(field_w),
                    );
                });

                ui.add_space(8.0);
                field_row(ui, "", |ui| {
                    if editing {
                        if ui.add(egui::Button::new("💾 บันทึก").fill(ACCENT)).clicked() {
                            submit = true;
                        }
                        if ui.button("ยกเลิก").clicked() {
                            cancel_edit = true;
                        }
                    } else if ui.add(egui::Button::new("➕ เพิ่ม").fill(ACCENT)).clicked() {
                        submit = true;
                    }
                });
            });

        // Right card: search + status filter.
        let c1 = &mut cols[1];
        egui::Frame::group(c1.style())
            .rounding(8.0)
            .inner_margin(12.0)
            .show(c1, |ui| {
                ui.label(
                    egui::RichText::new("🔍 ค้นหา / กรอง")
                        .color(ACCENT_STRONG)
                        .strong(),
                );
                ui.add_space(6.0);

                field_row(ui, "ค้นหา", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.search)
                            .hint_text("สินค้า / ชื่อ")
                            .desired_width(search_field_w),
                    );
                    if ui.button("ล้าง").clicked() {
                        app.search.clear();
                    }
                });
                field_row(ui, "สถานะ", |ui| {
                    egui::ComboBox::from_id_source("advance_status_cb")
                        .width(field_w)
                        .selected_text(app.advance_status_filter.label())
                        .show_ui(ui, |ui| {
                            for s in AdvanceStatusFilter::ALL {
                                ui.selectable_value(&mut app.advance_status_filter, s, s.label());
                            }
                        });
                });
            });
    });

    ui.add_space(6.0);

    // --- load rows (status filter applied in SQL) ---
    let filter = app.advance_status_filter.as_filter();
    let r = app.db.list_advances(&app.search, filter);
    let rows = app.handle(r, Vec::new());

    // Outstanding total comes from the DB (ALL outstanding rows, regardless of
    // the current status filter); rows.len() is what the filter currently shows.
    let rt = app.db.outstanding_total();
    let out_total = app.handle(rt, 0);
    ui.label(
        egui::RichText::new(format!(
            "ยอดรอเก็บรวมทั้งหมด: {} บาท • แสดง {} รายการ",
            group_thousands(out_total),
            rows.len()
        ))
        .small()
        .weak(),
    );
    ui.add_space(4.0);

    if rows.is_empty() {
        ui.weak("— ไม่มีรายการในตัวกรองนี้ —");
        apply_form(app, submit, cancel_edit);
        return;
    }

    let mut collect_req: Option<i64> = None;
    let mut edit_req: Option<i64> = None;
    let mut delete_req: Option<(i64, String)> = None;
    let collected_color = egui::Color32::from_rgb(0x2E, 0x7D, 0x32);
    let outstanding_color = egui::Color32::from_rgb(0xB2, 0x6A, 0x00);

    TableBuilder::new(ui)
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::auto().at_least(100.0)) // วันที่จ่าย
        .column(Column::auto().at_least(140.0)) // ชื่อ
        .column(Column::remainder().at_least(160.0)) // รายการสินค้า
        .column(Column::auto().at_least(90.0)) // จำนวนเงิน
        .column(Column::auto().at_least(120.0)) // สถานะ
        .column(Column::auto()) // จัดการ
        .header(28.0, |mut header| {
            for h in ["วันที่จ่าย", "ชื่อ", "รายการสินค้า", "จำนวนเงิน", "สถานะ", "จัดการ"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|mut body| {
            for row in &rows {
                body.row(30.0, |mut tr| {
                    tr.col(|ui| {
                        ui.label(
                            egui::RichText::new(
                                row.advance.advance_date.format("%Y-%m-%d").to_string(),
                            )
                            .small()
                            .weak(),
                        );
                    });
                    tr.col(|ui| match (&row.contact_name, row.contact_type) {
                        (Some(name), Some(ty)) => {
                            let color = match ty {
                                ContactType::Prospect => egui::Color32::from_rgb(0xB2, 0x6A, 0x00),
                                ContactType::Customer => egui::Color32::from_rgb(0x2E, 0x7D, 0x32),
                                ContactType::Abo => ACCENT_STRONG,
                            };
                            ui.label(egui::RichText::new(name.as_str()).color(color));
                        }
                        _ => {
                            ui.weak("—");
                        }
                    });
                    tr.col(|ui| {
                        let resp = ui.label(&row.advance.item);
                        if !row.advance.note.is_empty() {
                            resp.on_hover_text(&row.advance.note);
                        }
                    });
                    tr.col(|ui| {
                        ui.label(format!("{} บาท", group_thousands(row.advance.amount)));
                    });
                    tr.col(|ui| {
                        if row.advance.collected {
                            let when = row
                                .advance
                                .collected_at
                                .map(|d| d.format("%Y-%m-%d").to_string())
                                .unwrap_or_default();
                            ui.label(
                                egui::RichText::new(format!("✅ เก็บแล้ว {when}"))
                                    .small()
                                    .color(collected_color),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("รอเก็บเงิน").small().color(outstanding_color),
                            );
                        }
                    });
                    tr.col(|ui| {
                        if !row.advance.collected
                            && ui.small_button("เก็บเงิน").on_hover_text("บันทึกการเก็บเงิน").clicked()
                        {
                            collect_req = Some(row.advance.id);
                        }
                        if !row.advance.collected
                            && ui.small_button("✏").on_hover_text("แก้ไข").clicked()
                        {
                            edit_req = Some(row.advance.id);
                        }
                        if ui.small_button("🗑").on_hover_text("ลบ").clicked() {
                            delete_req = Some((row.advance.id, row.advance.item.clone()));
                        }
                    });
                });
            }
        });

    // --- apply deferred row actions ---
    if let Some(id) = collect_req {
        if let Some(row) = rows.iter().find(|r| r.advance.id == id) {
            match (row.advance.contact_id, &row.contact_name) {
                (Some(_), Some(name)) => {
                    app.pending_advance_collect = Some(PendingAdvanceCollect {
                        id,
                        item: row.advance.item.clone(),
                        amount: row.advance.amount,
                        contact_name: name.clone(),
                    });
                    app.advance_collect_date = Local::now().date_naive();
                    app.advance_collect_note.clear();
                }
                _ => {
                    // Orphaned (contact deleted): collect now, nothing to log.
                    if let Err(e) = app.db.collect_advance(id, Local::now().date_naive(), "") {
                        app.set_error(e);
                    } else {
                        app.set_status(
                            "ทำเครื่องหมายเก็บเงินแล้ว — รายการนี้ไม่มีรายชื่อ จึงไม่บันทึกลงประวัติ",
                        );
                    }
                }
            }
        }
    }
    if let Some(id) = edit_req {
        if let Some(row) = rows.iter().find(|r| r.advance.id == id) {
            app.advance_form = AdvanceForm {
                editing_id: Some(row.advance.id),
                contact_id: row.advance.contact_id,
                contact_filter: String::new(),
                item: row.advance.item.clone(),
                amount: row.advance.amount,
                advance_date: row.advance.advance_date,
                note: row.advance.note.clone(),
            };
        }
    }
    if let Some((id, item)) = delete_req {
        app.pending_delete = Some(PendingDelete::Advance { id, item });
    }

    apply_form(app, submit, cancel_edit);
}

/// Apply the add/edit form's submit or cancel (factored out so it runs whether or
/// not the table was drawn). Contact is required.
fn apply_form(app: &mut AppState, submit: bool, cancel_edit: bool) {
    if cancel_edit {
        app.advance_form.reset();
    }
    if !submit {
        return;
    }
    if app.advance_form.contact_id.is_none() {
        app.set_error("กรุณาเลือกรายชื่อ");
        return;
    }
    let editing = app.advance_form.editing_id;
    let result = match editing {
        Some(id) => {
            let a = Advance {
                id,
                contact_id: app.advance_form.contact_id,
                item: app.advance_form.item.clone(),
                amount: app.advance_form.amount,
                advance_date: app.advance_form.advance_date,
                note: app.advance_form.note.clone(),
                // update_advance writes only contact/item/amount/date/note;
                // these are placeholders it ignores.
                collected: false,
                collected_at: None,
                collected_note: None,
                created_at: Local::now(),
            };
            app.db.update_advance(&a)
        }
        None => app
            .db
            .add_advance(
                app.advance_form.contact_id,
                &app.advance_form.item,
                app.advance_form.amount,
                app.advance_form.advance_date,
                &app.advance_form.note,
            )
            .map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.advance_form.reset();
            app.set_status(if editing.is_some() { "บันทึกรายการแล้ว" } else { "เพิ่มรายการแล้ว" });
        }
        Err(e) => app.set_error(e),
    }
}
```

- [ ] **Step 2: Register the module and add the `View` variant**

In `src/ui/mod.rs`, add `pub mod advances;` directly after the `pub mod advance_collect;` line added in Task 6:

```rust
pub mod advance_collect;
pub mod advances;
pub mod confirm;
```

And add `Advances` to the `View` enum, after `Todos`:

```rust
    Todos,
    Advances,
    Network,
```

- [ ] **Step 3: Add the form/filter app-state fields**

In `src/app.rs`, in the `AppState` struct, add directly after the `pub advance_collect_note: String,` field (added in Task 6):

```rust
    /// Advance Payments add/edit form state.
    pub advance_form: crate::ui::advances::AdvanceForm,
    /// Status filter on the Advance Payments page.
    pub advance_status_filter: crate::ui::advances::AdvanceStatusFilter,
```

- [ ] **Step 4: Add their initializers**

In `src/app.rs`, in `AppState::new`, add directly after the `advance_collect_note: String::new(),` line (added in Task 6):

```rust
            advance_form: crate::ui::advances::AdvanceForm::default(),
            advance_status_filter: crate::ui::advances::AdvanceStatusFilter::Outstanding,
```

- [ ] **Step 5: Add the sidebar menu item**

In `src/app.rs`, in the `sidebar` method's `items` array, add the entry directly after the `(View::Todos, "📅  สิ่งที่ต้องทำ"),` line:

```rust
            (View::Advances, "💵  สำรองจ่าย"),
```

- [ ] **Step 6: Add the central-panel dispatch**

In `src/app.rs`, in `update`, in the `match self.view` block, add the arm directly after the `View::Todos => ui::todo::render(self, ui),` line:

```rust
            View::Advances => ui::advances::render(self, ui),
```

- [ ] **Step 7: Build and run the full suite**

Run: `cargo build`
Expected: compiles, no errors, no warnings.

Run: `cargo test`
Expected: all pass (50 tests).

- [ ] **Step 8: Boot smoke + glyph check**

Launch the app and confirm it starts and migration v9 applies cleanly:

```bash
cargo run
```

Then verify visually (per the [[egui-emoji-glyph-subset]] memory and the egui-app-screenshot-verify workflow): the **💵** glyph in the "💵  สำรองจ่าย" sidebar item must render as a banknote, not a tofu box. If it is tofu, replace the `💵  ` prefix in the sidebar item (Task 7 Step 5) with a glyph confirmed present in the bundled font (the app already renders `📋`, `📅`, `📝`, `📦`-style icons) and re-run. The collect/edit/delete controls use text + the already-proven `✏` / `🗑` glyphs, so no other glyph risk.

- [ ] **Step 9: Commit**

```bash
git add src/ui/advances.rs src/ui/mod.rs src/app.rs
git commit -m "Add the สำรองจ่าย (Advance Payments) page and menu"
```

---

## Self-Review (completed by plan author)

**Spec coverage:** Every spec section maps to a task — migration+kind (T1), model+helpers (T2), CRUD+list+total (T3), collect+logging (T4), DbConnection facade (T5), collect dialog+delete+state (T6), page+menu+dispatch (T7). The two-notes distinction (`note` vs `collected_note`) is realized in the DDL (T1), model (T2), `add_advance`/`update_advance` (T3), and the form/hover (T7). Activity timestamp = collection date (T4). contact SET NULL + contactless-collect-no-log (T1 DDL, T4 query, T7 orphan branch). No un-collect (only `collect_advance`, no reverse). Outstanding-total summary (T3 query, T7 header).

**Placeholder scan:** No TBD/TODO/"similar to"/"add error handling" — every code step shows complete code and exact commands.

**Type consistency:** `add_advance(contact_id, item, amount, advance_date, note)` is identical in T3 (query), T5 (passthrough), and T7 (caller). `collect_advance(id, collected_date, note)` identical in T4/T5/T6. `AdvanceForm`/`AdvanceStatusFilter`/`PendingAdvanceCollect`/`PendingDelete::Advance` are defined once and referenced consistently. `list_advances(query, Option<bool>)` consistent. Column indices in `row_to_advance_row` (0–12) match the `SELECT` list in `list_advances`.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-04-advance-payments.md`. Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, two-stage review (spec then quality) between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session with checkpoints.

Which approach?
