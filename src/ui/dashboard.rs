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

/// Where a metric card navigates when clicked. `Copy` so the card table can be a
/// plain array we iterate while laying out rows.
#[derive(Clone, Copy)]
enum CardNav {
    View(ui::View),
    Overdue,
    DueSoon,
    Outstanding,
}

impl CardNav {
    fn action(self) -> Action {
        match self {
            CardNav::View(v) => Action::Go(v),
            CardNav::Overdue => Action::Overdue,
            CardNav::DueSoon => Action::DueSoon,
            CardNav::Outstanding => Action::Outstanding,
        }
    }
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

    // Frame-based cards don't wrap inside `horizontal_wrapped` (a frame's width
    // isn't known until after it's placed), so chunk them into rows sized to the
    // available width ourselves.
    let cards: [(&str, String, egui::Color32, CardNav); 7] = [
        ("ผู้มุ่งหวัง (Prospects)", prospects.to_string(), ACCENT_STRONG, CardNav::View(ui::View::Prospects)),
        ("ลูกค้า VIP (Customers)", customers.to_string(), GREEN, CardNav::View(ui::View::Customers)),
        ("นักธุรกิจ (ABO)", abos.to_string(), ORANGE, CardNav::View(ui::View::Abos)),
        ("Monthly Activity", conversions.to_string(), PINK, CardNav::View(ui::View::Activities)),
        ("งานเลยกำหนด (Overdue)", overdue.to_string(), RED, CardNav::Overdue),
        ("งานใกล้ครบกำหนด (7 วัน)", due_soon.to_string(), AMBER, CardNav::DueSoon),
        (
            "ยอดสำรองจ่ายค้างรับ",
            format!("{} บาท", group_thousands(outstanding)),
            INDIGO,
            CardNav::Outstanding,
        ),
    ];

    // Each card is ~182px wide (150 min + 16×2 margin) plus item spacing.
    let card_w = 200.0;
    let per_row = ((ui.available_width() / card_w).floor() as usize).max(1);
    for chunk in cards.chunks(per_row) {
        ui.horizontal(|ui| {
            for (title, value, color, nav) in chunk {
                if ui::metric_card_clickable(ui, title, value, *color).clicked() {
                    *action = Some(nav.action());
                }
            }
        });
        ui.add_space(8.0);
    }
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
