# Dashboard Improvements Design

**Date:** 2026-06-12
**Status:** Approved (design), pending plan

## Goal

Turn the Dashboard from a static summary into an at-a-glance "what do I need to do
today" hub: more useful metrics, two actionable lists (tasks needing attention,
upcoming meetings), all cards clickable to jump to the relevant view, and a
tidier layout — using data the app already stores (no new DB queries).

## Background

`src/ui/dashboard.rs` currently renders five metric cards (Prospects, Customers,
ABO, conversions-this-month, overdue todos — only the last is clickable), a
"VIP 20" progress bar, and a static Sponsor-Flow text line. Several useful figures
the app already computes are not surfaced: outstanding advance total, due-soon
todos, and upcoming meetings.

All needed data is already exposed on `DbConnection`:
- `count_by_type(ContactType)`, `count_conversions_this_month()`,
  `count_overdue_todos()` — used today.
- `count_due_soon_todos(days)` — exists, currently `#[allow(dead_code)]`.
- `outstanding_total()` — sum of uncollected advances (baht).
- `list_meetings(false)` — upcoming meetings, sorted by `start_date` ascending.
- `list_todos("")` — all todos as `TodoRow { todo, contact_name, contact_type }`,
  sorted done-last then by due date.

Reusable helpers: `ui::metric_card`, `ui::metric_card_clickable`,
`crate::db::queries::group_thousands(i64) -> String` (baht formatting), the
`pending_todo_done` completion modal, and the navigation pattern
`app.view = View::X` (+ filter fields).

## Locked Decisions

Implement all of: more metrics, actionable lists, every card clickable, and a
tidier layout (the user chose "D — all of the above", then approved each item).

## Architecture

One file is rewritten — `src/ui/dashboard.rs` — split into focused private
functions plus one pure, unit-tested helper. No DB-layer changes (the
`#[allow(dead_code)]` on `count_due_soon_todos` is removed once it is used).

### Pure helper (unit-tested)

```rust
/// Todos that need attention: not done, with a due date on or before
/// `today + days` (covers overdue AND due-soon), earliest due first, capped to
/// `limit`. Input is the app's standard todo list (already done-last sorted).
fn attention_todos(rows: Vec<TodoRow>, today: NaiveDate, days: i64, limit: usize) -> Vec<TodoRow>
```

Logic: keep rows where `!todo.done && todo.due_date.is_some_and(|d| d <= today + days)`;
sort by `due_date` ascending; truncate to `limit`. Pure over its inputs (today is
passed in), so it is fully testable without a clock or DB.

### `render(app, ui)` composition

Calls these private section functions in order:

1. **`metric_row(app, ui)`** — heading + subtitle, then a `horizontal_wrapped`
   row of seven clickable metric cards. Each computes its count via
   `app.handle(app.db.<query>, 0)` (same error-handling as today), uses
   `metric_card_clickable`, and on click sets the target view/filters:

   | Card | Value | On click |
   |------|-------|----------|
   | ผู้มุ่งหวัง (Prospects) | `count_by_type(Prospect)` | `view = Prospects` |
   | ลูกค้า VIP (Customers) | `count_by_type(Customer)` | `view = Customers` |
   | นักธุรกิจ (ABO) | `count_by_type(Abo)` | `view = Abos` |
   | เปลี่ยนสถานะเดือนนี้ | `count_conversions_this_month()` | `view = Activities` |
   | งานเลยกำหนด (red) | `count_overdue_todos()` | `view = Todos`, status = `Overdue`, who = `All` |
   | งานใกล้ครบกำหนด (7 วัน) | `count_due_soon_todos(7)` | `view = Todos`, status = `Pending`, who = `All` |
   | ยอดสำรองจ่ายค้างรับ | `group_thousands(outstanding_total())` + " บาท" | `view = Advances`, advance_status_filter = `Outstanding` |

   Card accent colors: keep the existing four (Prospects = ACCENT_STRONG,
   Customers = green `0x2E,0x7D,0x32`, ABO = orange `0xE6,0x51,0x00`, conversions
   = pink `0xAD,0x14,0x57`, overdue = red `0xD3,0x2F,0x2F`); due-soon = dark amber
   `0xB2,0x6A,0x00`, outstanding = indigo `0x30,0x3F,0x9F`. Navigation is applied
   after the row (collect a single deferred action during layout, then act),
   mirroring today's `go_overdue` pattern so we never mutate `app.view`
   mid-borrow.

