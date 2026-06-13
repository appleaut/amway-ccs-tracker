# Dashboard Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the Dashboard into an at-a-glance hub: seven clickable metric cards, two actionable panels (tasks needing attention + upcoming meetings), and a tidy goals section — using existing data only.

**Architecture:** Rewrite `src/ui/dashboard.rs` into focused private section functions plus one pure, unit-tested helper (`attention_todos`). Clicks are collected into a deferred `Action` applied after layout (so `app` is never mutated mid-borrow). No DB-layer changes except removing a now-unneeded `#[allow(dead_code)]`.

**Tech Stack:** Rust, eframe/egui 0.28, chrono.

**Conventions for every task:**
- This repo is **hand-formatted** — NEVER run `cargo fmt`. Verify with `cargo build` / `cargo test`.
- Every commit message must end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`
- Work from `D:\Projects\amway\space-to-grow\amway_ccs_tracker` on branch `dashboard-improvements`.

---

### Task 1: Rewrite `dashboard.rs` (cards, panels, goals, helper + tests)

**Files:**
- Replace: `src/ui/dashboard.rs` (whole file)
- Modify: `src/db/mod.rs` (remove the `#[allow(dead_code)]` on `count_due_soon_todos`)

Context you need (all already exist):
- `DbConnection` methods: `count_by_type(ContactType)`, `count_conversions_this_month()`, `count_overdue_todos()`, `count_due_soon_todos(i64)`, `outstanding_total()`, `list_todos(&str) -> Vec<TodoRow>`, `list_meetings(false) -> Vec<Meeting>`, `set_todo_done(i64, bool)`.
- `app.handle(result, default)` returns the value or logs the error and returns `default`.
- `ui::metric_card_clickable(ui, title, value, color) -> egui::Response`.
- `crate::db::queries::{group_thousands, TodoRow}` — `TodoRow { todo: Todo, contact_name: Option<String>, contact_type: Option<ContactType> }` (NOT `Clone`).
- `Todo { id, contact_id: Option<i64>, task, due_date: Option<NaiveDate>, done, created_at }`.
- `Meeting { id, name, start_date: NaiveDate, end_date: NaiveDate, description, fee, created_at }`.
- Filters: `ui::todo::{TodoStatusFilter::{Overdue,Pending}, TodoWhoFilter::All}`, `ui::advances::AdvanceStatusFilter::Outstanding`.
- Completion modal: `app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone { id, task, contact_name }); app.todo_done_result.clear();`.

- [ ] **Step 1: Replace `src/ui/dashboard.rs` with this exact content**

