//! Domain models for the Amway CCS Tracker.
//!
//! * [`enums`] — closed sets used across the domain (gender, network category,
//!   contact type, rank, sponsor-flow step).
//! * [`contact`] — the unified person record plus prospect/customer scoring and
//!   the 8-step sponsor-flow status.
//! * [`followup`] — the BK1 / BK2 / C1 follow-up checklist per ABO.
//! * [`todo`] — a task, optionally tied to a contact, with a due date and done flag.
//! * [`advance`] — an advance payment fronted to buy products for a contact, collected back later.

pub mod activity;
pub mod advance;
pub mod contact;
pub mod enums;
pub mod followup;
pub mod todo;