2. **`attention_panel` + `meetings_panel`** rendered side by side via
   `ui.columns(2, |cols| { ... })`:

   - **งานที่ต้องสนใจ** (left): heading, then `attention_todos(list_todos(""),
     today, 7, 5)`. Each row: a checkbox + task text + contact (if any) + due
     date; overdue rows (`due_date < today`) show the date in red. Ticking the
     checkbox reproduces the Todos page's completion behavior: if the todo has a
     contact, open the result dialog (`app.pending_todo_done = Some(PendingTodoDone
     { id, task, contact_name }); app.todo_done_result.clear();`); otherwise
     `app.db.set_todo_done(id, true)` directly. Navigation is an explicit
     "ดูทั้งหมด →" link at the bottom of the panel (a `ui.link`/small button) that
     sets `view = Todos` — kept separate from the rows so it never conflicts with
     the row checkboxes. Empty state: "ไม่มีงานเร่งด่วน 🎉".
   - **งานประชุมที่กำลังจะถึง** (right): heading, then up to 5 of
     `list_meetings(false)`: meeting name + date (range shown when
     `start_date != end_date`). A "ดูทั้งหมด →" link at the bottom sets
     `view = Meetings` (meeting rows have no interactive widgets, so a row click
     may also navigate). Empty state: "ยังไม่มีงานประชุม".

3. **`goals_panel(app, ui)`** — the existing VIP-20 progress bar, plus the
   Sponsor-Flow 8-step line wrapped in a subtle `Frame::group` for tidiness.
   Both unchanged in content.

### App-state interactions

Read-only DB calls through `app.handle(...)`, plus on interaction: set
`app.view`, `app.todo_status_filter`, `app.todo_who_filter`,
`app.advance_status_filter`, or `app.pending_todo_done` / `app.todo_done_result` /
`app.db.set_todo_done`. All of these fields/methods already exist. No new
`AppState` fields.

## Data Flow

`render` → per-frame DB reads (counts, todo list, meeting list) → lay out cards +
panels, collecting any click into a deferred action → after layout, apply the
action (navigate / open done-dialog / mark done). The done-dialog itself is the
existing `ui::todo_done` modal already rendered from `update()`.

## Error Handling

Every DB read goes through `app.handle(result, default)` (existing pattern: shows
the error in the status bar, returns the default). A failed `set_todo_done` calls
`app.set_error(e)`. No panics/unwraps.

## Behavior Details

- "งานใกล้ครบกำหนด" counts the next 7 days inclusive (matches
  `count_due_soon_todos`, which is `today <= due <= today+7` and excludes overdue);
  the attention list intentionally merges overdue + due-soon for action.
- Money shown as `group_thousands(total) + " บาท"`, consistent with the Advances
  page.
- Lists cap at 5 rows; the "ดูทั้งหมด" affordance leads to the full page.

## Testing

- **Unit tests** (`src/ui/dashboard.rs`): `attention_todos` —
  - excludes done todos and todos with no due date,
  - includes overdue (due < today) and due-soon (due ≤ today+days),
  - excludes due dates beyond `today + days`,
  - sorts earliest-due first and respects `limit`.
  Build `TodoRow`s with `Todo`/`Contact` test constructors already used in
  `db/queries.rs` tests.
- **Manual/visual:** run the app, open the Dashboard, verify the seven cards,
  both panels (with seeded data and empty states), navigation on click, and the
  done-checkbox flow. Capture a window screenshot; confirm the 🎉 glyph renders
  (not tofu) per the egui bundled-font constraint — fall back to a text-only empty
  message if it is tofu.

## Out of Scope (YAGNI)

- Historical charts / trend lines.
- User-customizable dashboard layout or card selection.
- New KPIs that would require new DB queries.
- Refactoring the Todos page's completion logic into a shared helper (the small
  toggle block is mirrored in the dashboard with a comment; a shared extraction
  is deferred to avoid widening this change).
