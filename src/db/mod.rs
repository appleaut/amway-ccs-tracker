//! Database access layer.
//!
//! All SQL lives in [`queries`]; [`schema`] owns table creation and migrations.
//! [`DbConnection`] is the single owner of the live connection — there are no
//! global/static connections. UI code talks only to `DbConnection`.

pub mod queries;
pub mod schema;

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;

use rusqlite::{params, Connection};

use crate::error::{AppError, Result};
use crate::models::activity::Activity;
use crate::models::advance::Advance;
use crate::models::contact::{Contact, CustomerScore, ProspectScore, SponsorFlowStatus};
use crate::models::enums::{AttendeeStatus, ContactType, SponsorStep};
use crate::models::followup::FollowUpSheet;
use crate::models::meeting::{Meeting, MeetingAttendee};
use crate::models::todo::Todo;
use crate::models::todo_schedule::{Recurrence, TodoSchedule};
use queries::{
    AboRow, ActivityKindRow, ActivityLogRow, AdvanceRow, CustomerRow, ProspectRow, TodoRow,
    TodoScheduleRow,
};

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

    /// Write a clean, compact, consistent copy of the live database to `dest`
    /// using SQLite `VACUUM INTO`. The connection stays open. `VACUUM INTO`
    /// refuses a pre-existing destination, so an existing `dest` (the OS Save
    /// dialog already got the user's overwrite consent) is removed first.
    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        // Remove any existing destination (VACUUM INTO refuses a pre-existing
        // file). Unconditional + ignore-NotFound avoids a TOCTOU race.
        if let Err(e) = std::fs::remove_file(dest) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e.into());
            }
        }
        let dest_str = dest.to_str().ok_or_else(|| {
            AppError::validation("เส้นทางไฟล์ไม่ถูกต้อง (มีอักขระที่ไม่รองรับ)")
        })?;
        self.conn.execute("VACUUM INTO ?1", params![dest_str])?;
        Ok(())
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
    pub fn add_activity(&self, contact_id: i64, kind: &str, note: &str) -> Result<i64> {
        queries::add_activity(&self.conn, contact_id, kind, note)
    }
    pub fn list_activities(&self, contact_id: i64) -> Result<Vec<Activity>> {
        queries::list_activities(&self.conn, contact_id)
    }
    pub fn delete_activity(&self, id: i64) -> Result<()> {
        queries::delete_activity(&self.conn, id)
    }
    pub fn list_all_activities(&self, q: &str) -> Result<Vec<ActivityLogRow>> {
        queries::list_all_activities(&self.conn, q)
    }

    // --- activity kinds (user-managed types) ------------------------------

    pub fn list_activity_kinds(&self) -> Result<Vec<ActivityKindRow>> {
        queries::list_activity_kinds(&self.conn)
    }
    pub fn add_activity_kind(&self, name: &str) -> Result<i64> {
        queries::add_activity_kind(&self.conn, name)
    }
    pub fn rename_activity_kind(&self, id: i64, name: &str) -> Result<()> {
        queries::rename_activity_kind(&self.conn, id, name)
    }
    pub fn delete_activity_kind(&self, id: i64) -> Result<()> {
        queries::delete_activity_kind(&self.conn, id)
    }
    pub fn activity_kind_usage(&self, name: &str) -> Result<i64> {
        queries::activity_kind_usage(&self.conn, name)
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

    // --- todos ------------------------------------------------------------

    pub fn add_todo(
        &self,
        contact_id: Option<i64>,
        task: &str,
        due_date: Option<NaiveDate>,
    ) -> Result<i64> {
        queries::add_todo(&self.conn, contact_id, task, due_date)
    }
    pub fn update_todo(&self, t: &Todo) -> Result<()> {
        queries::update_todo(&self.conn, t)
    }
    pub fn set_todo_done(&self, id: i64, done: bool) -> Result<()> {
        queries::set_todo_done(&self.conn, id, done)
    }
    pub fn complete_todo(&self, id: i64, result: &str) -> Result<()> {
        queries::complete_todo(&self.conn, id, result)
    }
    pub fn delete_todo(&self, id: i64) -> Result<()> {
        queries::delete_todo(&self.conn, id)
    }
    pub fn list_todos(&self, query: &str) -> Result<Vec<TodoRow>> {
        queries::list_todos(&self.conn, query)
    }
    pub fn count_overdue_todos(&self) -> Result<i64> {
        queries::count_overdue_todos(&self.conn)
    }
    /// Count of unfinished todos due within the next `days` days (the dashboard's
    /// "due soon" card).
    pub fn count_due_soon_todos(&self, days: i64) -> Result<i64> {
        queries::count_due_soon_todos(&self.conn, days)
    }

    // --- todo schedules (recurring tasks) ---------------------------------

    pub fn add_todo_schedule(
        &self,
        contact_id: Option<i64>,
        task: &str,
        recurrence: Recurrence,
        start_date: NaiveDate,
    ) -> Result<i64> {
        queries::add_todo_schedule(&self.conn, contact_id, task, recurrence, start_date)
    }
    pub fn update_todo_schedule(&self, s: &TodoSchedule) -> Result<()> {
        queries::update_todo_schedule(&self.conn, s)
    }
    pub fn delete_todo_schedule(&self, id: i64) -> Result<()> {
        queries::delete_todo_schedule(&self.conn, id)
    }
    pub fn list_todo_schedules(&self) -> Result<Vec<TodoScheduleRow>> {
        queries::list_todo_schedules(&self.conn)
    }
    pub fn generate_due_todos(&self, today: NaiveDate) -> Result<usize> {
        queries::generate_due_todos(&self.conn, today)
    }

    // --- advances ---------------------------------------------------------

    pub fn add_advance(
        &self,
        contact_id: Option<i64>,
        item: &str,
        amount: i64,
        advance_date: NaiveDate,
        note: &str,
    ) -> Result<i64> {
        queries::add_advance(&self.conn, contact_id, item, amount, advance_date, note)
    }
    pub fn update_advance(&self, a: &Advance) -> Result<()> {
        queries::update_advance(&self.conn, a)
    }
    pub fn collect_advance(&self, id: i64, collected_date: NaiveDate, note: &str) -> Result<()> {
        queries::collect_advance(&self.conn, id, collected_date, note)
    }
    pub fn delete_advance(&self, id: i64) -> Result<()> {
        queries::delete_advance(&self.conn, id)
    }
    pub fn list_advances(
        &self,
        query: &str,
        collected_filter: Option<bool>,
    ) -> Result<Vec<AdvanceRow>> {
        queries::list_advances(&self.conn, query, collected_filter)
    }
    pub fn outstanding_total(&self) -> Result<i64> {
        queries::outstanding_total(&self.conn)
    }

    // --- meetings ---------------------------------------------------------

    pub fn add_meeting(
        &self,
        name: &str,
        start: NaiveDate,
        end: NaiveDate,
        description: &str,
        fee: i64,
    ) -> Result<i64> {
        queries::add_meeting(&self.conn, name, start, end, description, fee)
    }
    pub fn update_meeting(&self, m: &Meeting) -> Result<()> {
        queries::update_meeting(&self.conn, m)
    }
    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        queries::delete_meeting(&self.conn, id)
    }
    pub fn list_meetings(&self, include_past: bool) -> Result<Vec<Meeting>> {
        queries::list_meetings(&self.conn, include_past)
    }
    pub fn attendee_map(&self) -> Result<HashMap<(i64, i64), MeetingAttendee>> {
        queries::attendee_map(&self.conn)
    }
    pub fn upsert_attendee(
        &self,
        meeting_id: i64,
        contact_id: i64,
        status: AttendeeStatus,
        paid: bool,
        attended: Option<bool>,
    ) -> Result<()> {
        queries::upsert_attendee(&self.conn, meeting_id, contact_id, status, paid, attended)
    }
    pub fn remove_attendee(&self, meeting_id: i64, contact_id: i64) -> Result<()> {
        queries::remove_attendee(&self.conn, meeting_id, contact_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::contact::Contact;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A unique temp path that won't collide across parallel tests (no RNG/clock,
    /// which are unavailable/forbidden — use pid + a counter).
    fn temp_path(tag: &str) -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "amway_db_test_{}_{}_{}.db",
            std::process::id(),
            tag,
            n
        ))
    }

    #[test]
    fn backup_to_copies_live_data() {
        let live = temp_path("live");
        let copy = temp_path("copy");

        let db = DbConnection::open(&live).unwrap();
        let mut c = Contact::new_blank();
        c.name = "สมหญิง".to_string();
        db.insert_contact(&c).unwrap();

        db.backup_to(&copy).unwrap();

        let restored = DbConnection::open(&copy).unwrap();
        let names: Vec<String> = restored
            .list_contacts()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert!(names.contains(&"สมหญิง".to_string()));
        drop(restored); // release the file handle before overwriting (Windows)

        // Backing up again over the now-existing destination must succeed
        // (exercises the pre-existing-file removal branch).
        db.backup_to(&copy).unwrap();

        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&copy);
    }
}