```rust
//! Dashboard: at-a-glance metrics, the tasks/meetings that need attention, and
//! progress toward goals. Clicks are collected into a deferred `Action` and
//! applied after layout so `app` is never mutated while a closure borrows it.

use chrono::{Duration, Local, NaiveDate};

use crate::app::AppState;
use crate::db::queries::{group_thousands, TodoRow};
use crate::models::enums::ContactType;
use crate::ui::{self, ACCENT_STRONG};

const GREEN: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x7D, 0x32);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(0xE6, 0x51, 0x00);
const PINK: egui::Color32 = egui::Color32::from_rgb(0xAD, 0x14, 0x57);
const RED: egui::Color32 = egui::Color32::from_rgb(0xD3, 0x2F, 0x2F);
const AMBER: egui::Color32 = egui::Color32::from_rgb(0xB2, 0x6A, 0x00);
const INDIGO: egui::Color32 = egui::Color32::from_rgb(0x30, 0x3F, 0x9F);

/// A click outcome chosen during layout, applied after rendering.
enum Action {
    Go(ui::View),
    Overdue,
    DueSoon,
    Outstanding,
    Meetings,
    Todos,
    CompleteTodo {
        id: i64,
        task: String,
        contact_id: Option<i64>,
        contact_name: Option<String>,
    },
}

/// Todos needing attention: unfinished, due on or before `today + days` (covers
/// both overdue and due-soon), earliest due first, capped to `limit`.
fn attention_todos(rows: Vec<TodoRow>, today: NaiveDate, days: i64, limit: usize) -> Vec<TodoRow> {
    let cutoff = today + Duration::days(days);
    let mut items: Vec<TodoRow> = rows
        .into_iter()
        .filter(|r| !r.todo.done && r.todo.due_date.is_some_and(|d| d <= cutoff))
        .collect();
    items.sort_by_key(|r| r.todo.due_date);
    items.truncate(limit);
    items
}

pub fn render(app: &mut AppState, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.heading("แดชบอร์ด / Dashboard");
    ui.label(egui::RichText::new("ภาพรวมธุรกิจตามแนวทาง CCS Guide").weak());
    ui.add_space(12.0);

    let today = Local::now().date_naive();
    let mut action: Option<Action> = None;

    metric_row(app, ui, &mut action);

    ui.add_space(16.0);
    ui.columns(2, |cols| {
        attention_panel(app, &mut cols[0], today, &mut action);
        meetings_panel(app, &mut cols[1], &mut action);
    });

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(10.0);
    goals_panel(app, ui);

    if let Some(act) = action {
        apply_action(app, act);
    }
}

fn metric_row(app: &mut AppState, ui: &mut egui::Ui, action: &mut Option<Action>) {
    let r = app.db.count_by_type(ContactType::Prospect);
    let prospects = app.handle(r, 0);
    let r = app.db.count_by_type(ContactType::Customer);
    let customers = app.handle(r, 0);
    let r = app.db.count_by_type(ContactType::Abo);
    let abos = app.handle(r, 0);
    let r = app.db.count_conversions_this_month();
    let conversions = app.handle(r, 0);
    let r = app.db.count_overdue_todos();
    let overdue = app.handle(r, 0);
    let r = app.db.count_due_soon_todos(7);
    let due_soon = app.handle(r, 0);
    let r = app.db.outstanding_total();
    let outstanding = app.handle(r, 0);

    ui.horizontal_wrapped(|ui| {
        if ui::metric_card_clickable(ui, "ผู้มุ่งหวัง (Prospects)", &prospects.to_string(), ACCENT_STRONG)
            .clicked()
        {
            *action = Some(Action::Go(ui::View::Prospects));
        }
        if ui::metric_card_clickable(ui, "ลูกค้า VIP (Customers)", &customers.to_string(), GREEN)
            .clicked()
        {
            *action = Some(Action::Go(ui::View::Customers));
        }
        if ui::metric_card_clickable(ui, "นักธุรกิจ (ABO)", &abos.to_string(), ORANGE).clicked() {
            *action = Some(Action::Go(ui::View::Abos));
        }
        if ui::metric_card_clickable(ui, "Monthly Activity", &conversions.to_string(), PINK)
            .clicked()
        {
            *action = Some(Action::Go(ui::View::Activities));
        }
        if ui::metric_card_clickable(ui, "งานเลยกำหนด (Overdue)", &overdue.to_string(), RED).clicked()
        {
            *action = Some(Action::Overdue);
        }
        if ui::metric_card_clickable(ui, "งานใกล้ครบกำหนด (7 วัน)", &due_soon.to_string(), AMBER)
            .clicked()
        {
            *action = Some(Action::DueSoon);
        }
        let money = format!("{} บาท", group_thousands(outstanding));
        if ui::metric_card_clickable(ui, "ยอดสำรองจ่ายค้างรับ", &money, INDIGO).clicked() {
            *action = Some(Action::Outstanding);
        }
    });
}

fn attention_panel(
    app: &mut AppState,
    ui: &mut egui::Ui,
    today: NaiveDate,
    action: &mut Option<Action>,
) {
    ui.label(egui::RichText::new("งานที่ต้องสนใจ").strong());
    ui.add_space(4.0);

    let r = app.db.list_todos("");
    let rows = app.handle(r, Vec::new());
    let items = attention_todos(rows, today, 7, 5);

    if items.is_empty() {
        ui.label(egui::RichText::new("ไม่มีงานเร่งด่วน 🎉").weak());
    } else {
        for row in &items {
            ui.horizontal(|ui| {
                let mut done = false;
                if ui.checkbox(&mut done, "").changed() && done {
                    *action = Some(Action::CompleteTodo {
                        id: row.todo.id,
                        task: row.todo.task.clone(),
                        contact_id: row.todo.contact_id,
                        contact_name: row.contact_name.clone(),
                    });
                }
                let who = row
                    .contact_name
                    .as_deref()
                    .map(|n| format!("  ·  {n}"))
                    .unwrap_or_default();
                ui.label(format!("{}{}", row.todo.task, who));
                if let Some(d) = row.todo.due_date {
                    let txt = egui::RichText::new(d.format("%d/%m").to_string());
                    let txt = if d < today { txt.color(RED) } else { txt.weak() };
                    ui.label(txt);
                }
            });
        }
    }

    ui.add_space(4.0);
    if ui.link("ดูทั้งหมด →").clicked() {
        *action = Some(Action::Todos);
    }
}

fn meetings_panel(app: &mut AppState, ui: &mut egui::Ui, action: &mut Option<Action>) {
    ui.label(egui::RichText::new("งานประชุมที่กำลังจะถึง").strong());
    ui.add_space(4.0);

    let r = app.db.list_meetings(false);
    let meetings = app.handle(r, Vec::new());

    if meetings.is_empty() {
        ui.label(egui::RichText::new("ยังไม่มีงานประชุม").weak());
    } else {
        for m in meetings.iter().take(5) {
            let date = if m.start_date == m.end_date {
                m.start_date.format("%d/%m/%Y").to_string()
            } else {
                format!(
                    "{} – {}",
                    m.start_date.format("%d/%m"),
                    m.end_date.format("%d/%m/%Y")
                )
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&m.name).strong());
                ui.label(egui::RichText::new(date).weak());
            });
        }
    }

    ui.add_space(4.0);
    if ui.link("ดูทั้งหมด →").clicked() {
        *action = Some(Action::Meetings);
    }
}

fn goals_panel(app: &mut AppState, ui: &mut egui::Ui) {
    let r = app.db.count_by_type(ContactType::Customer);
    let customers = app.handle(r, 0);

    ui.label(egui::RichText::new("เป้าหมายลูกค้า VIP 20 คน").strong());
    let frac = (customers as f32 / 20.0).clamp(0.0, 1.0);
    ui.add(egui::ProgressBar::new(frac).text(format!("{customers} / 20")));

    ui.add_space(16.0);
    egui::Frame::group(ui.style())
        .rounding(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("ขั้นตอน Sponsor Flow (8 ขั้น)").strong());
            ui.add_space(4.0);
            ui.label(
                "1 จดรายชื่อ → 2 สร้างนัด → 3 เช็คฟอร์ม → 4 เปิดใจ → \
                 5 เปิดภาพ → 6 ปิดสมัคร → 7 ติดตาม BK → 8 วางแผน",
            );
        });
}

fn apply_action(app: &mut AppState, action: Action) {
    match action {
        Action::Go(v) => app.view = v,
        Action::Overdue => {
            app.view = ui::View::Todos;
            app.todo_status_filter = ui::todo::TodoStatusFilter::Overdue;
            app.todo_who_filter = ui::todo::TodoWhoFilter::All;
        }
        Action::DueSoon => {
            app.view = ui::View::Todos;
            app.todo_status_filter = ui::todo::TodoStatusFilter::Pending;
            app.todo_who_filter = ui::todo::TodoWhoFilter::All;
        }
        Action::Outstanding => {
            app.view = ui::View::Advances;
            app.advance_status_filter = ui::advances::AdvanceStatusFilter::Outstanding;
        }
        Action::Meetings => app.view = ui::View::Meetings,
        Action::Todos => app.view = ui::View::Todos,
        Action::CompleteTodo {
            id,
            task,
            contact_id,
            contact_name,
        } => match (contact_id, contact_name) {
            (Some(_), Some(name)) => {
                app.pending_todo_done = Some(crate::ui::todo_done::PendingTodoDone {
                    id,
                    task,
                    contact_name: name,
                });
                app.todo_done_result.clear();
            }
            _ => match app.db.set_todo_done(id, true) {
                Ok(()) => app.set_status("ทำงานเสร็จแล้ว"),
                Err(e) => app.set_error(e),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::todo::Todo;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn row(id: i64, due: Option<NaiveDate>, done: bool) -> TodoRow {
        TodoRow {
            todo: Todo {
                id,
                contact_id: None,
                task: format!("task {id}"),
                due_date: due,
                done,
                created_at: Local::now(),
            },
            contact_name: None,
            contact_type: None,
        }
    }

    #[test]
    fn attention_includes_overdue_and_due_soon_excludes_others() {
        let today = d("2026-06-10");
        let rows = vec![
            row(1, Some(d("2026-06-05")), false), // overdue -> include
            row(2, Some(d("2026-06-10")), false), // due today -> include
            row(3, Some(d("2026-06-17")), false), // within 7 days -> include
            row(4, Some(d("2026-06-18")), false), // 8 days out -> exclude
            row(5, Some(d("2026-06-05")), true),  // done -> exclude
            row(6, None, false),                  // no due date -> exclude
        ];
        let got: Vec<i64> = attention_todos(rows, today, 7, 10)
            .into_iter()
            .map(|r| r.todo.id)
            .collect();
        assert_eq!(got, vec![1, 2, 3]);
    }

    #[test]
    fn attention_sorts_by_due_and_caps_to_limit() {
        let today = d("2026-06-10");
        let rows = vec![
            row(1, Some(d("2026-06-12")), false),
            row(2, Some(d("2026-06-05")), false),
            row(3, Some(d("2026-06-11")), false),
        ];
        let got: Vec<i64> = attention_todos(rows, today, 30, 2)
            .into_iter()
            .map(|r| r.todo.id)
            .collect();
        assert_eq!(got, vec![2, 3]); // earliest two: 06-05 then 06-11
    }
}
```

