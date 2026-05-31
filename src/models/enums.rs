//! Closed enumerations used across the domain.
//!
//! Each enum stores a stable string (or integer, for [`SponsorStep`]) in the
//! database via `as_str` / `as_int`, and parses back with an infallible
//! `from_db` that falls back to a sensible default for forward compatibility.
//! Thai display labels live in `label_th`.

/// Biological sex captured on the contact card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

impl Gender {
    pub const ALL: [Gender; 2] = [Gender::Male, Gender::Female];

    pub fn as_str(self) -> &'static str {
        match self {
            Gender::Male => "Male",
            Gender::Female => "Female",
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Gender::Male => "ชาย",
            Gender::Female => "หญิง",
        }
    }

    pub fn from_db(s: &str) -> Gender {
        match s {
            "Female" => Gender::Female,
            _ => Gender::Male,
        }
    }
}

/// "Your Network" relationship buckets (ครอบครัว, ญาติ, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkCategory {
    Family,
    Relative,
    Friend,
    Coworker,
    Partner,
    Acquaintance,
    Stranger,
}

impl NetworkCategory {
    pub const ALL: [NetworkCategory; 7] = [
        NetworkCategory::Family,
        NetworkCategory::Relative,
        NetworkCategory::Friend,
        NetworkCategory::Coworker,
        NetworkCategory::Partner,
        NetworkCategory::Acquaintance,
        NetworkCategory::Stranger,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NetworkCategory::Family => "Family",
            NetworkCategory::Relative => "Relative",
            NetworkCategory::Friend => "Friend",
            NetworkCategory::Coworker => "Coworker",
            NetworkCategory::Partner => "Partner",
            NetworkCategory::Acquaintance => "Acquaintance",
            NetworkCategory::Stranger => "Stranger",
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            NetworkCategory::Family => "ครอบครัว",
            NetworkCategory::Relative => "ญาติ",
            NetworkCategory::Friend => "เพื่อน",
            NetworkCategory::Coworker => "เพื่อนที่ทำงาน",
            NetworkCategory::Partner => "คู่ค้า",
            NetworkCategory::Acquaintance => "คนรู้จัก",
            NetworkCategory::Stranger => "คนแปลกหน้า",
        }
    }

    pub fn from_db(s: &str) -> NetworkCategory {
        match s {
            "Family" => NetworkCategory::Family,
            "Relative" => NetworkCategory::Relative,
            "Coworker" => NetworkCategory::Coworker,
            "Partner" => NetworkCategory::Partner,
            "Acquaintance" => NetworkCategory::Acquaintance,
            "Stranger" => NetworkCategory::Stranger,
            _ => NetworkCategory::Friend,
        }
    }
}

/// Where a person sits in the pipeline. A person is exactly one of these at a
/// time — Prospect and Customer are mutually exclusive by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactType {
    Prospect,
    Customer,
    Abo,
}

impl ContactType {
    pub const ALL: [ContactType; 3] =
        [ContactType::Prospect, ContactType::Customer, ContactType::Abo];

    pub fn as_str(self) -> &'static str {
        match self {
            ContactType::Prospect => "Prospect",
            ContactType::Customer => "Customer",
            ContactType::Abo => "ABO",
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            ContactType::Prospect => "ผู้มุ่งหวัง",
            ContactType::Customer => "ลูกค้า VIP",
            ContactType::Abo => "นักธุรกิจ (ABO)",
        }
    }

    pub fn from_db(s: &str) -> ContactType {
        match s {
            "Customer" => ContactType::Customer,
            "ABO" => ContactType::Abo,
            _ => ContactType::Prospect,
        }
    }
}

/// Rank in the "5 Steps to 21%" progression. Ordering matters: rank may only
/// advance, never regress (enforced in [`crate::utils::scoring`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Koc,
    C1,
    Cl,
    Cl15,
    Cl21,
}

impl Rank {
    pub const ALL: [Rank; 5] = [Rank::Koc, Rank::C1, Rank::Cl, Rank::Cl15, Rank::Cl21];

