# Todo Search/Filter Layout — Design Spec

**Date:** 2026-06-02
**Scope:** Presentational only. Lay out the add/edit form and the search/filter
controls on the Todo List page (`src/ui/todo.rs`). No DB / model / query / test /
behaviour changes — same controls wired to the same state (`app.search`,
`app.todo_status_filter`, `app.todo_who_filter`, `app.todo_form`).

## Problem

The search/filter controls started as a bare single horizontal row that clipped
on narrow windows and looked inconsistent next to the bordered "เพิ่มงานใหม่" form.
A first pass wrapped them in a titled group, but testing surfaced two issues:
the search box / "ล้าง" button / dropdowns did not line up with each other, and
the search panel stacked *below* the form instead of sitting beside it.

## Design (final)

Lay the form and the search/filter panel out as **two equal, side-by-side
cards**, and give the search card the **same aligned label/field rows** as the
form so its controls line up.

```
┌─ ➕ เพิ่มงานใหม่ ──────────────┐   ┌─ 🔍 ค้นหา / กรอง ─────────────┐
│ สิ่งที่ต้องทำ [______________] │   │ ค้นหา   [____________] [ล้าง] │
│ เกี่ยวกับ    [— ไม่ระบุ — ▼] │   │ สถานะ   [ ยังไม่เสร็จ    ▼ ] │
│ กำหนดส่ง   [✓ มีกำหนดส่ง] 📅 │   │ ของ     [ ทั้งหมด        ▼ ] │
│                  [ ➕ เพิ่ม ] │   │                              │
└──────────────────────────────┘   └──────────────────────────────┘
   ทั้งหมด N รายการ
   <table>
```

- **Two equal columns** via `ui.columns(2, ...)` — form card left, search/filter
  card right: equal width, adjacent.
- Each card is an `egui::Frame::group` (rounding 8.0, inner_margin 12.0) with a
  strong `ACCENT_STRONG` title (`➕ เพิ่มงานใหม่` / `✏ แก้ไขงาน`; `🔍 ค้นหา / กรอง`).
- **Both cards use the shared `field_row(label, widget)` helper** (a fixed
  `LABEL_W` label column, then the widget) so every control sits on its own
  vertically-centred row and lines up. Search-card rows: `ค้นหา` (text + ล้าง),
  `สถานะ` (combo), `ของ` (combo). This is what fixes the misalignment.
- **Fields are sized from the real column width** (`cols[i].available_width()`
  minus the label column and frame margins), so each card fills its column
  *without overflowing it* — measuring availability deeper inside the nested
  frame/rows over-reports and pushes the card past the screen edge. The earlier
  fixed `PANEL_W` / `FIELD_W` constants are removed; only `LABEL_W` remains.
- The result count (`ทั้งหมด N รายการ`) and the table below are unchanged.

## Non-goals

- No change to filter semantics, the table, or any state/behaviour.
- Cards always sit side by side (columns split the available width); they do not
  stack on a very narrow window.

## Verification

- `cargo check` clean; `cargo test` still 33 passing (no logic touched).
- Visual (`cargo run`): two equal cards side by side; inside the search card the
  text box, the "ล้าง" button, and both dropdowns line up on their rows; resizing
  the window keeps them side by side with the fields adapting.