- [ ] **Step 2: Remove the now-unneeded `#[allow(dead_code)]` in `src/db/mod.rs`**

Find this block:

```rust
    /// Reserved for a planned "due soon" dashboard card (see the query of the
    /// same name); kept so the card can be wired up without new plumbing.
    #[allow(dead_code)]
    pub fn count_due_soon_todos(&self, days: i64) -> Result<i64> {
        queries::count_due_soon_todos(&self.conn, days)
    }
```

Replace it with (drop the attribute, update the comment):

```rust
    /// Count of unfinished todos due within the next `days` days (the dashboard's
    /// "due soon" card).
    pub fn count_due_soon_todos(&self, days: i64) -> Result<i64> {
        queries::count_due_soon_todos(&self.conn, days)
    }
```

- [ ] **Step 3: Run the helper unit tests**

Run: `cargo test dashboard::`
Expected: `attention_includes_overdue_and_due_soon_excludes_others` and `attention_sorts_by_due_and_caps_to_limit` pass (2 passed).

- [ ] **Step 4: Build and run the full suite**

Run: `cargo build`
Expected: compiles with **no warnings** (the `count_due_soon_todos` dead-code allow is gone because the dashboard now calls it).

Run: `cargo test`
Expected: all pass (the prior 95 + 2 new = 97 passed; 0 failed).

