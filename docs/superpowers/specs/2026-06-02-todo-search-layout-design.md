# Todo Search/Filter Layout — Design Spec

**Date:** 2026-06-02
**Scope:** Presentational only. Restructure the search/filter bar on the Todo
List page (`src/ui/todo.rs`, the "filter row" block). No DB / model / query /
test / behaviour changes — the same search box and two filter dropdowns, wired
to the same state (`app.search`, `app.todo_status_filter`, `app.todo_who_filter`).

## Problem

The search/filter controls sit in a bare single `ui.horizontal` row
(🔍 + search + ล้าง + status combo + who combo). It uses plain `horizontal`, so on
a narrow window it clips/overflows instead of wrapping, and it looks inconsistent
next to the new bordered "เพิ่มงานใหม่" form group above it.

## Design (approved: "Approach 1 — matching titled group + horizontal_wrapped")

Wrap the controls in a bordered, titled group matching the form group, and let
them wrap responsively.

```
┌─ 🔍 ค้นหา / กรอง ──────────────────────────────┐
│ [🔍 ค้นหา งาน / ชื่อ............] [ล้าง]            │
│ สถานะ [ ยังไม่เสร็จ ▼ ]   ของ [ ทั้งหมด ▼ ]        │   narrow window: each group wraps as a unit
└──────────────────────────────────────────────────┘
   ทั้งหมด N รายการ
   <table>
```

- **Shared block width:** rename the existing `FORM_W` constant to `PANEL_W`
  (= 460.0) and use it for both the form group and this search/filter group so
  the two panels line up.
- **Group:** `egui::Frame::group(ui.style()).rounding(8.0).inner_margin(12.0)`,
  `ui.set_max_width(PANEL_W)`, title `🔍 ค้นหา / กรอง` (ACCENT_STRONG, strong).
- **Body:** `ui.horizontal_wrapped`, containing three units — each a nested
  `ui.horizontal` so a label stays glued to its control when it wraps:
  1. `[🔍] [search TextEdit ~220] [ล้าง]`
  2. `สถานะ: [status combo]`
  3. `ของ: [who combo]`
  with a small `add_space(8.0)` between units.
- Search box widened 200 → 220.
- **Count line** (`ทั้งหมด N รายการ`) stays in its current position below the group
  (it is computed after the filters are read; moving it into the group header
  would lag the filters by one frame).

## Non-goals

- No change to the form group, the table, filter semantics, or any state.
- Not moving the count into the group; not adding new filters.

## Verification

- `cargo build` clean; `cargo test` still 33 passing (no logic touched).
- Visual (`cargo run`): the search/filter area is a bordered titled panel the
  same width as the form panel; shrinking the window wraps the status/who groups
  to a new line instead of clipping; search + both filters still work.
