//! Application state, the eframe main loop, sidebar navigation, settings, and
//! one-time setup (fonts, theme, database location).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::Local;

use crate::db::DbConnection;
use crate::error::{AppError, Result};
use crate::ui::forms::ContactForm;
use crate::ui::{self, View, ACCENT, ACCENT_STRONG};

/// Top-level mutable state shared across all views.
pub struct AppState {
    pub db: DbConnection,
    pub view: View,
    pub search: String,
    /// Last error, shown in the status bar until dismissed.
    pub last_error: Option<String>,
    /// Transient success/info message.
    pub status: Option<String>,
    /// State of the add/edit modal.
    pub form: ContactForm,
    /// ABO currently selected in the Follow-Up view.
    pub selected_abo: Option<i64>,
    /// Search text for the Follow-Up ABO picker (filterable combo box).
    pub followup_abo_filter: String,
    /// Human-readable database location, shown in Settings.
    pub db_location: String,
    /// Name of the loaded Thai font (or a note if none was found).
    pub font_name: String,
    /// PV figure typed into the Settings rank calculator.
    pub pv_input: String,
    /// Downline leg counts entered in the Settings rank calculator.
    pub pv_legs_c1: usize,
    pub pv_legs_cl: usize,
    pub pv_legs_cl15: usize,
    /// Per-table sort state.
    pub prospect_sort: ui::SortSpec,
    pub customer_sort: ui::SortSpec,
    pub abo_sort: ui::SortSpec,
    /// User-dragged position offsets for downline-chart nodes, keyed by contact
    /// id (the central "me" node uses `i64::MIN`). Empty = pure auto-layout.
    pub node_offsets: HashMap<i64, egui::Vec2>,
    /// Zoom factor for the network chart (1.0 = default; reset by Auto-arrange).
    pub chart_zoom: f32,
    /// Downline-chart nodes currently selected (keyed like `node_offsets`:
    /// contact id, or `i64::MIN` for me). Dragging any one of them moves the
    /// whole set together; a rubber-band drag on empty canvas (re)builds it.
    pub selected_nodes: HashSet<i64>,
    /// Anchor of an in-progress rubber-band selection (screen coords); `None`
    /// when no box is being drawn.
    pub chart_select_start: Option<egui::Pos2>,
    /// View pan offset for the network chart (screen px). Ctrl + drag and the
    /// mouse wheel move it freely in any direction; reset by Auto-arrange.
    pub chart_pan: egui::Vec2,
    /// Row awaiting delete confirmation (a contact or an activity type).
    pub pending_delete: Option<ui::confirm::PendingDelete>,
    /// Backup file the user picked to restore from, awaiting confirmation.
    pub pending_restore: Option<PathBuf>,
    /// ABO id currently open in the Rank Advisor modal.
    pub rank_advisor: Option<i64>,
    /// Whether the self ("ฉัน / ME") Rank Advisor modal is open.
    pub me_advisor: bool,
    /// Contact whose activity log is open, plus the new-entry draft.
    pub activity_contact: Option<i64>,
    /// Selected activity-type name in the activity-log "add" form.
    pub activity_kind: String,
    pub activity_note: String,
    /// Kind filter on the aggregate Activity History page (`None` = all kinds).
    pub history_kind: Option<String>,
    /// Draft text for the Activity Types page (add / rename buffer).
    pub kind_draft: String,
    /// Activity-type id being renamed on the Activity Types page (`None` = add).
    pub kind_edit: Option<i64>,
    /// Network-chart PNG export. The button sets `export_chart_pending`; we then
    /// request a framebuffer screenshot and, once it arrives, crop it to
    /// `chart_export_rect` (the chart's on-screen viewport) and save the file.
    pub export_chart_pending: bool,
    pub awaiting_screenshot: bool,
    pub chart_export_rect: Option<egui::Rect>,
    /// Path of the most recently saved image — shown as a clickable link in the
    /// status bar (click → open it with the OS default app).
    pub last_saved_image: Option<String>,
    /// Todo List add/edit form state.
    pub todo_form: crate::ui::todo::TodoForm,
    /// Status filter on the Todo List page.
    pub todo_status_filter: crate::ui::todo::TodoStatusFilter,
    /// Contact-type filter on the Todo List page.
    pub todo_who_filter: crate::ui::todo::TodoWhoFilter,
    /// A todo whose done-toggle is awaiting its result text (drives the
    /// `ui::todo_done` modal); `None` when no completion dialog is open.
    pub pending_todo_done: Option<crate::ui::todo_done::PendingTodoDone>,
    /// Result-text buffer for the todo-completion dialog.
    pub todo_done_result: String,
    /// Contact picked in the todo-completion dialog when the todo is contactless;
    /// the result is logged to this contact. `None` = log no history.
    pub todo_done_contact_id: Option<i64>,
    /// Search-filter buffer for the contactless todo-completion contact picker.
    pub todo_done_contact_filter: String,
    /// An advance whose collect-action is awaiting its date + note (drives the
    /// `ui::advance_collect` modal); `None` when no collect dialog is open.
    pub pending_advance_collect: Option<crate::ui::advance_collect::PendingAdvanceCollect>,
    /// Collection-date and note buffers for the advance-collect dialog.
    pub advance_collect_date: chrono::NaiveDate,
    pub advance_collect_note: String,
    /// Advance Payments add/edit form state.
    pub advance_form: crate::ui::advances::AdvanceForm,
    /// Status filter on the Advance Payments page.
    pub advance_status_filter: crate::ui::advances::AdvanceStatusFilter,
    /// Add/edit meeting modal state.
    pub meeting_form: crate::ui::meeting_form::MeetingForm,
    /// Contact-type filter on the Meetings page.
    pub meeting_who_filter: crate::ui::meetings::MeetingWhoFilter,
    /// Whether the Meetings matrix also shows meetings that have already finished.
    pub meeting_show_past: bool,
    /// Recurring-task schedule add/edit form state.
    pub todo_schedule_form: crate::ui::todo_schedules::TodoScheduleForm,
    /// Calendar date of the last auto-generation run. Initialised to a sentinel
    /// in the past so the first `update` frame generates due todos (covering app
    /// start); thereafter it re-runs whenever the date changes while open.
    pub last_gen_check: chrono::NaiveDate,
    /// Promotion-downloader: whether a background run is in progress.
    pub promo_running: bool,
    /// Channel from the download worker thread (`None` when idle).
    pub promo_rx: Option<std::sync::mpsc::Receiver<crate::promo::PromoMsg>>,
    /// Streamed progress lines, capped to the most recent ~200.
    pub promo_log: Vec<String>,
    /// Summary of the last finished run (success or error), shown in the view.
    pub promo_last_result: Option<String>,
}