- [ ] **Step 5: Commit**

```
git add src/ui/dashboard.rs src/db/mod.rs
git commit -m "Rework dashboard: clickable metrics, attention + meetings panels

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Visual verification and finish

**Files:** none (verification only; re-touch `src/ui/dashboard.rs` only if a glyph/layout fix is needed).

- [ ] **Step 1: Launch the app and capture the Dashboard**

Build and run the release exe, then capture the window (PowerShell + `Add-Type` C# `PrintWindow` against the `amway_ccs_tracker` process `MainWindowHandle`, run via Windows PowerShell `powershell.exe` so `System.Drawing.Bitmap` is available — same technique used for prior visual checks). The Dashboard is the default view, so no navigation is needed.

```
cargo build --release
.\target\release\amway_ccs_tracker.exe   # then capture its window to a PNG and open it
```

- [ ] **Step 2: Verify the layout and glyphs**

Confirm: seven metric cards wrap neatly; the two panels sit side by side (งานที่ต้องสนใจ | งานประชุมที่กำลังจะถึง); the VIP-20 bar and Sponsor-Flow card show below. Check the empty-state line **"ไม่มีงานเร่งด่วน 🎉"** renders the 🎉 glyph (not a tofu box).

**If 🎉 is tofu:** it is not in egui's bundled font subset. Replace `"ไม่มีงานเร่งด่วน 🎉"` with `"ไม่มีงานเร่งด่วน ✅"` (✅ is already used elsewhere in the app and is known-good), rebuild, re-capture. Commit the fix:
```
git add src/ui/dashboard.rs
git commit -m "Use a font-subset glyph for the dashboard empty state

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 3: Manual interaction check**

With some seeded data: click each metric card and confirm it navigates to the right view (and that Overdue/Due-soon/Outstanding land on the filtered page); tick a task in "งานที่ต้องสนใจ" and confirm a contact-linked task opens the result dialog while an unassigned task is marked done; click "ดูทั้งหมด →" on each panel. (If data is sparse, verify what you can and note the rest as covered by the navigation map.)

- [ ] **Step 4: Finish the branch**

Use the **superpowers:finishing-a-development-branch** skill: confirm `cargo test` passes, then present the merge/PR options. (Per project rule, do NOT merge to main without explicit user approval.)

---

## Notes for the implementer

- **Deferred-action pattern:** never set `app.view` (or other `app` fields) inside the card/panel closures — record an `Action` and apply it once after layout. This avoids borrow conflicts and matches the existing `go_overdue` approach the old dashboard used.
- **`app.handle(r, default)` needs two statements** (`let r = app.db.X(); let v = app.handle(r, 0);`) — calling `app.handle(app.db.X(), 0)` in one expression double-borrows `app`.
- **`TodoRow` is not `Clone`** — that is why `Action::CompleteTodo` carries individual fields, and `attention_todos` consumes its input via `into_iter()` and returns owned rows.
- **Completion behavior mirrors `src/ui/todo.rs`:** contact-linked todos open `pending_todo_done` (result captured on save); unassigned todos are marked done immediately.
