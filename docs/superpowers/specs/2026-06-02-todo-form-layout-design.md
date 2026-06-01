# Todo Form Layout — Design Spec

**Date:** 2026-06-02
**Scope:** Presentational only. Restructure the add/edit form on the Todo List
page (`src/ui/todo.rs`, the form block). No DB / model / query / test changes;
behaviour, fields, and submit/cancel logic stay identical.

## Problem

The add/edit form is packed into a single `ui.horizontal_wrapped` row
(task field, contact combo, due-date checkbox + picker, action button). On a
narrow window it wraps mid-control and reads as a cluttered strip. The
"กำหนดส่ง" checkbox has an empty label, so its purpose isn't obvious.

## Design (approved: "Approach 1 — titled group + aligned rows")

Wrap the form in a bordered, titled group with aligned label/field rows, matching
the contact modal's style (`src/ui/forms.rs` `field_row`).

```
┌─ ➕ เพิ่มงานใหม่ ────────────────────────────────┐
│ สิ่งที่ต้องทำ   [___________________________________]│
│ เกี่ยวกับ        [ — ไม่ระบุ —                    ▼]   │
│ กำหนดส่ง        [✓ มีกำหนดส่ง]  [ 2026-06-02  📅 ]    │
│                                                      │
│                 [ ➕ เพิ่ม ]                          │
└────────────────────────────────────────────────────────┘
```

- **Group:** `egui::Frame::group(ui.style()).rounding(8.0).inner_margin(12.0)`,
  inner `ui.set_max_width(460.0)` so fields don't stretch across the window.
- **Title line:** `➕ เพิ่มงานใหม่` (ACCENT_STRONG, strong) in add mode; `✏ แก้ไขงาน`
  in edit mode.
- **Aligned rows** via a local `field_row(ui, label, add)` helper (fixed label
  column `LABEL_W = 110.0`, then the widget):
  - `สิ่งที่ต้องทำ` → `TextEdit` (`FIELD_W = 300.0`).
  - `เกี่ยวกับ` → `filter_combo` over all contacts, none-label `— ไม่ระบุ —`
    (`FIELD_W` wide).
  - `กำหนดส่ง` → checkbox **labelled `มีกำหนดส่ง`** (was empty); when on, the
    `DatePickerButton`; when off, weak `ไม่มีกำหนด`.
- **Actions** in a final row aligned under the field column (empty-label
  `field_row`): add mode → `➕ เพิ่ม`; edit mode → `💾 บันทึก` + `ยกเลิก`.

The filter row, the result count, and the table below are unchanged.

## Non-goals

- No change to filters, the table, or any behaviour/state.
- Not converting the form to a modal (rejected Approach 3).

## Verification

- `cargo build` clean; `cargo test` still 33 passing (no logic touched).
- Visual (`cargo run`): the form shows as a titled box with three aligned rows
  and the buttons beneath; adding/editing/cancelling still works; the
  "มีกำหนดส่ง" toggle shows/hides the date picker.
