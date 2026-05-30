//! Scoring, rank, and state-transition rules.
//!
//! This module is deliberately free of any database or UI dependency so the
//! business rules can be unit-tested in isolation.

use crate::error::{AppError, Result};
use crate::models::enums::{Rank, SponsorStep};

/// Inclusive range for the "relationship" fields (1-10).
const REL_MIN: u8 = 1;
const REL_MAX: u8 = 10;
/// Inclusive range for the 1-5 scoring fields.
const FIVE_MIN: u8 = 1;
const FIVE_MAX: u8 = 5;

fn check_range(label: &str, value: u8, min: u8, max: u8) -> Result<()> {
    if value < min || value > max {
        return Err(AppError::validation(format!(
            "{label} must be between {min} and {max} (got {value})"
        )));
    }
    Ok(())
}

/// Validate every component field of a prospect score.
///
/// `relationship_closeness` is 1-10; the remaining four are 1-5.
pub fn validate_prospect_fields(
    relationship_closeness: u8,
    financial_stability: u8,
    leadership: u8,
    financial_status: u8,
    accessibility: u8,
) -> Result<()> {
    check_range("ความสัมพันธ์", relationship_closeness, REL_MIN, REL_MAX)?;
    check_range("ความมั่นคง", financial_stability, FIVE_MIN, FIVE_MAX)?;
    check_range("ความเป็นผู้นำ", leadership, FIVE_MIN, FIVE_MAX)?;
    check_range("สถานะการเงิน", financial_status, FIVE_MIN, FIVE_MAX)?;
    check_range("ติดต่อง่าย", accessibility, FIVE_MIN, FIVE_MAX)?;
    Ok(())
}

/// Sum of the five prospect-score components (max 30).
pub fn prospect_total(
    relationship_closeness: u8,
    financial_stability: u8,
    leadership: u8,
    financial_status: u8,
    accessibility: u8,
) -> u8 {
    relationship_closeness
        .saturating_add(financial_stability)
        .saturating_add(leadership)
        .saturating_add(financial_status)
        .saturating_add(accessibility)
}

/// Validate every component field of a customer score.
///
/// `relationship_level` is 1-10; the remaining two are 1-5.
pub fn validate_customer_fields(
    relationship_level: u8,
    financial_status: u8,
    decision_power: u8,
) -> Result<()> {
    check_range("สายสัมพันธ์", relationship_level, REL_MIN, REL_MAX)?;
    check_range("สถานะการเงิน", financial_status, FIVE_MIN, FIVE_MAX)?;
    check_range("อำนาจการตัดสินใจ", decision_power, FIVE_MIN, FIVE_MAX)?;
    Ok(())
}

/// Sum of the three customer-score components (max 20).
pub fn customer_total(relationship_level: u8, financial_status: u8, decision_power: u8) -> u8 {
    relationship_level
        .saturating_add(financial_status)
        .saturating_add(decision_power)
}

/// Performance-bonus percentage for a monthly business-volume (PV) figure,
/// using the CCS tier table. Returns the highest tier reached, or 0 below the
/// first tier.
pub fn bonus_percent_for_pv(pv: i64) -> u8 {
    const TIERS: [(i64, u8); 6] = [
        (5_000, 6),
        (15_000, 9),
        (30_000, 12),
        (55_000, 15),
        (90_000, 18),
        (150_000, 21),
    ];
    let mut pct = 0;
    for (threshold, percent) in TIERS {
        if pv >= threshold {
            pct = percent;
        }
    }
    pct
}

/// Rank implied by a Personal Group PV figure (see [`Rank::from_ppv`]).
pub fn rank_for_ppv(ppv: i64) -> Rank {
    Rank::from_ppv(ppv)
}