impl AppState {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let font_name = setup_fonts(&cc.egui_ctx);
        setup_theme(&cc.egui_ctx);

        let path = db_path()?;
        let db = DbConnection::open(&path)?;

        Ok(AppState {
            db,
            view: View::Dashboard,
            search: String::new(),
            last_error: None,
            status: None,
            form: ContactForm::default(),
            selected_abo: None,
            followup_abo_filter: String::new(),
            db_location: path.display().to_string(),
            font_name,
            pv_input: String::new(),
            pv_legs_c1: 0,
            pv_legs_cl: 0,
            pv_legs_cl15: 0,
            prospect_sort: ui::SortSpec::new(2, false), // score, descending
            customer_sort: ui::SortSpec::new(2, false), // score, descending
            abo_sort: ui::SortSpec::new(0, true),       // name, ascending
            node_offsets: HashMap::new(),
            chart_zoom: 1.0,
            selected_nodes: HashSet::new(),
            chart_select_start: None,
            chart_pan: egui::Vec2::ZERO,
            pending_delete: None,
            pending_restore: None,
            rank_advisor: None,
            me_advisor: false,
            activity_contact: None,
            activity_kind: String::new(),
            activity_note: String::new(),
            history_kind: None,
            kind_draft: String::new(),
            kind_edit: None,
            export_chart_pending: false,
            awaiting_screenshot: false,
            chart_export_rect: None,
            last_saved_image: None,
            todo_form: crate::ui::todo::TodoForm::default(),
            todo_status_filter: crate::ui::todo::TodoStatusFilter::Pending,
            todo_who_filter: crate::ui::todo::TodoWhoFilter::All,
            pending_todo_done: None,
            todo_done_result: String::new(),
            todo_done_contact_id: None,
            todo_done_contact_filter: String::new(),
            pending_advance_collect: None,
            advance_collect_date: chrono::Local::now().date_naive(),
            advance_collect_note: String::new(),
            advance_form: crate::ui::advances::AdvanceForm::default(),
            advance_status_filter: crate::ui::advances::AdvanceStatusFilter::Outstanding,
            meeting_form: crate::ui::meeting_form::MeetingForm::default(),
            meeting_who_filter: crate::ui::meetings::MeetingWhoFilter::All,
            meeting_show_past: false,
            todo_schedule_form: crate::ui::todo_schedules::TodoScheduleForm::default(),
            last_gen_check: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            promo_running: false,
            promo_rx: None,
            promo_log: Vec::new(),
            promo_last_result: None,
        })
    }

    /// Record an error for display.
    pub fn set_error<E: std::fmt::Display>(&mut self, e: E) {
        self.last_error = Some(e.to_string());
        self.last_saved_image = None;
    }

    /// Record a transient status message (clears any prior error).
    pub fn set_status(&mut self, s: impl Into<String>) {
        self.last_error = None;
        self.status = Some(s.into());
        self.last_saved_image = None;
    }

    /// Record a status whose `path` is shown as a clickable link in the status
    /// bar (click → open with the OS default app).
    pub fn set_saved_image(&mut self, msg: impl Into<String>, path: String) {
        self.last_error = None;
        self.status = Some(msg.into());
        self.last_saved_image = Some(path);
    }

    /// Restore the database from `src`: drop the live connection so the file is
    /// unlocked, run the validate→safety-backup→swap, then reopen. On any error,
    /// reopen the still-intact original so the app always ends with a live
    /// connection.
    pub fn perform_restore(&mut self, src: PathBuf) {
        let now = Local::now().naive_local();
        let result = (|| -> Result<PathBuf> {
            let db_file = db_path()?;
            let backups = backups_dir()?;
            // Open the scratch DB BEFORE replacing, so a failure here leaves the
            // real connection intact.
            let scratch = DbConnection::open(std::path::Path::new(":memory:"))?;
            let _ = std::mem::replace(&mut self.db, scratch); // drops the real conn
            let safety = crate::backup::restore_from(&src, &db_file, &backups, now)?;
            self.db = DbConnection::open(&db_file)?;
            Ok(safety)
        })();
        match result {
            Ok(safety) => self.set_status(format!(
                "กู้คืนข้อมูลแล้ว (สำรองเดิมไว้ที่ {})",
                safety.display()
            )),
            Err(e) => {
                // restore_from leaves the original data.db intact on failure, so
                // reopen it. If that ALSO fails the app is running on the empty
                // in-memory scratch — surface that louder than the restore error.
                self.set_error(e);
                if let Ok(p) = db_path() {
                    if let Err(e2) = DbConnection::open(&p).map(|db| self.db = db) {
                        self.set_error(AppError::validation(format!(
                            "กู้คืนไม่สำเร็จ และเปิดฐานข้อมูลเดิมไม่ได้: {e2}"
                        )));
                    }
                }
            }
        }
    }

    /// Unwrap a `Result`, surfacing any error in the status bar and falling back
    /// to `default`. Keeps view code free of repetitive match arms.
    pub fn handle<T>(&mut self, r: Result<T>, default: T) -> T {
        match r {
            Ok(v) => v,
            Err(e) => {
                self.set_error(e);
                default
            }
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Amway CCS").color(ACCENT_STRONG).size(24.0).strong());
        ui.label(egui::RichText::new("Prospect & Downline Tracker").size(11.0).weak());
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        // Scroll the nav items (and the add button) so every entry stays
        // reachable on short windows where the list would otherwise overflow.
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let items = [
            (View::Dashboard, "🏠  แดชบอร์ด"),
            (View::Prospects, "🎯  ผู้มุ่งหวัง"),
            (View::Customers, "💳  ลูกค้า VIP"),
            (View::Abos, "💼  นักธุรกิจ"),
            (View::FollowUp, "✅  ติดตามผล"),
            (View::Meetings, "🎟  งานประชุม"),
            (View::Todos, "📅  สิ่งที่ต้องทำ"),
            (View::TodoSchedules, "🔁  ตารางงานประจำ"),
            (View::Advances, "💵  สำรองจ่าย"),
            (View::PromoDownload, "🖼  ดาวน์โหลดโปรโมชัน"),
            (View::Network, "🌳  เครือข่าย"),
            (View::Activities, "📝  ประวัติติดต่อ"),
            (View::ActivityKinds, "📋  ประเภทกิจกรรม"),
            (View::Settings, "⚙  ตั้งค่า"),
        ];
        for (view, label) in items {
            if ui
                .selectable_label(self.view == view, egui::RichText::new(label).size(16.0))
                .clicked()
            {
                self.view = view;
            }
            ui.add_space(2.0);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        if ui
            .add(egui::Button::new("➕ เพิ่มรายชื่อ").fill(ACCENT))
            .clicked()
        {
            self.form = ContactForm::for_new();
        }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let error = self.last_error.clone();
        let status = self.status.clone();
        let saved = self.last_saved_image.clone();
        let mut clear = false;
        let mut open_path: Option<String> = None;
        ui.horizontal(|ui| {
            if let Some(err) = error {
                ui.colored_label(egui::Color32::from_rgb(0xFF, 0x6E, 0x6E), format!("⚠ {err}"));
                if ui.small_button("✖ ล้าง").clicked() {
                    clear = true;
                }
            } else if let Some(s) = status {
                ui.colored_label(ACCENT_STRONG, format!("✅ {s}"));
                if let Some(path) = &saved {
                    ui.label("→");
                    let link = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(path.as_str()).color(ACCENT).underline(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text("คลิกเพื่อเปิดรูป");
                    if link.clicked() {
                        open_path = Some(path.clone());
                    }
                }
            } else {
                ui.label(
                    egui::RichText::new("พร้อมใช้งาน • Amway CCS Tracker v0.1")
                        .small()
                        .weak(),
                );
            }
        });
        if clear {
            self.last_error = None;
        }
        if let Some(path) = open_path {
            if let Err(e) = open_in_os(&path) {
                self.set_error(e);
            }
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.heading("ตั้งค่า / Settings");
        ui.add_space(10.0);

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("ฐานข้อมูล (Database):");
                ui.label(&self.db_location);
                ui.end_row();
                ui.label("ฟอนต์ (Font):");
                ui.label(&self.font_name);
                ui.end_row();
            });

        ui.add_space(8.0);
        let total = self.db.list_contacts().map(|v| v.len()).unwrap_or(0);
        ui.label(format!("รายชื่อทั้งหมด (Total contacts): {total}"));

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "ข้อมูลรายชื่อถูกบันทึกในเครื่อง (Local SQLite) • ตัวดาวน์โหลดโปรโมชันจะเชื่อมต่อ amway.co.th เฉพาะตอนสั่งงานเท่านั้น",
            )
            .small()
            .weak(),
        );

        ui::settings_backup::render(self, ui);

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("คำนวณระดับ/โบนัสตามเงื่อนไข").strong());
        ui.horizontal(|ui| {
            ui.label("ยอด PV:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pv_input)
                    .hint_text("เช่น 15000")
                    .desired_width(120.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("สายงานดาวน์ไลน์:");
            ui.label("C1+");
            ui.add(egui::DragValue::new(&mut self.pv_legs_c1).range(0..=99));
            ui.label("CL+");
            ui.add(egui::DragValue::new(&mut self.pv_legs_cl).range(0..=99));
            ui.label("CL15+");
            ui.add(egui::DragValue::new(&mut self.pv_legs_cl15).range(0..=99));
        });
        let trimmed = self.pv_input.trim();
        if let Ok(pv) = trimmed.parse::<i64>() {
            let rank = crate::utils::scoring::qualified_rank(
                pv,
                self.pv_legs_c1,
                self.pv_legs_cl,
                self.pv_legs_cl15,
            );
            let bonus = crate::utils::scoring::bonus_percent_for_pv(pv);
            ui.label(
                egui::RichText::new(format!(
                    "ระดับที่ผ่านเงื่อนไข: {}  •  โบนัส: {}%",
                    rank.as_str(),
                    bonus
                ))
                .color(ACCENT_STRONG)
                .strong(),
            );
            ui.label(
                egui::RichText::new(
                    "เงื่อนไข: C1 = PV>=5,000 | CL = >=10,000 + 3 สาย C1+ | \
                     CL15 = >=20,000 + 3 สาย CL+ | CL21 = >=30,000 + 3 สาย CL15+",
                )
                .small()
                .weak(),
            );
        } else if !trimmed.is_empty() {
            ui.weak("กรุณากรอกตัวเลข PV (numeric only)");
        }
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Auto-create due recurring todos on the first frame (covers app start)
        // and whenever the calendar date changes while the app stays open.
        let today = chrono::Local::now().date_naive();
        if today != self.last_gen_check {
            self.last_gen_check = today;
            let r = self.db.generate_due_todos(today);
            let _ = self.handle(r, 0);
        }

        // Drain promotion-download progress from the worker thread. Take the
        // receiver out so we can mutate `self` while processing messages; put it
        // back unless the run finished.
        if let Some(rx) = self.promo_rx.take() {
            let mut finished = false;
            loop {
                match rx.try_recv() {
                    Ok(crate::promo::PromoMsg::Line(l)) => {
                        self.promo_log.push(l);
                        let n = self.promo_log.len();
                        if n > 200 {
                            self.promo_log.drain(0..n - 200);
                        }
                    }
                    Ok(crate::promo::PromoMsg::Done { saved, dir }) => {
                        self.promo_running = false;
                        self.promo_last_result = Some(format!("ดาวน์โหลด {saved} รูปแล้ว"));
                        self.set_saved_image(
                            format!("ดาวน์โหลดโปรโมชัน {saved} รูปแล้ว"),
                            dir,
                        );
                        finished = true;
                    }
                    Ok(crate::promo::PromoMsg::Failed(reason)) => {
                        self.promo_running = false;
                        self.promo_last_result = Some(format!("ผิดพลาด: {reason}"));
                        self.set_error(reason);
                        finished = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // Worker ended without a terminal message (e.g. it
                        // panicked) — don't wedge the button/spinner forever.
                        if self.promo_running {
                            self.promo_running = false;
                            self.promo_last_result =
                                Some("ผิดพลาด: การดาวน์โหลดสิ้นสุดผิดปกติ".to_string());
                            self.set_error("การดาวน์โหลดสิ้นสุดผิดปกติ");
                        }
                        finished = true;
                        break;
                    }
                }
            }
            if !finished {
                self.promo_rx = Some(rx);
            }
        }
        if self.promo_running {
            ctx.request_repaint();
        }

        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .exact_width(210.0)
            .show(ctx, |ui| self.sidebar(ui));

        egui::TopBottomPanel::bottom("status_panel").show(ctx, |ui| self.status_bar(ui));

        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Dashboard => ui::dashboard::render(self, ui),
            View::Prospects => ui::prospect_list::render(self, ui),
            View::Customers => ui::customer_list::render(self, ui),
            View::Abos => ui::abo_list::render(self, ui),
            View::FollowUp => ui::followup::render(self, ui),
            View::Meetings => ui::meetings::render(self, ui),
            View::Todos => ui::todo::render(self, ui),
            View::TodoSchedules => ui::todo_schedules::render(self, ui),
            View::Advances => ui::advances::render(self, ui),
            View::PromoDownload => ui::promo_download::render(self, ui),
            View::Network => ui::downline_tree::render(self, ui),
            View::Activities => ui::activities::render(self, ui),
            View::ActivityKinds => ui::activity_kinds::render(self, ui),
            View::Settings => self.settings(ui),
        });

        // Modals render on top of whatever view is active.
        ui::forms::render(self, ctx);
        ui::confirm::render(self, ctx);
        ui::settings_backup::render_restore_confirm(self, ctx);
        ui::todo_done::render(self, ctx);
        ui::advance_collect::render(self, ctx);
        ui::meeting_form::render(self, ctx);
        ui::rank_advisor::render(self, ctx);
        ui::rank_advisor::render_me(self, ctx);
        ui::activity_log::render(self, ctx);

        self.handle_chart_export(ctx);
    }
}

