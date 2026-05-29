//! The BK1 → BK2 → C1 follow-up checklist tracked per ABO.

use chrono::{DateTime, Local};

/// Per-ABO follow-up sheet. Each boolean is one checklist item from the CCS
/// Follow-Up Sheet. The 26 items split across four sections (BK1, BK2, C1
/// Qualification, CCS Conference); section boundaries are described by
/// [`FollowUpSheet::SECTIONS`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUpSheet {
    pub contact_id: i64,

    // --- BK1 (9 items) ---
    pub bk1_jumpstart1: bool,
    pub bk1_core_plan: bool,
    pub bk1_why_amway: bool,
    pub bk1_why_nutrilite: bool,
    pub bk1_closed: bool,
    pub bk1_jumpstart2: bool,
    pub bk1_why_artistry: bool,
    pub bk1_smart_home_tech: bool,
    pub bk1_aec_health: bool,

    // --- BK2 (7 items) ---
    pub bk2_jumpstart3: bool,
    pub bk2_space_to_grow: bool,
    pub bk2_100_dreams: bool,
    pub bk2_5f1f: bool,
    pub bk2_name_list: bool,
    pub bk2_study_table: bool,
    pub bk2_analysis: bool,

    // --- C1 Qualification (7 items) ---
    pub c1_link3: bool,
    pub c1_weekly_meeting: bool,
    pub c1_ccs_seminar: bool,
    pub c1_auto_renewal: bool,
    pub c1_sop: bool,
    pub c1_1abo: bool,
    pub c1_5000pv: bool,

    // --- CCS Conference (3 items) ---
    pub conf_crack_code: bool,
    pub conf_5stars: bool,
    pub conf_spirit: bool,

    pub updated_at: DateTime<Local>,
}

impl FollowUpSheet {
    /// Total number of checklist items (BK1: 9, BK2: 7, C1: 7, Conference: 3).
    pub const TOTAL: usize = 26;

    /// An empty sheet for the given contact.
    pub fn new(contact_id: i64) -> Self {
        FollowUpSheet {
            contact_id,
            bk1_jumpstart1: false,
            bk1_core_plan: false,
            bk1_why_amway: false,
            bk1_why_nutrilite: false,
            bk1_closed: false,
            bk1_jumpstart2: false,
            bk1_why_artistry: false,
            bk1_smart_home_tech: false,
            bk1_aec_health: false,
            bk2_jumpstart3: false,
            bk2_space_to_grow: false,
            bk2_100_dreams: false,
            bk2_5f1f: false,
            bk2_name_list: false,
            bk2_study_table: false,
            bk2_analysis: false,
            c1_link3: false,
            c1_weekly_meeting: false,
            c1_ccs_seminar: false,
            c1_auto_renewal: false,
            c1_sop: false,
            c1_1abo: false,
            c1_5000pv: false,
            conf_crack_code: false,
            conf_5stars: false,
            conf_spirit: false,
            updated_at: Local::now(),
        }
    }

    /// All 26 flags in display order. Order matches [`FollowUpSheet::SECTIONS`].
    pub fn flags(&self) -> [bool; Self::TOTAL] {
        [
            self.bk1_jumpstart1,
            self.bk1_core_plan,
            self.bk1_why_amway,
            self.bk1_why_nutrilite,
            self.bk1_closed,
            self.bk1_jumpstart2,
            self.bk1_why_artistry,
            self.bk1_smart_home_tech,
            self.bk1_aec_health,
            self.bk2_jumpstart3,
            self.bk2_space_to_grow,
            self.bk2_100_dreams,
            self.bk2_5f1f,
            self.bk2_name_list,
            self.bk2_study_table,
            self.bk2_analysis,
            self.c1_link3,
            self.c1_weekly_meeting,
            self.c1_ccs_seminar,
            self.c1_auto_renewal,
            self.c1_sop,
            self.c1_1abo,
            self.c1_5000pv,
            self.conf_crack_code,
            self.conf_5stars,
            self.conf_spirit,
        ]
    }

    /// Number of checked items.
    pub fn done_count(&self) -> usize {
        self.flags().iter().filter(|&&b| b).count()
    }

    /// Completion fraction in `0.0..=1.0`, for the progress bar.
    pub fn fraction(&self) -> f32 {
        self.done_count() as f32 / Self::TOTAL as f32
    }
}
