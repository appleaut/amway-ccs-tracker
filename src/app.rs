//! Application state, the eframe main loop, sidebar navigation, settings, and
//! one-time setup (fonts, theme, database location).

use std::path::PathBuf;

use crate::db::DbConnection;
use crate::error::{AppError, Result};
use crate::models::contact::{Contact, CustomerScore, ProspectScore};
use crate::models::enums::{ContactType, NetworkCategory, Rank, SponsorStep};
use crate::ui::forms::ContactForm;
use crate::ui::{self, View, ACCENT};

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
    /// Human-readable database location, shown in Settings.
    pub db_location: String,
    /// Name of the loaded Thai font (or a note if none was found).
    pub font_name: String,
    /// PV figure typed into the Settings rank calculator.
    pub pv_input: String,
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
            db_location: path.display().to_string(),
            font_name,
            pv_input: String::new(),
        })
    }

    /// Record an error for display.
    pub fn set_error<E: std::fmt::Display>(&mut self, e: E) {
        self.last_error = Some(e.to_string());
    }

    /// Record a transient status message (clears any prior error).
    pub fn set_status(&mut self, s: impl Into<String>) {
        self.last_error = None;
        self.status = Some(s.into());
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
        ui.label(egui::RichText::new("Amway CCS").color(ACCENT).size(22.0).strong());
        ui.label(egui::RichText::new("Prospect & Downline Tracker").size(11.0).weak());
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        let items = [
            (View::Dashboard, "🏠  แดชบอร์ด"),
            (View::Prospects, "🎯  ผู้มุ่งหวัง"),
            (View::Customers, "🛒  ลูกค้า VIP"),
            (View::FollowUp, "✅  ติดตามผล"),
            (View::Network, "🌳  เครือข่าย"),
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
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let error = self.last_error.clone();
        let status = self.status.clone();
        let mut clear = false;
        ui.horizontal(|ui| {
            if let Some(err) = error {
                ui.colored_label(egui::Color32::from_rgb(0xFF, 0x6E, 0x6E), format!("⚠ {err}"));
                if ui.small_button("✖ ล้าง").clicked() {
                    clear = true;
                }
            } else if let Some(s) = status {
                ui.colored_label(ACCENT, format!("✓ {s}"));
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
        if ui
            .add(egui::Button::new("📋 ใส่ข้อมูลตัวอย่าง (Load sample data)").fill(ACCENT))
            .clicked()
        {
            self.seed_sample();
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("ข้อมูลถูกบันทึกในเครื่อง (Local SQLite) ไม่มีการเชื่อมต่อเครือข่าย")
                .small()
                .weak(),
        );

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(egui::RichText::new("คำนวณระดับ/โบนัสจากยอด PV").strong());
        ui.horizontal(|ui| {
            ui.label("PV:");
            ui.add(
                egui::TextEdit::singleline(&mut self.pv_input)
                    .hint_text("เช่น 15000")
                    .desired_width(140.0),
            );
        });
        let trimmed = self.pv_input.trim();
        if let Ok(pv) = trimmed.parse::<i64>() {
            let rank = crate::utils::scoring::rank_for_ppv(pv);
            let bonus = crate::utils::scoring::bonus_percent_for_pv(pv);
            ui.label(
                egui::RichText::new(format!(
                    "ระดับ (Rank): {}  •  โบนัส (Bonus): {}%",
                    rank.as_str(),
                    bonus
                ))
                .color(ACCENT)
                .strong(),
            );
        } else if !trimmed.is_empty() {
            ui.weak("กรุณากรอกตัวเลข PV (numeric only)");
        }
    }

    fn seed_sample(&mut self) {
        match self.do_seed() {
            Ok(n) => self.set_status(format!("เพิ่มข้อมูลตัวอย่าง {n} รายการ")),
            Err(e) => self.set_error(e),
        }
    }

    /// Insert a small demo dataset: a 3-level ABO hierarchy plus prospects and
    /// customers with scores. Used by the Settings "Load sample data" button.
    fn do_seed(&mut self) -> Result<usize> {
        let mut count = 0;

        let abo = |name: &str, rank: Rank, sponsor: Option<i64>| -> Result<i64> {
            let mut c = Contact::new_blank();
            c.name = name.to_string();
            c.contact_type = ContactType::Abo;
            c.rank = Some(rank);
            c.sponsor_id = sponsor;
            c.network_category = NetworkCategory::Friend;
            self.db.insert_contact(&c)
        };

        let a = abo("พิชัย (ทีม A)", Rank::Cl21, None)?;
        count += 1;
        let b = abo("สมหญิง", Rank::Cl, Some(a))?;
        count += 1;
        let _c = abo("วีระ", Rank::C1, Some(b))?; // 3rd level: A -> สมหญิง -> วีระ
        count += 1;
        let _d = abo("กานดา", Rank::C1, Some(a))?;
        count += 1;

        // Prospects with scores and flow progress.
        let p1 = {
            let mut c = Contact::new_blank();
            c.name = "ธนวัฒน์".to_string();
            c.phone = Some("0890001111".to_string());
            c.network_category = NetworkCategory::Coworker;
            self.db.insert_contact(&c)?
        };
        count += 1;
        let mut s1 = ProspectScore::new(p1);
        s1.relationship_closeness = 9;
        s1.financial_stability = 4;
        s1.leadership = 4;
        s1.financial_status = 4;
        s1.accessibility = 5;
        self.db.upsert_prospect_score(&s1)?;
        self.db.set_sponsor_step(p1, SponsorStep::Step2)?;
        self.db.set_sponsor_step(p1, SponsorStep::Step3)?;

        let p2 = {
            let mut c = Contact::new_blank();
            c.name = "มาลี".to_string();
            c.phone = Some("0890002222".to_string());
            c.network_category = NetworkCategory::Relative;
            self.db.insert_contact(&c)?
        };
        count += 1;
        let mut s2 = ProspectScore::new(p2);
        s2.relationship_closeness = 6;
        s2.financial_stability = 3;
        s2.leadership = 2;
        s2.financial_status = 3;
        s2.accessibility = 4;
        self.db.upsert_prospect_score(&s2)?;

        // Customers with scores.
        let cu1 = {
            let mut c = Contact::new_blank();
            c.name = "อรุณี".to_string();
            c.phone = Some("0890003333".to_string());
            c.contact_type = ContactType::Customer;
            self.db.insert_contact(&c)?
        };
        count += 1;
        let mut cs1 = CustomerScore::new(cu1);
        cs1.relationship_level = 8;
        cs1.financial_status = 4;
        cs1.decision_power = 4;
        cs1.problems = "ปวดเข่า อยากดูแลสุขภาพ".to_string();
        self.db.upsert_customer_score(&cs1)?;

        Ok(count)
    }
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .exact_width(210.0)
            .show(ctx, |ui| self.sidebar(ui));

        egui::TopBottomPanel::bottom("status_panel").show(ctx, |ui| self.status_bar(ui));

        egui::CentralPanel::default().show(ctx, |ui| match self.view {
            View::Dashboard => ui::dashboard::render(self, ui),
            View::Prospects => ui::prospect_list::render(self, ui),
            View::Customers => ui::customer_list::render(self, ui),
            View::FollowUp => ui::followup::render(self, ui),
            View::Network => ui::downline_tree::render(self, ui),
            View::Settings => self.settings(ui),
        });

        // Modal form renders on top of whatever view is active.
        ui::forms::render(self, ctx);
    }
}

/// Resolve `%APPDATA%\AmwayCCSTracker\data.db`, creating the directory.
fn db_path() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map_err(|_| AppError::validation("APPDATA environment variable is not set"))?;
    let dir = PathBuf::from(base).join("AmwayCCSTracker");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("data.db"))
}

/// Install a Thai-capable font as the primary proportional/monospace family.
/// Returns the font description for the Settings screen.
fn setup_fonts(ctx: &egui::Context) -> String {
    const CANDIDATES: [(&str, &str); 3] = [
        ("Leelawadee UI", r"C:\Windows\Fonts\LeelawUI.ttf"),
        ("Leelawadee", r"C:\Windows\Fonts\leelawad.ttf"),
        ("Tahoma", r"C:\Windows\Fonts\tahoma.ttf"),
    ];

    let mut fonts = egui::FontDefinitions::default();
    for (name, path) in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("thai".to_owned(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "thai".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("thai".to_owned());
            ctx.set_fonts(fonts);
            return format!("{name} ({path})");
        }
    }
    // No Thai font found: keep egui defaults (Thai glyphs may be missing).
    "default (no Thai font found)".to_string()
}

/// Dark theme with the CCS teal accent applied to selections and links.
fn setup_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.55);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.hovered.bg_stroke.color = ACCENT;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    ctx.set_style(style);
}