impl AppState {
    /// Drive the network-chart PNG export: request a framebuffer screenshot,
    /// then crop it to the chart's viewport and save once the reply arrives
    /// (one frame later).
    fn handle_chart_export(&mut self, ctx: &egui::Context) {
        if self.export_chart_pending {
            self.export_chart_pending = false;
            self.awaiting_screenshot = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            ctx.request_repaint();
        }
        if !self.awaiting_screenshot {
            return;
        }
        ctx.request_repaint();
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            self.awaiting_screenshot = false;
            let ppp = ctx.pixels_per_point();
            match save_chart_png(&image, self.chart_export_rect, ppp) {
                Ok(path) => self.set_saved_image("บันทึกรูปผังเครือข่ายแล้ว", path),
                Err(e) => self.set_error(e),
            }
        }
    }
}

/// Crop `image` (a full-window framebuffer) to `rect` (chart viewport, in
/// points) and write it as a PNG under `…/AmwayCCSTracker/exports/`. Returns the
/// saved path.
fn save_chart_png(image: &egui::ColorImage, rect: Option<egui::Rect>, ppp: f32) -> Result<String> {
    let [iw, ih] = image.size;
    let (x0, y0, x1, y1) = match rect {
        Some(r) => (
            (r.min.x * ppp).floor().max(0.0) as usize,
            (r.min.y * ppp).floor().max(0.0) as usize,
            ((r.max.x * ppp).ceil() as usize).min(iw),
            ((r.max.y * ppp).ceil() as usize).min(ih),
        ),
        None => (0, 0, iw, ih),
    };
    let w = x1.saturating_sub(x0);
    let h = y1.saturating_sub(y0);
    if w == 0 || h == 0 {
        return Err(AppError::validation("พื้นที่ผังว่างเปล่า บันทึกรูปไม่ได้"));
    }

    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in y0..y1 {
        let row = y * iw;
        for x in x0..x1 {
            let c = image.pixels[row + x];
            rgba.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
        }
    }

    let dir = db_path()?
        .parent()
        .map(|p| p.join("exports"))
        .ok_or_else(|| AppError::validation("ไม่พบโฟลเดอร์สำหรับบันทึกรูป"))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("network_{}.png", Local::now().format("%Y%m%d_%H%M%S")));

    let file = std::fs::File::create(&path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| AppError::validation(format!("เขียนไฟล์ PNG ไม่สำเร็จ: {e}")))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| AppError::validation(format!("เขียนไฟล์ PNG ไม่สำเร็จ: {e}")))?;

    Ok(path.display().to_string())
}

