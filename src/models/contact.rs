//! The unified person record and its associated scoring / flow data.

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate};

use super::enums::{ContactType, Gender, NetworkCategory, Rank, SponsorStep};
use crate::utils::scoring;

/// A single person tracked by the app — prospect, customer, or ABO.
///
/// `rank` and `sponsor_id` are only meaningful when `contact_type == Abo`.
#[derive(Debug, Clone)]
pub struct Contact {
    pub id: i64,
    pub name: String,
    pub nickname: Option<String>,
    pub phone: Option<String>,
    pub line_id: Option<String>,
    pub age: Option<u8>,
    pub gender: Gender,
    pub address: Option<String>,
    pub network_category: NetworkCategory,
    pub contact_type: ContactType,
    /// Only set for ABOs.
    pub rank: Option<Rank>,
    /// Upline ABO id. Must reference a Contact whose type is ABO.
    pub sponsor_id: Option<i64>,
    pub created_at: DateTime<Local>,
    pub notes: Option<String>,
}

impl Contact {
    /// A blank contact with `id == 0` (used as the in-memory target before an
    /// insert assigns a real row id).
    pub fn new_blank() -> Self {
        Contact {
            id: 0,
            name: String::new(),
            nickname: None,
            phone: None,
            line_id: None,
            age: None,
            gender: Gender::Male,
            address: None,
            network_category: NetworkCategory::Friend,
            contact_type: ContactType::Prospect,
            rank: None,
            sponsor_id: None,
            created_at: Local::now(),
            notes: None,
        }
    }

    /// Display name combining the full name and nickname if present.
    pub fn display_name(&self) -> String {
        match &self.nickname {
            Some(nick) if !nick.is_empty() => format!("{} ({})", self.name, nick),
            _ => self.name.clone(),
        }
    }
}

/// Sponsor-List scoring for a prospect. `total` is derived, never trusted from
/// the caller — it is recomputed by [`ProspectScore::recompute`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProspectScore {
    pub contact_id: i64,
    pub relationship_closeness: u8, // 1-10
    pub financial_stability: u8,    // 1-5
    pub leadership: u8,             // 1-5
    pub financial_status: u8,       // 1-5
    pub accessibility: u8,          // 1-5
    pub total: u8,                  // computed (max 30, "high" >= 20)
}

impl ProspectScore {
    pub fn new(contact_id: i64) -> Self {
        let mut s = ProspectScore {
            contact_id,
            relationship_closeness: 1,
            financial_stability: 1,
            leadership: 1,
            financial_status: 1,
            accessibility: 1,
            total: 0,
        };
        s.recompute();
        s
    }

    /// Recompute and store `total` from the component fields.
    pub fn recompute(&mut self) {
        self.total = scoring::prospect_total(
            self.relationship_closeness,
            self.financial_stability,
            self.leadership,
            self.financial_status,
            self.accessibility,
        );
    }
}

/// Customer-Name-List scoring for a VIP customer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerScore {
    pub contact_id: i64,
    pub relationship_level: u8, // 1-10
    pub financial_status: u8,   // 1-5
    pub decision_power: u8,     // 1-5
    pub problems: String,
    pub total: u8, // computed (max 20, "high" >= 10)
}

impl CustomerScore {
    pub fn new(contact_id: i64) -> Self {
        let mut s = CustomerScore {
            contact_id,
            relationship_level: 1,
            financial_status: 1,
            decision_power: 1,
            problems: String::new(),
            total: 0,
        };
        s.recompute();
        s
    }

    pub fn recompute(&mut self) {
        self.total = scoring::customer_total(
            self.relationship_level,
            self.financial_status,
            self.decision_power,
        );
    }
}

/// Current position of a prospect in the 8-step sponsor flow, with the date each
/// step was reached.
#[derive(Debug, Clone)]
pub struct SponsorFlowStatus {
    pub contact_id: i64,
    pub current_step: SponsorStep,
    pub step_date: HashMap<SponsorStep, NaiveDate>,
    pub notes: String,
}

impl SponsorFlowStatus {
    /// A fresh flow at Step1 with today's date recorded.
    pub fn new(contact_id: i64) -> Self {
        let mut step_date = HashMap::new();
        step_date.insert(SponsorStep::Step1, Local::now().date_naive());
        SponsorFlowStatus {
            contact_id,
            current_step: SponsorStep::Step1,
            step_date,
            notes: String::new(),
        }
    }
}