/// Highest rank an ABO qualifies for under the "5 Steps to 21%" rules, given
/// their PPV and how many direct downline legs reach each rank:
///
/// * C1  — PPV ≥ 5,000
/// * CL  — PPV ≥ 10,000 and ≥ 3 legs at C1 or above
/// * CL15 — PPV ≥ 20,000 and ≥ 3 legs at CL or above
/// * CL21 — PPV ≥ 30,000 and ≥ 3 legs at CL15 or above
pub fn qualified_rank(ppv: i64, c1_legs: usize, cl_legs: usize, cl15_legs: usize) -> Rank {
    if ppv >= 30_000 && cl15_legs >= 3 {
        Rank::Cl21
    } else if ppv >= 20_000 && cl_legs >= 3 {
        Rank::Cl15
    } else if ppv >= 10_000 && c1_legs >= 3 {
        Rank::Cl
    } else if ppv >= 5_000 {
        Rank::C1
    } else {
        Rank::Koc
    }
}

/// Requirement to reach `rank`: `(min PPV, required leg rank, required leg count)`.
/// `None` for KOC (entry level). C1 has no leg requirement (count 0).
pub fn rank_requirement(rank: Rank) -> Option<(i64, Rank, usize)> {
    match rank {
        Rank::Koc => None,
        Rank::C1 => Some((5_000, Rank::C1, 0)),
        Rank::Cl => Some((10_000, Rank::C1, 3)),
        Rank::Cl15 => Some((20_000, Rank::Cl, 3)),
        Rank::Cl21 => Some((30_000, Rank::Cl15, 3)),
    }
}

/// A rank may only advance or hold — never regress.
pub fn validate_rank_transition(from: Rank, to: Rank) -> Result<()> {
    if to.ordinal() < from.ordinal() {
        return Err(AppError::validation(format!(
            "ไม่สามารถลดระดับจาก {} เป็น {} ได้ (rank cannot regress)",
            from.as_str(),
            to.as_str()
        )));
    }
    Ok(())
}