/// Open a file with the OS default handler (Windows Explorer launches the file's
/// associated app). Fire-and-forget — we don't wait on the child.
fn open_in_os(path: &str) -> Result<()> {
    std::process::Command::new("explorer").arg(path).spawn()?;
    Ok(())
}

/// Resolve `%APPDATA%\AmwayCCSTracker\data.db`, creating the directory.
pub(crate) fn db_path() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|_| AppError::validation("APPDATA environment variable is not set"))?;
    let dir = PathBuf::from(base).join("AmwayCCSTracker");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("data.db"))
}

/// Resolve `%APPDATA%\AmwayCCSTracker\backups`, creating the directory.
pub(crate) fn backups_dir() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|_| AppError::validation("APPDATA environment variable is not set"))?;
    let dir = PathBuf::from(base).join("AmwayCCSTracker").join("backups");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Embed the Kanit Thai font (Regular + Medium) and make it the primary face,
/// so the binary stays self-contained and renders Thai everywhere. Returns a
/// description for the Settings screen.
fn setup_fonts(ctx: &egui::Context) -> String {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "kanit".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Kanit-Regular.ttf")),
    );
    fonts.font_data.insert(
        "kanit-medium".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Kanit-Medium.ttf")),
    );

    // Kanit Regular as the default proportional + monospace face, keeping the
    // existing fallbacks (Ubuntu + egui's emoji/icon fonts) for glyph coverage.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "kanit".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, "kanit".to_owned());

    // A medium-weight named family for headings / buttons. It MUST keep the
    // emoji/icon fallback fonts: egui renders buttons AND menu items
    // (selectable_label) with TextStyle::Button, so without these fallbacks the
    // icons (➕ 🔍 ▶ ✏ 🗑 …) would be missing glyphs.
    let mut medium_chain = vec!["kanit-medium".to_owned()];
    if let Some(proportional) = fonts.families.get(&egui::FontFamily::Proportional) {
        medium_chain.extend(proportional.iter().cloned());
    }
    fonts
        .families
        .insert(egui::FontFamily::Name("kanit-medium".into()), medium_chain);

    ctx.set_fonts(fonts);
    "Kanit (embedded: Regular + Medium)".to_string()
}

/// Light theme: CCS teal accent, rounded widgets, generous spacing, and larger
/// Kanit text (medium weight on headings & buttons) for a softer, readable UI.
fn setup_theme(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle};

    let mut visuals = egui::Visuals::light();
    visuals.selection.bg_fill = egui::Color32::from_rgb(0xB2, 0xEB, 0xF2); // light teal tint
    visuals.selection.stroke.color = ACCENT_STRONG;
    visuals.hyperlink_color = ACCENT_STRONG;
    visuals.widgets.hovered.bg_stroke.color = ACCENT;

    // Rounder corners across widgets and windows to soften the UI.
    let rounding = egui::Rounding::same(8.0);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.rounding = rounding;
    }
    visuals.window_rounding = egui::Rounding::same(10.0);
    visuals.menu_rounding = egui::Rounding::same(8.0);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();

    // Larger Kanit text; medium weight for headings and buttons.
    let medium = FontFamily::Name("kanit-medium".into());
    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, medium.clone())),
        (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(16.0, medium)),
        (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(15.0, FontFamily::Monospace)),
    ]
    .into();

    // More breathing room between and inside widgets.
    style.spacing.item_spacing = egui::vec2(10.0, 9.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    ctx.set_style(style);
}
