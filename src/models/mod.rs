//! Domain models for the Amway CCS Tracker.
//!
//! * [`enums`] — closed sets used across the domain (gender, network category,
//!   contact type, rank, sponsor-flow step).
//! * [`contact`] — the unified person record plus prospect/customer scoring and
//!   the 8-step sponsor-flow status.
//! * [`followup`] — the BK1 / BK2 / C1 follow-up checklist per ABO.

pub mod contact;
pub mod enums;
pub mod followup;