    /// 0-based position in the progression. Higher = more senior.
    pub fn ordinal(self) -> u8 {
        match self {
            Rank::Koc => 0,
            Rank::C1 => 1,
            Rank::Cl => 2,
            Rank::Cl15 => 3,
            Rank::Cl21 => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Rank::Koc => "KOC",
            Rank::C1 => "C1",
            Rank::Cl => "CL",
            Rank::Cl15 => "CL15",
            Rank::Cl21 => "CL21",
        }
    }

    pub fn label_th(self) -> &'static str {
        match self {
            Rank::Koc => "KOC (เริ่มต้น)",
            Rank::C1 => "C1",
            Rank::Cl => "CL (10,000 PPV)",
            Rank::Cl15 => "CL15 (20,000 PPV)",
            Rank::Cl21 => "CL21 (30,000 PPV)",
        }
    }

    pub fn from_db(s: &str) -> Rank {
        match s {
            "C1" => Rank::C1,
            "CL" => Rank::Cl,
            "CL15" => Rank::Cl15,
            "CL21" => Rank::Cl21,
            _ => Rank::Koc,
        }
    }

    /// Rank implied by a Personal Group PV figure, per the CCS thresholds:
    /// <5,000 = KOC, 5,000 = C1, 10,000 = CL, 20,000 = CL15, 30,000 = CL21.
    pub fn from_ppv(ppv: i64) -> Rank {
        if ppv >= 30_000 {
            Rank::Cl21
        } else if ppv >= 20_000 {
            Rank::Cl15
        } else if ppv >= 10_000 {
            Rank::Cl
        } else if ppv >= 5_000 {
            Rank::C1
        } else {
            Rank::Koc
        }
    }
}

/// One of the 8 sponsor-flow steps a prospect moves through. Stored as the
/// integer 1..=8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SponsorStep {
    Step1,
    Step2,
    Step3,
    Step4,
    Step5,
    Step6,
    Step7,
    Step8,
}

impl SponsorStep {
    pub const ALL: [SponsorStep; 8] = [
        SponsorStep::Step1,
        SponsorStep::Step2,
        SponsorStep::Step3,
        SponsorStep::Step4,
        SponsorStep::Step5,
        SponsorStep::Step6,
        SponsorStep::Step7,
        SponsorStep::Step8,
    ];

    /// 1-based step number as stored in the database.
    pub fn as_int(self) -> u8 {
        match self {
            SponsorStep::Step1 => 1,
            SponsorStep::Step2 => 2,
            SponsorStep::Step3 => 3,
            SponsorStep::Step4 => 4,
            SponsorStep::Step5 => 5,
            SponsorStep::Step6 => 6,
            SponsorStep::Step7 => 7,
            SponsorStep::Step8 => 8,
        }
    }

    pub fn from_int(n: i64) -> SponsorStep {
        match n {
            2 => SponsorStep::Step2,
            3 => SponsorStep::Step3,
            4 => SponsorStep::Step4,
            5 => SponsorStep::Step5,
            6 => SponsorStep::Step6,
            7 => SponsorStep::Step7,
            8 => SponsorStep::Step8,
            _ => SponsorStep::Step1,
        }
    }

    /// The next step, or `None` if already at Step8.
    pub fn next(self) -> Option<SponsorStep> {
        match self {
            SponsorStep::Step8 => None,
            other => Some(SponsorStep::from_int(other.as_int() as i64 + 1)),
        }
    }

    /// Short badge text, e.g. "S3".
    pub fn short(self) -> String {
        format!("S{}", self.as_int())
    }

    pub fn label_th(self) -> &'static str {
        match self {
            SponsorStep::Step1 => "จดรายชื่อ เช็คฟอร์มเบื้องต้น",
            SponsorStep::Step2 => "สร้างนัด",
            SponsorStep::Step3 => "เช็คฟอร์มหน้างาน ค้นหาความต้องการ",
            SponsorStep::Step4 => "เปิดใจ ชวนคิด",
            SponsorStep::Step5 => "เปิดภาพ สินค้า/ธุรกิจ",
            SponsorStep::Step6 => "ปิดการสมัคร",
            SponsorStep::Step7 => "นัดหมายติดตาม BK / พบอัพไลน์",
            SponsorStep::Step8 => "วิเคราะห์ ออกแบบ วางแผน",
        }
    }
}