/// The sponsor flow advances one step at a time and never skips ahead.
///
/// Moving forward is only valid to the immediately following step. Moving back
/// (to correct a mistake) or staying on the same step is permitted.
pub fn validate_step_transition(from: SponsorStep, to: SponsorStep) -> Result<()> {
    let (f, t) = (from.as_int() as i16, to.as_int() as i16);
    if t > f + 1 {
        return Err(AppError::validation(format!(
            "ต้องทำตามลำดับ ข้ามจาก Step{} ไป Step{} ไม่ได้ (cannot skip steps)",
            f, t
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prospect_total_is_sum_of_all_fields() {
        // 10 + 5 + 4 + 3 + 2 = 24
        assert_eq!(prospect_total(10, 5, 4, 3, 2), 24);
        assert_eq!(prospect_total(1, 1, 1, 1, 1), 5);
        // Maximum possible.
        assert_eq!(prospect_total(10, 5, 5, 5, 5), 30);
    }

    #[test]
    fn customer_total_is_sum_of_all_fields() {
        // 10 + 5 + 5 = 20
        assert_eq!(customer_total(10, 5, 5), 20);
        assert_eq!(customer_total(1, 1, 1), 3);
    }

    #[test]
    fn prospect_field_out_of_range_is_rejected() {
        // relationship max is 10
        assert!(validate_prospect_fields(11, 1, 1, 1, 1).is_err());
        // a 1-5 field given 6
        assert!(validate_prospect_fields(5, 6, 1, 1, 1).is_err());
        // zero is below the minimum of 1
        assert!(validate_prospect_fields(5, 0, 1, 1, 1).is_err());
        // a fully valid set passes
        assert!(validate_prospect_fields(10, 5, 5, 5, 5).is_ok());
    }

    #[test]
    fn customer_field_out_of_range_is_rejected() {
        assert!(validate_customer_fields(11, 1, 1).is_err());
        assert!(validate_customer_fields(5, 6, 1).is_err());
        assert!(validate_customer_fields(5, 5, 5).is_ok());
    }

    #[test]
    fn rank_progression_thresholds() {
        assert_eq!(rank_for_ppv(0), Rank::Koc);
        assert_eq!(rank_for_ppv(4_999), Rank::Koc);
        assert_eq!(rank_for_ppv(5_000), Rank::C1); // 5000 PV => C1
        assert_eq!(rank_for_ppv(10_000), Rank::Cl); // CL
        assert_eq!(rank_for_ppv(20_000), Rank::Cl15); // CL15
        assert_eq!(rank_for_ppv(30_000), Rank::Cl21); // CL21
        assert_eq!(rank_for_ppv(999_999), Rank::Cl21);
    }

    #[test]
    fn qualified_rank_needs_both_ppv_and_legs() {
        // PPV alone gets you to C1.
        assert_eq!(qualified_rank(5_000, 0, 0, 0), Rank::C1);
        assert_eq!(qualified_rank(4_999, 9, 9, 9), Rank::Koc);
        // CL needs PPV 10k AND 3 C1+ legs — missing either keeps you at C1.
        assert_eq!(qualified_rank(10_000, 2, 0, 0), Rank::C1);
        assert_eq!(qualified_rank(9_000, 3, 0, 0), Rank::C1);
        assert_eq!(qualified_rank(10_000, 3, 0, 0), Rank::Cl);
        // CL15 needs 20k + 3 CL legs.
        assert_eq!(qualified_rank(20_000, 3, 2, 0), Rank::Cl);
        assert_eq!(qualified_rank(20_000, 3, 3, 0), Rank::Cl15);
        // CL21 needs 30k + 3 CL15 legs.
        assert_eq!(qualified_rank(30_000, 5, 5, 2), Rank::Cl15);
        assert_eq!(qualified_rank(30_000, 5, 5, 3), Rank::Cl21);
    }

    #[test]
    fn bonus_percent_tiers() {
        assert_eq!(bonus_percent_for_pv(0), 0);
        assert_eq!(bonus_percent_for_pv(5_000), 6);
        assert_eq!(bonus_percent_for_pv(15_000), 9);
        assert_eq!(bonus_percent_for_pv(30_000), 12);
        assert_eq!(bonus_percent_for_pv(55_000), 15);
        assert_eq!(bonus_percent_for_pv(90_000), 18);
        assert_eq!(bonus_percent_for_pv(150_000), 21);
        assert_eq!(bonus_percent_for_pv(200_000), 21);
        // One unit below each threshold must stay on the lower tier.
        assert_eq!(bonus_percent_for_pv(4_999), 0);
        assert_eq!(bonus_percent_for_pv(14_999), 6);
        assert_eq!(bonus_percent_for_pv(29_999), 9);
        assert_eq!(bonus_percent_for_pv(54_999), 12);
        assert_eq!(bonus_percent_for_pv(89_999), 15);
        assert_eq!(bonus_percent_for_pv(149_999), 18);
    }

    #[test]
    fn rank_cannot_regress() {
        assert!(validate_rank_transition(Rank::Cl, Rank::C1).is_err());
        assert!(validate_rank_transition(Rank::Koc, Rank::C1).is_ok());
        assert!(validate_rank_transition(Rank::Cl, Rank::Cl).is_ok()); // hold
        assert!(validate_rank_transition(Rank::C1, Rank::Cl21).is_ok());
    }

    #[test]
    fn sponsor_step_must_advance_sequentially() {
        // cannot skip Step1 -> Step5
        assert!(validate_step_transition(SponsorStep::Step1, SponsorStep::Step5).is_err());
        // one step forward is fine
        assert!(validate_step_transition(SponsorStep::Step1, SponsorStep::Step2).is_ok());
        // moving back to correct a mistake is allowed
        assert!(validate_step_transition(SponsorStep::Step5, SponsorStep::Step3).is_ok());
        // staying put is allowed
        assert!(validate_step_transition(SponsorStep::Step3, SponsorStep::Step3).is_ok());
    }
}
