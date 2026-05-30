//! Database access layer.
//!
//! All SQL lives in [`queries`]; [`schema`] owns table creation and migrations.
//! [`DbConnection`] is the single owner of the live connection — there are no
//! global/static connections. UI code talks only to `DbConnection`.

pub mod queries;
pub mod schema;

use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;
use crate::models::activity::Activity;
use crate::models::contact::{Contact, CustomerScore, ProspectScore, SponsorFlowStatus};
use crate::models::enums::{ActivityKind, ContactType, SponsorStep};
use crate::models::followup::FollowUpSheet;
use queries::{AboRow, CustomerRow, ProspectRow};

/// Owns the SQLite connection and exposes typed, validated operations.
pub struct DbConnection {
    conn: Connection,
}

fn init(conn: &Connection) -> Result<()> {
    // Foreign keys must be enabled per-connection for cascades to fire.
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    schema::migrate(conn)?;
    Ok(())
}

impl DbConnection {
    /// Open (creating if needed) a database file at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        init(&conn)?;
        Ok(DbConnection { conn })
    }

    // --- contacts ---------------------------------------------------------

    pub fn insert_contact(&self, c: &Contact) -> Result<i64> {
        queries::insert_contact(&self.conn, c)
    }
    pub fn update_contact(&self, c: &Contact) -> Result<()> {
        queries::update_contact(&self.conn, c)
    }
    pub fn get_contact(&self, id: i64) -> Result<Contact> {
        queries::get_contact(&self.conn, id)
    }
    pub fn list_contacts(&self) -> Result<Vec<Contact>> {
        queries::list_contacts(&self.conn)
    }
    pub fn list_abos(&self) -> Result<Vec<Contact>> {
        queries::list_abos(&self.conn)
    }
    pub fn delete_contact(&self, id: i64) -> Result<()> {
        queries::delete_contact(&self.conn, id)
    }
    pub fn update_ppv(&self, id: i64, ppv: i64) -> Result<()> {
        queries::update_ppv(&self.conn, id, ppv)
    }
    pub fn abo_leg_counts(&self, abo_id: i64) -> Result<(usize, usize, usize)> {
        queries::abo_leg_counts(&self.conn, abo_id)
    }
    pub fn me_leg_counts(&self) -> Result<(usize, usize, usize)> {
        queries::me_leg_counts(&self.conn)
    }
    pub fn get_me_ppv(&self) -> Result<i64> {
        queries::get_me_ppv(&self.conn)
    }
    pub fn set_me_ppv(&self, ppv: i64) -> Result<()> {
        queries::set_me_ppv(&self.conn, ppv)
    }
    pub fn add_activity(&self, contact_id: i64, kind: ActivityKind, note: &str) -> Result<i64> {
        queries::add_activity(&self.conn, contact_id, kind, note)
    }
    pub fn list_activities(&self, contact_id: i64) -> Result<Vec<Activity>> {
        queries::list_activities(&self.conn, contact_id)
    }
    pub fn delete_activity(&self, id: i64) -> Result<()> {
        queries::delete_activity(&self.conn, id)
    }

    // --- scores -----------------------------------------------------------

    pub fn upsert_prospect_score(&self, s: &ProspectScore) -> Result<()> {
        queries::upsert_prospect_score(&self.conn, s)
    }
    pub fn get_prospect_score(&self, id: i64) -> Result<Option<ProspectScore>> {
        queries::get_prospect_score(&self.conn, id)
    }
    pub fn upsert_customer_score(&self, s: &CustomerScore) -> Result<()> {
        queries::upsert_customer_score(&self.conn, s)
    }
    pub fn get_customer_score(&self, id: i64) -> Result<Option<CustomerScore>> {
        queries::get_customer_score(&self.conn, id)
    }

    // --- sponsor flow -----------------------------------------------------

    pub fn get_sponsor_flow(&self, id: i64) -> Result<SponsorFlowStatus> {
        queries::get_sponsor_flow(&self.conn, id)
    }
    pub fn set_sponsor_step(&self, id: i64, step: SponsorStep) -> Result<()> {
        queries::set_sponsor_step(&self.conn, id, step)
    }
    pub fn set_sponsor_step_direct(&self, id: i64, step: SponsorStep) -> Result<()> {
        queries::set_sponsor_step_direct(&self.conn, id, step)
    }

    // --- follow-up --------------------------------------------------------

    pub fn get_follow_up(&self, id: i64) -> Result<FollowUpSheet> {
        queries::get_follow_up(&self.conn, id)
    }
    pub fn save_follow_up(&self, f: &FollowUpSheet) -> Result<()> {
        queries::save_follow_up(&self.conn, f)
    }

    // --- aggregates / table rows -----------------------------------------

    pub fn count_by_type(&self, ty: ContactType) -> Result<i64> {
        queries::count_by_type(&self.conn, ty)
    }
    pub fn count_conversions_this_month(&self) -> Result<i64> {
        queries::count_conversions_this_month(&self.conn)
    }
    pub fn list_prospect_rows(&self, q: &str) -> Result<Vec<ProspectRow>> {
        queries::list_prospect_rows(&self.conn, q)
    }
    pub fn list_customer_rows(&self, q: &str) -> Result<Vec<CustomerRow>> {
        queries::list_customer_rows(&self.conn, q)
    }
    pub fn list_abo_rows(&self, q: &str) -> Result<Vec<AboRow>> {
        queries::list_abo_rows(&self.conn, q)
    }
}
