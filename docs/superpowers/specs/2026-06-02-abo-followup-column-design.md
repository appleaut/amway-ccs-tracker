# ABO List: "% การติดตาม" Column — Design Spec

**Date:** 2026-06-02
**Scope:** Replace the **เบอร์โทร (phone)** column on the นักธุรกิจ / ABO page with a
**% การติดตาม** column showing each ABO's Follow-Up Sheet completion as a mini
progress bar. Touches the ABO list query/row and `src/ui/abo_list.rs`.

## Decision

Follow-up completion = checked items / 26 from `follow_up_sheets` (the same data
the Follow-Up page shows). Display as a mini progress bar + `NN%` (option ก).

## Design

**1. Data (`src/db/queries.rs`)**
- Add `followup_done: i64` to `AboRow` (0..=26).
- `list_abo_rows` gains `LEFT JOIN follow_up_sheets fs ON fs.contact_id = c.id`
  and selects `COALESCE(<sum of the 26 boolean columns>, 0)` as the done count
  (NULL → 0 when an ABO has no sheet yet). Mapped from column index 18.
- New test `abo_rows_include_followup_done`: 0 with no sheet; equals the number of
  checked items after saving a sheet with 3 items ticked.

**2. View (`src/ui/abo_list.rs`)**
- Header label `เบอร์โทร` → `% การติดตาม` (still column index 1, sortable).
- Cell: `egui::ProgressBar::new(done / FollowUpSheet::TOTAL)` with
  `.desired_width(110.0).text("NN%")` — same look as the Follow-Up page bar.
- The column-1 sort comparator changes from `contact.phone` to `followup_done`.
- Search still matches phone (query unchanged); the search box stays as is.

## Non-goals

- No change to other columns, the Follow-Up page, or the underlying schema.
- Phone is only removed from the ABO table display, not from the data or search.

## Verification

- `cargo test` (incl. the new test) + `cargo check` clean.
- Visual (`cargo run`): the ABO table shows a "% การติดตาม" bar per row; ABOs with a
  partly-completed sheet show the right percentage; sorting by that column orders
  by completion.
