//! Typed SQL queries.
//!
//! Every function takes a borrowed [`rusqlite::Connection`] so the same code is
//! exercised by the in-memory integration tests and by the live
//! [`super::DbConnection`] wrapper. Business rules (sponsor must be an ABO,
//! score ranges, sequential sponsor steps, non-regressing rank, Prospect/Customer
//! exclusivity) are enforced here, not just in the UI.

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, Result};
use crate::models::activity::Activity;
use crate::models::contact::{Contact, CustomerScore, ProspectScore, SponsorFlowStatus};
use crate::models::enums::{ContactType, Gender, NetworkCategory, Rank, SponsorStep};
use crate::models::followup::FollowUpSheet;
use crate::models::todo::Todo;
use crate::utils::scoring;

/// The 14 contact columns, qualified with the `c` alias so queries can join
/// other tables (which share column names such as `notes`) without ambiguity.
const C: &str = "c.id, c.name, c.nickname, c.phone, c.line_id, c.age, c.gender, \
                 c.address, c.network_category, c.contact_type, c.rank, \
                 c.sponsor_id, c.created_at, c.notes, c.ppv, c.member_no, c.abo_no";

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn parse_dt(s: &str) -> DateTime<Local> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Local))
        .unwrap_or_else(|_| Local::now())
}

/// Map the first 17 columns of a row (in `C` order) into a [`Contact`].
fn row_to_contact(row: &Row) -> rusqlite::Result<Contact> {
    let age: Option<i64> = row.get(5)?;
    let gender: String = row.get(6)?;
    let netcat: String = row.get(8)?;
    let ctype: String = row.get(9)?;
    let rank: Option<String> = row.get(10)?;
    let created: String = row.get(12)?;

    Ok(Contact {
        id: row.get(0)?,
        name: row.get(1)?,
        nickname: row.get(2)?,
        phone: row.get(3)?,
        line_id: row.get(4)?,
        age: age.map(|a| a as u8),
        gender: Gender::from_db(&gender),
        address: row.get(7)?,
        network_category: NetworkCategory::from_db(&netcat),
        contact_type: ContactType::from_db(&ctype),
        rank: rank.map(|r| Rank::from_db(&r)),
        sponsor_id: row.get(11)?,
        created_at: parse_dt(&created),
        notes: row.get(13)?,
        ppv: row.get(14)?,
        member_no: row.get(15)?,
        abo_no: row.get(16)?,
    })
}

// ---------------------------------------------------------------------------
// Internal validation helpers
// ---------------------------------------------------------------------------

fn contact_type_of(conn: &Connection, id: i64) -> Result<ContactType> {
    let t: Option<String> = conn
        .query_row("SELECT contact_type FROM contacts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    match t {
        Some(s) => Ok(ContactType::from_db(&s)),
        None => Err(AppError::NotFound(format!("contact id {id}"))),
    }
}

/// Enforce that, if a `sponsor_id` is set, it points at an existing ABO and is
/// not the contact itself.
fn ensure_sponsor_valid(
    conn: &Connection,
    self_id: Option<i64>,
    sponsor_id: Option<i64>,
) -> Result<()> {
    let Some(sid) = sponsor_id else { return Ok(()) };
    if Some(sid) == self_id {
        return Err(AppError::validation("contact cannot be its own sponsor"));
    }
    match contact_type_of(conn, sid) {
        Ok(ContactType::Abo) => Ok(()),
        Ok(_) => Err(AppError::validation(
            "sponsor_id must reference an ABO (อัพไลน์ต้องเป็นนักธุรกิจ)",
        )),
        Err(AppError::NotFound(_)) => {
            Err(AppError::validation(format!("sponsor id {sid} does not exist")))
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Contacts
// ---------------------------------------------------------------------------

/// Insert a new contact and return its assigned id.
pub fn insert_contact(conn: &Connection, c: &Contact) -> Result<i64> {
    ensure_sponsor_valid(conn, None, c.sponsor_id)?;
    conn.execute(
        "INSERT INTO contacts
            (name, nickname, phone, line_id, age, gender, address,
             network_category, contact_type, rank, sponsor_id, created_at, notes, ppv,
             member_no, abo_no)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            c.name,
            c.nickname,
            c.phone,
            c.line_id,
            c.age,
            c.gender.as_str(),
            c.address,
            c.network_category.as_str(),
            c.contact_type.as_str(),
            c.rank.map(|r| r.as_str()),
            c.sponsor_id,
            c.created_at.to_rfc3339(),
            c.notes,
            c.ppv,
            c.member_no,
            c.abo_no,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update an existing contact. Enforces non-regressing rank and drops the
/// opposing score row when the contact type flips between Prospect and Customer
/// (keeping the two mutually exclusive).
pub fn update_contact(conn: &Connection, c: &Contact) -> Result<()> {
    ensure_sponsor_valid(conn, Some(c.id), c.sponsor_id)?;

    let old = get_contact(conn, c.id)?;
    if let (Some(old_rank), Some(new_rank)) = (old.rank, c.rank) {
        scoring::validate_rank_transition(old_rank, new_rank)?;
    }

    conn.execute(
        "UPDATE contacts SET
            name = ?1, nickname = ?2, phone = ?3, line_id = ?4, age = ?5,
            gender = ?6, address = ?7, network_category = ?8, contact_type = ?9,
            rank = ?10, sponsor_id = ?11, notes = ?12, ppv = ?13,
            member_no = ?14, abo_no = ?15
         WHERE id = ?16",
        params![
            c.name,
            c.nickname,
            c.phone,
            c.line_id,
            c.age,
            c.gender.as_str(),
            c.address,
            c.network_category.as_str(),
            c.contact_type.as_str(),
            c.rank.map(|r| r.as_str()),
            c.sponsor_id,
            c.notes,
            c.ppv,
            c.member_no,
            c.abo_no,
            c.id,
        ],
    )?;

    // Maintain Prospect/Customer exclusivity at the data level.
    match c.contact_type {
        ContactType::Prospect => {
            conn.execute("DELETE FROM customer_scores WHERE contact_id = ?1", [c.id])?;
        }
        ContactType::Customer => {
            conn.execute("DELETE FROM prospect_scores WHERE contact_id = ?1", [c.id])?;
        }
        ContactType::Abo => {}
    }
    Ok(())
}

pub fn get_contact(conn: &Connection, id: i64) -> Result<Contact> {
    conn.query_row(
        &format!("SELECT {C} FROM contacts c WHERE c.id = ?1"),
        [id],
        row_to_contact,
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("contact id {id}")))
}

pub fn list_contacts(conn: &Connection) -> Result<Vec<Contact>> {
    let sql = format!("SELECT {C} FROM contacts c ORDER BY c.name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_contact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_by_type(conn: &Connection, ty: ContactType) -> Result<Vec<Contact>> {
    let sql = format!("SELECT {C} FROM contacts c WHERE c.contact_type = ?1 ORDER BY c.name");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([ty.as_str()], row_to_contact)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_abos(conn: &Connection) -> Result<Vec<Contact>> {
    list_by_type(conn, ContactType::Abo)
}

/// Delete a contact. Scores / follow-up / flow rows cascade; any downline's
/// `sponsor_id` is set to NULL by the schema.
pub fn delete_contact(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM contacts WHERE id = ?1", [id])?;
    Ok(())
}

/// Update only the Personal Point Value (PPV) of a contact.
pub fn update_ppv(conn: &Connection, id: i64, ppv: i64) -> Result<()> {
    conn.execute("UPDATE contacts SET ppv = ?1 WHERE id = ?2", params![ppv, id])?;
    Ok(())
}

/// Count an ABO's direct downline legs that reach at least C1 / CL / CL15.
/// Returns `(c1_plus, cl_plus, cl15_plus)` — used by the rank advisor.
pub fn abo_leg_counts(conn: &Connection, abo_id: i64) -> Result<(usize, usize, usize)> {
    let mut stmt =
        conn.prepare("SELECT rank FROM contacts WHERE sponsor_id = ?1 AND contact_type = 'ABO'")?;
    let ranks = stmt.query_map([abo_id], |row| {
        let s: Option<String> = row.get(0)?;
        Ok(s)
    })?;
    let (mut c1, mut cl, mut cl15) = (0usize, 0usize, 0usize);
    for r in ranks {
        let rank = r?.map(|s| Rank::from_db(&s)).unwrap_or(Rank::Koc);
        let o = rank.ordinal();
        if o >= Rank::C1.ordinal() {
            c1 += 1;
        }
        if o >= Rank::Cl.ordinal() {
            cl += 1;
        }
        if o >= Rank::Cl15.ordinal() {
            cl15 += 1;
        }
    }
    Ok((c1, cl, cl15))
}

/// Count *my own* direct downline legs — ABOs sponsored by "me" (i.e. with no
/// stored sponsor) — that reach at least C1 / CL / CL15. The root-level mirror
/// of [`abo_leg_counts`], used by the self Rank Advisor.
pub fn me_leg_counts(conn: &Connection) -> Result<(usize, usize, usize)> {
    let mut stmt = conn
        .prepare("SELECT rank FROM contacts WHERE sponsor_id IS NULL AND contact_type = 'ABO'")?;
    let ranks = stmt.query_map([], |row| {
        let s: Option<String> = row.get(0)?;
        Ok(s)
    })?;
    let (mut c1, mut cl, mut cl15) = (0usize, 0usize, 0usize);
    for r in ranks {
        let rank = r?.map(|s| Rank::from_db(&s)).unwrap_or(Rank::Koc);
        let o = rank.ordinal();
        if o >= Rank::C1.ordinal() {
            c1 += 1;
        }
        if o >= Rank::Cl.ordinal() {
            cl += 1;
        }
        if o >= Rank::Cl15.ordinal() {
            cl15 += 1;
        }
    }
    Ok((c1, cl, cl15))
}

/// Read my own Personal PV from the `meta` store (0 if never set).
pub fn get_me_ppv(conn: &Connection) -> Result<i64> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'me_ppv'", [], |r| r.get(0))
        .optional()?;
    Ok(v.and_then(|s| s.parse::<i64>().ok()).unwrap_or(0))
}

/// Persist my own Personal PV into the `meta` store.
pub fn set_me_ppv(conn: &Connection, ppv: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('me_ppv', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![ppv.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Activity history
// ---------------------------------------------------------------------------

/// Log an interaction with a contact; returns the new activity id.
pub fn add_activity(conn: &Connection, contact_id: i64, kind: &str, note: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO activities (contact_id, kind, note, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![contact_id, kind, note, Local::now().to_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// All activities for a contact, newest first.
pub fn list_activities(conn: &Connection, contact_id: i64) -> Result<Vec<Activity>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, note, created_at
         FROM activities WHERE contact_id = ?1
         ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([contact_id], |row| {
        let created: String = row.get(3)?;
        Ok(Activity {
            id: row.get(0)?,
            kind: row.get(1)?,
            note: row.get(2)?,
            created_at: parse_dt(&created),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete_activity(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM activities WHERE id = ?1", [id])?;
    Ok(())
}

/// One activity joined with its contact, for the aggregate history view.
pub struct ActivityLogRow {
    pub activity: Activity,
    pub contact_id: i64,
    pub contact_name: String,
    pub contact_type: ContactType,
}

/// Every logged activity across all contacts, newest first, filtered by a
/// substring of the contact name/nickname or the note text.
pub fn list_all_activities(conn: &Connection, query: &str) -> Result<Vec<ActivityLogRow>> {
    let like = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT a.id, a.kind, a.note, a.created_at, c.id, c.name, c.nickname, c.contact_type
         FROM activities a
         JOIN contacts c ON c.id = a.contact_id
         WHERE c.name LIKE ?1 OR IFNULL(c.nickname, '') LIKE ?1 OR a.note LIKE ?1
         ORDER BY a.created_at DESC, a.id DESC",
    )?;
    let rows = stmt.query_map([like], |row| {
        let created: String = row.get(3)?;
        let name: String = row.get(5)?;
        let nickname: Option<String> = row.get(6)?;
        let ctype: String = row.get(7)?;
        let contact_name = match nickname {
            Some(n) if !n.is_empty() => format!("{name} ({n})"),
            _ => name,
        };
        Ok(ActivityLogRow {
            activity: Activity {
                id: row.get(0)?,
                kind: row.get(1)?,
                note: row.get(2)?,
                created_at: parse_dt(&created),
            },
            contact_id: row.get(4)?,
            contact_name,
            contact_type: ContactType::from_db(&ctype),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Activity kinds (user-managed types)
// ---------------------------------------------------------------------------

/// A user-managed activity type.
pub struct ActivityKindRow {
    pub id: i64,
    pub name: String,
}

/// Map a UNIQUE-constraint failure to a friendly message; pass other errors on.
fn dup_or(e: rusqlite::Error, msg: &str) -> AppError {
    if let rusqlite::Error::SqliteFailure(f, _) = &e {
        if f.code == rusqlite::ErrorCode::ConstraintViolation {
            return AppError::validation(msg);
        }
    }
    AppError::from(e)
}

/// All activity types, ordered by name.
pub fn list_activity_kinds(conn: &Connection) -> Result<Vec<ActivityKindRow>> {
    let mut stmt =
        conn.prepare("SELECT id, name FROM activity_kinds ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], |row| {
        Ok(ActivityKindRow {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Add a new activity type (names are unique and non-empty).
pub fn add_activity_kind(conn: &Connection, name: &str) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation("กรุณากรอกชื่อประเภทกิจกรรม"));
    }
    conn.execute("INSERT INTO activity_kinds (name) VALUES (?1)", params![name])
        .map_err(|e| dup_or(e, "มีประเภทกิจกรรมนี้อยู่แล้ว"))?;
    Ok(conn.last_insert_rowid())
}

/// Rename an activity type; existing activity rows using the old name are
/// relabelled too, so history stays consistent with the dropdown.
pub fn rename_activity_kind(conn: &Connection, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation("กรุณากรอกชื่อประเภทกิจกรรม"));
    }
    let old: String =
        conn.query_row("SELECT name FROM activity_kinds WHERE id = ?1", [id], |r| r.get(0))?;
    if old == name {
        return Ok(());
    }
    conn.execute("UPDATE activity_kinds SET name = ?1 WHERE id = ?2", params![name, id])
        .map_err(|e| dup_or(e, "มีประเภทกิจกรรมนี้อยู่แล้ว"))?;
    conn.execute("UPDATE activities SET kind = ?1 WHERE kind = ?2", params![name, old])?;
    Ok(())
}

/// Delete an activity type. Past activities keep their stored text.
pub fn delete_activity_kind(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM activity_kinds WHERE id = ?1", [id])?;
    Ok(())
}

/// How many logged activities currently use a kind name (for delete warnings).
pub fn activity_kind_usage(conn: &Connection, name: &str) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM activities WHERE kind = ?1", [name], |r| r.get(0))?)
}

// ---------------------------------------------------------------------------
// Prospect scores
// ---------------------------------------------------------------------------

fn row_to_prospect_score(row: &Row) -> rusqlite::Result<ProspectScore> {
    Ok(ProspectScore {
        contact_id: row.get(0)?,
        relationship_closeness: row.get::<_, i64>(1)? as u8,
        financial_stability: row.get::<_, i64>(2)? as u8,
        leadership: row.get::<_, i64>(3)? as u8,
        financial_status: row.get::<_, i64>(4)? as u8,
        accessibility: row.get::<_, i64>(5)? as u8,
        total: row.get::<_, i64>(6)? as u8,
    })
}

/// Insert or update a prospect score. Validates ranges and that the contact is
/// actually a Prospect; `total` is always recomputed server-side.
pub fn upsert_prospect_score(conn: &Connection, s: &ProspectScore) -> Result<()> {
    scoring::validate_prospect_fields(
        s.relationship_closeness,
        s.financial_stability,
        s.leadership,
        s.financial_status,
        s.accessibility,
    )?;
    if contact_type_of(conn, s.contact_id)? != ContactType::Prospect {
        return Err(AppError::validation(
            "prospect score can only be set on a Prospect",
        ));
    }
    let mut s = s.clone();
    s.recompute();
    conn.execute(
        "INSERT INTO prospect_scores
            (contact_id, relationship_closeness, financial_stability, leadership,
             financial_status, accessibility, total)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(contact_id) DO UPDATE SET
            relationship_closeness = ?2, financial_stability = ?3, leadership = ?4,
            financial_status = ?5, accessibility = ?6, total = ?7",
        params![
            s.contact_id,
            s.relationship_closeness,
            s.financial_stability,
            s.leadership,
            s.financial_status,
            s.accessibility,
            s.total,
        ],
    )?;
    Ok(())
}

pub fn get_prospect_score(conn: &Connection, contact_id: i64) -> Result<Option<ProspectScore>> {
    Ok(conn
        .query_row(
            "SELECT contact_id, relationship_closeness, financial_stability, leadership,
                    financial_status, accessibility, total
             FROM prospect_scores WHERE contact_id = ?1",
            [contact_id],
            row_to_prospect_score,
        )
        .optional()?)
}

// ---------------------------------------------------------------------------
// Customer scores
// ---------------------------------------------------------------------------

fn row_to_customer_score(row: &Row) -> rusqlite::Result<CustomerScore> {
    Ok(CustomerScore {
        contact_id: row.get(0)?,
        relationship_level: row.get::<_, i64>(1)? as u8,
        financial_status: row.get::<_, i64>(2)? as u8,
        decision_power: row.get::<_, i64>(3)? as u8,
        problems: row.get(4)?,
        total: row.get::<_, i64>(5)? as u8,
    })
}

pub fn upsert_customer_score(conn: &Connection, s: &CustomerScore) -> Result<()> {
    scoring::validate_customer_fields(s.relationship_level, s.financial_status, s.decision_power)?;
    if contact_type_of(conn, s.contact_id)? != ContactType::Customer {
        return Err(AppError::validation(
            "customer score can only be set on a Customer",
        ));
    }
    let mut s = s.clone();
    s.recompute();
    conn.execute(
        "INSERT INTO customer_scores
            (contact_id, relationship_level, financial_status, decision_power, problems, total)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(contact_id) DO UPDATE SET
            relationship_level = ?2, financial_status = ?3, decision_power = ?4,
            problems = ?5, total = ?6",
        params![
            s.contact_id,
            s.relationship_level,
            s.financial_status,
            s.decision_power,
            s.problems,
            s.total,
        ],
    )?;
    Ok(())
}

pub fn get_customer_score(conn: &Connection, contact_id: i64) -> Result<Option<CustomerScore>> {
    Ok(conn
        .query_row(
            "SELECT contact_id, relationship_level, financial_status, decision_power, problems, total
             FROM customer_scores WHERE contact_id = ?1",
            [contact_id],
            row_to_customer_score,
        )
        .optional()?)
}

// ---------------------------------------------------------------------------
// Sponsor flow
// ---------------------------------------------------------------------------

fn serialize_step_dates(m: &HashMap<SponsorStep, NaiveDate>) -> Result<String> {
    let mapped: HashMap<String, String> = m
        .iter()
        .map(|(k, v)| (k.as_int().to_string(), v.format("%Y-%m-%d").to_string()))
        .collect();
    Ok(serde_json::to_string(&mapped)?)
}

fn deserialize_step_dates(s: &str) -> HashMap<SponsorStep, NaiveDate> {
    let mapped: HashMap<String, String> = serde_json::from_str(s).unwrap_or_default();
    mapped
        .into_iter()
        .filter_map(|(k, v)| {
            let step = k.parse::<i64>().ok().map(SponsorStep::from_int)?;
            let date = NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok()?;
            Some((step, date))
        })
        .collect()
}

/// Load the flow for a prospect, or a fresh Step1 flow (not persisted) if none
/// exists yet.
pub fn get_sponsor_flow(conn: &Connection, contact_id: i64) -> Result<SponsorFlowStatus> {
    let row = conn
        .query_row(
            "SELECT current_step, step_date, notes FROM sponsor_flow_status WHERE contact_id = ?1",
            [contact_id],
            |r| {
                let step: i64 = r.get(0)?;
                let dates: String = r.get(1)?;
                let notes: String = r.get(2)?;
                Ok((step, dates, notes))
            },
        )
        .optional()?;

    match row {
        Some((step, dates, notes)) => Ok(SponsorFlowStatus {
            contact_id,
            current_step: SponsorStep::from_int(step),
            step_date: deserialize_step_dates(&dates),
            notes,
        }),
        None => Ok(SponsorFlowStatus::new(contact_id)),
    }
}

pub fn save_sponsor_flow(conn: &Connection, s: &SponsorFlowStatus) -> Result<()> {
    let dates = serialize_step_dates(&s.step_date)?;
    conn.execute(
        "INSERT INTO sponsor_flow_status (contact_id, current_step, step_date, notes)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(contact_id) DO UPDATE SET current_step = ?2, step_date = ?3, notes = ?4",
        params![s.contact_id, s.current_step.as_int(), dates, s.notes],
    )?;
    Ok(())
}

/// Move a prospect to `step`, enforcing sequential advancement (no skipping
/// ahead) and recording today's date for the new step.
pub fn set_sponsor_step(conn: &Connection, contact_id: i64, step: SponsorStep) -> Result<()> {
    let mut flow = get_sponsor_flow(conn, contact_id)?;
    scoring::validate_step_transition(flow.current_step, step)?;
    flow.current_step = step;
    flow.step_date.insert(step, Local::now().date_naive());
    save_sponsor_flow(conn, &flow)
}

/// Set a prospect's flow step to *any* value (manual correction from the UI).
/// Unlike [`set_sponsor_step`], this does not enforce sequential advancement, so
/// the user can jump to, or step back to, any point in the flow. Today's date is
/// still recorded for the chosen step.
pub fn set_sponsor_step_direct(
    conn: &Connection,
    contact_id: i64,
    step: SponsorStep,
) -> Result<()> {
    let mut flow = get_sponsor_flow(conn, contact_id)?;
    flow.current_step = step;
    flow.step_date.insert(step, Local::now().date_naive());
    save_sponsor_flow(conn, &flow)
}

// ---------------------------------------------------------------------------
// Follow-up sheet
// ---------------------------------------------------------------------------

/// Load the follow-up sheet for an ABO, or a blank sheet if none saved yet.
pub fn get_follow_up(conn: &Connection, contact_id: i64) -> Result<FollowUpSheet> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT contact_id FROM follow_up_sheets WHERE contact_id = ?1",
            [contact_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Ok(FollowUpSheet::new(contact_id));
    }

    let f = conn.query_row(
        "SELECT bk1_jumpstart1, bk1_core_plan, bk1_why_amway, bk1_why_nutrilite, bk1_closed,
                bk1_jumpstart2, bk1_why_artistry, bk1_smart_home_tech, bk1_aec_health,
                bk2_jumpstart3, bk2_space_to_grow, bk2_100_dreams, bk2_5f1f, bk2_name_list,
                bk2_study_table, bk2_analysis,
                c1_link3, c1_weekly_meeting, c1_ccs_seminar, c1_auto_renewal, c1_sop,
                c1_1abo, c1_5000pv,
                conf_crack_code, conf_5stars, conf_spirit, updated_at
         FROM follow_up_sheets WHERE contact_id = ?1",
        [contact_id],
        |r| {
            let b = |i: usize| -> rusqlite::Result<bool> { Ok(r.get::<_, i64>(i)? != 0) };
            let updated: String = r.get(26)?;
            Ok(FollowUpSheet {
                contact_id,
                bk1_jumpstart1: b(0)?,
                bk1_core_plan: b(1)?,
                bk1_why_amway: b(2)?,
                bk1_why_nutrilite: b(3)?,
                bk1_closed: b(4)?,
                bk1_jumpstart2: b(5)?,
                bk1_why_artistry: b(6)?,
                bk1_smart_home_tech: b(7)?,
                bk1_aec_health: b(8)?,
                bk2_jumpstart3: b(9)?,
                bk2_space_to_grow: b(10)?,
                bk2_100_dreams: b(11)?,
                bk2_5f1f: b(12)?,
                bk2_name_list: b(13)?,
                bk2_study_table: b(14)?,
                bk2_analysis: b(15)?,
                c1_link3: b(16)?,
                c1_weekly_meeting: b(17)?,
                c1_ccs_seminar: b(18)?,
                c1_auto_renewal: b(19)?,
                c1_sop: b(20)?,
                c1_1abo: b(21)?,
                c1_5000pv: b(22)?,
                conf_crack_code: b(23)?,
                conf_5stars: b(24)?,
                conf_spirit: b(25)?,
                updated_at: parse_dt(&updated),
            })
        },
    )?;
    Ok(f)
}

pub fn save_follow_up(conn: &Connection, f: &FollowUpSheet) -> Result<()> {
    conn.execute(
        "INSERT INTO follow_up_sheets (
            contact_id, bk1_jumpstart1, bk1_core_plan, bk1_why_amway, bk1_why_nutrilite,
            bk1_closed, bk1_jumpstart2, bk1_why_artistry, bk1_smart_home_tech, bk1_aec_health,
            bk2_jumpstart3, bk2_space_to_grow, bk2_100_dreams, bk2_5f1f, bk2_name_list,
            bk2_study_table, bk2_analysis, c1_link3, c1_weekly_meeting, c1_ccs_seminar,
            c1_auto_renewal, c1_sop, c1_1abo, c1_5000pv, conf_crack_code, conf_5stars,
            conf_spirit, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
         ON CONFLICT(contact_id) DO UPDATE SET
            bk1_jumpstart1=?2, bk1_core_plan=?3, bk1_why_amway=?4, bk1_why_nutrilite=?5,
            bk1_closed=?6, bk1_jumpstart2=?7, bk1_why_artistry=?8, bk1_smart_home_tech=?9,
            bk1_aec_health=?10, bk2_jumpstart3=?11, bk2_space_to_grow=?12, bk2_100_dreams=?13,
            bk2_5f1f=?14, bk2_name_list=?15, bk2_study_table=?16, bk2_analysis=?17,
            c1_link3=?18, c1_weekly_meeting=?19, c1_ccs_seminar=?20, c1_auto_renewal=?21,
            c1_sop=?22, c1_1abo=?23, c1_5000pv=?24, conf_crack_code=?25, conf_5stars=?26,
            conf_spirit=?27, updated_at=?28",
        params![
            f.contact_id,
            f.bk1_jumpstart1, f.bk1_core_plan, f.bk1_why_amway, f.bk1_why_nutrilite,
            f.bk1_closed, f.bk1_jumpstart2, f.bk1_why_artistry, f.bk1_smart_home_tech,
            f.bk1_aec_health, f.bk2_jumpstart3, f.bk2_space_to_grow, f.bk2_100_dreams,
            f.bk2_5f1f, f.bk2_name_list, f.bk2_study_table, f.bk2_analysis,
            f.c1_link3, f.c1_weekly_meeting, f.c1_ccs_seminar, f.c1_auto_renewal,
            f.c1_sop, f.c1_1abo, f.c1_5000pv, f.conf_crack_code, f.conf_5stars,
            f.conf_spirit, f.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Todos
// ---------------------------------------------------------------------------

/// A todo joined with its contact (name + type), for the list view.
pub struct TodoRow {
    pub todo: Todo,
    pub contact_name: Option<String>,
    pub contact_type: Option<ContactType>,
}

/// Add a task; returns the new id. `task` is trimmed and must be non-empty.
pub fn add_todo(
    conn: &Connection,
    contact_id: Option<i64>,
    task: &str,
    due_date: Option<NaiveDate>,
) -> Result<i64> {
    let task = task.trim();
    if task.is_empty() {
        return Err(AppError::validation("กรุณากรอกสิ่งที่ต้องทำ"));
    }
    let due = due_date.map(|d| d.format("%Y-%m-%d").to_string());
    conn.execute(
        "INSERT INTO todos (contact_id, task, due_date, done, created_at)
         VALUES (?1, ?2, ?3, 0, ?4)",
        params![contact_id, task, due, Local::now().to_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update a task's contact, text, and due date (not `done` / `created_at`).
pub fn update_todo(conn: &Connection, t: &Todo) -> Result<()> {
    let task = t.task.trim();
    if task.is_empty() {
        return Err(AppError::validation("กรุณากรอกสิ่งที่ต้องทำ"));
    }
    let due = t.due_date.map(|d| d.format("%Y-%m-%d").to_string());
    conn.execute(
        "UPDATE todos SET contact_id = ?1, task = ?2, due_date = ?3 WHERE id = ?4",
        params![t.contact_id, task, due, t.id],
    )?;
    Ok(())
}

/// Set a task's done flag.
pub fn set_todo_done(conn: &Connection, id: i64, done: bool) -> Result<()> {
    conn.execute("UPDATE todos SET done = ?1 WHERE id = ?2", params![done as i64, id])?;
    Ok(())
}

/// Delete a task.
pub fn delete_todo(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM todos WHERE id = ?1", [id])?;
    Ok(())
}

fn row_to_todo_row(row: &Row) -> rusqlite::Result<TodoRow> {
    let due: Option<String> = row.get(3)?;
    let created: String = row.get(5)?;
    let name: Option<String> = row.get(6)?;
    let nickname: Option<String> = row.get(7)?;
    let ctype: Option<String> = row.get(8)?;
    let contact_name = name.map(|n| match nickname {
        Some(nk) if !nk.is_empty() => format!("{n} ({nk})"),
        _ => n,
    });
    Ok(TodoRow {
        todo: Todo {
            id: row.get(0)?,
            contact_id: row.get(1)?,
            task: row.get(2)?,
            due_date: due.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            done: row.get::<_, i64>(4)? != 0,
            created_at: parse_dt(&created),
        },
        contact_name,
        contact_type: ctype.map(|s| ContactType::from_db(&s)),
    })
}

/// All todos, joined with their contact, filtered by a substring of the task
/// text or the contact name/nickname. Ordered: pending first, then soonest due
/// date (no-due-date last), newest as the tiebreak.
pub fn list_todos(conn: &Connection, query: &str) -> Result<Vec<TodoRow>> {
    let like = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT t.id, t.contact_id, t.task, t.due_date, t.done, t.created_at,
                c.name, c.nickname, c.contact_type
         FROM todos t
         LEFT JOIN contacts c ON c.id = t.contact_id
         WHERE t.task LIKE ?1 OR IFNULL(c.name,'') LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1
         ORDER BY t.done ASC, (t.due_date IS NULL) ASC, t.due_date ASC, t.id DESC",
    )?;
    let rows = stmt.query_map([like], row_to_todo_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Count unfinished todos whose due date is before today.
pub fn count_overdue_todos(conn: &Connection) -> Result<i64> {
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM todos WHERE done = 0 AND due_date IS NOT NULL AND due_date < ?1",
        [today],
        |r| r.get(0),
    )?)
}

/// Count unfinished todos due between today and today+`days` (inclusive).
///
/// Reserved for a planned "due soon" dashboard card; kept (and tested) so the
/// card can be added without re-deriving the query.
#[allow(dead_code)]
pub fn count_due_soon_todos(conn: &Connection, days: i64) -> Result<i64> {
    let today = Local::now().date_naive();
    let until = today + chrono::Duration::days(days);
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM todos
         WHERE done = 0 AND due_date IS NOT NULL AND due_date >= ?1 AND due_date <= ?2",
        params![
            today.format("%Y-%m-%d").to_string(),
            until.format("%Y-%m-%d").to_string()
        ],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Dashboard aggregates
// ---------------------------------------------------------------------------

pub fn count_by_type(conn: &Connection, ty: ContactType) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM contacts WHERE contact_type = ?1",
        [ty.as_str()],
        |r| r.get(0),
    )?)
}

/// Contacts that became a Customer or ABO in the current calendar month.
pub fn count_conversions_this_month(conn: &Connection) -> Result<i64> {
    let ym = Local::now().format("%Y-%m").to_string();
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM contacts
         WHERE contact_type IN ('Customer', 'ABO') AND substr(created_at, 1, 7) = ?1",
        [ym],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Joined list rows for the table views
// ---------------------------------------------------------------------------

/// A prospect plus its derived score total and current sponsor-flow step.
pub struct ProspectRow {
    pub contact: Contact,
    pub score_total: u8,
    pub current_step: SponsorStep,
}

pub fn list_prospect_rows(conn: &Connection, query: &str) -> Result<Vec<ProspectRow>> {
    let like = format!("%{query}%");
    let sql = format!(
        "SELECT {C}, COALESCE(ps.total, 0), COALESCE(sf.current_step, 1)
         FROM contacts c
         LEFT JOIN prospect_scores ps ON ps.contact_id = c.id
         LEFT JOIN sponsor_flow_status sf ON sf.contact_id = c.id
         WHERE c.contact_type = 'Prospect'
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
         ORDER BY COALESCE(ps.total, 0) DESC, c.name ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([like], |row| {
        let contact = row_to_contact(row)?;
        let total: i64 = row.get(17)?;
        let step: i64 = row.get(18)?;
        Ok(ProspectRow {
            contact,
            score_total: total as u8,
            current_step: SponsorStep::from_int(step),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// A customer plus its derived score total.
pub struct CustomerRow {
    pub contact: Contact,
    pub score_total: u8,
    /// Resolved name of the managing upline ABO, if any (None = mine directly).
    pub upline_name: Option<String>,
}

pub fn list_customer_rows(conn: &Connection, query: &str) -> Result<Vec<CustomerRow>> {
    let like = format!("%{query}%");
    let sql = format!(
        "SELECT {C}, COALESCE(cs.total, 0), up.name
         FROM contacts c
         LEFT JOIN customer_scores cs ON cs.contact_id = c.id
         LEFT JOIN contacts up ON up.id = c.sponsor_id
         WHERE c.contact_type = 'Customer'
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
         ORDER BY COALESCE(cs.total, 0) DESC, c.name ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([like], |row| {
        let contact = row_to_contact(row)?;
        let total: i64 = row.get(17)?;
        let upline_name: Option<String> = row.get(18)?;
        Ok(CustomerRow {
            contact,
            score_total: total as u8,
            upline_name,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// An ABO plus the resolved name of its upline (sponsor), if any, and the number
/// of completed follow-up items (0..=26) — feeds the "% การติดตาม" column.
pub struct AboRow {
    pub contact: Contact,
    pub upline_name: Option<String>,
    pub followup_done: i64,
}

pub fn list_abo_rows(conn: &Connection, query: &str) -> Result<Vec<AboRow>> {
    let like = format!("%{query}%");
    // Sum the 26 follow-up booleans (NULL → 0 when the ABO has no sheet yet).
    let sql = format!(
        "SELECT {C}, up.name,
                COALESCE(fs.bk1_jumpstart1 + fs.bk1_core_plan + fs.bk1_why_amway
                       + fs.bk1_why_nutrilite + fs.bk1_closed + fs.bk1_jumpstart2
                       + fs.bk1_why_artistry + fs.bk1_smart_home_tech + fs.bk1_aec_health
                       + fs.bk2_jumpstart3 + fs.bk2_space_to_grow + fs.bk2_100_dreams
                       + fs.bk2_5f1f + fs.bk2_name_list + fs.bk2_study_table + fs.bk2_analysis
                       + fs.c1_link3 + fs.c1_weekly_meeting + fs.c1_ccs_seminar
                       + fs.c1_auto_renewal + fs.c1_sop + fs.c1_1abo + fs.c1_5000pv
                       + fs.conf_crack_code + fs.conf_5stars + fs.conf_spirit, 0)
         FROM contacts c
         LEFT JOIN contacts up ON up.id = c.sponsor_id
         LEFT JOIN follow_up_sheets fs ON fs.contact_id = c.id
         WHERE c.contact_type = 'ABO'
           AND (c.name LIKE ?1 OR IFNULL(c.nickname,'') LIKE ?1 OR IFNULL(c.phone,'') LIKE ?1)
         ORDER BY c.name ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([like], |row| {
        let contact = row_to_contact(row)?;
        let upline_name: Option<String> = row.get(17)?;
        let followup_done: i64 = row.get(18)?;
        Ok(AboRow {
            contact,
            upline_name,
            followup_done,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    /// Build an in-memory database with foreign keys on and the schema applied.
    fn mem() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn sample_prospect(name: &str) -> Contact {
        let mut c = Contact::new_blank();
        c.name = name.to_string();
        c.phone = Some("0812345678".to_string());
        c.contact_type = ContactType::Prospect;
        c
    }

    fn sample_abo(name: &str, rank: Rank) -> Contact {
        let mut c = Contact::new_blank();
        c.name = name.to_string();
        c.contact_type = ContactType::Abo;
        c.rank = Some(rank);
        c
    }

    #[test]
    fn insert_then_read_back_matches() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("สมชาย")).unwrap();
        let back = get_contact(&conn, id).unwrap();
        assert_eq!(back.name, "สมชาย");
        assert_eq!(back.phone.as_deref(), Some("0812345678"));
        assert_eq!(back.contact_type, ContactType::Prospect);
    }

    #[test]
    fn update_sponsor_step_persists() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Bee")).unwrap();
        set_sponsor_step(&conn, id, SponsorStep::Step2).unwrap();
        let flow = get_sponsor_flow(&conn, id).unwrap();
        assert_eq!(flow.current_step, SponsorStep::Step2);
        assert!(flow.step_date.contains_key(&SponsorStep::Step2));
    }

    #[test]
    fn follow_up_checkbox_toggle_persists() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_abo("Up", Rank::C1)).unwrap();
        let mut sheet = get_follow_up(&conn, id).unwrap();
        sheet.bk1_why_amway = true;
        sheet.c1_5000pv = true;
        save_follow_up(&conn, &sheet).unwrap();

        let reloaded = get_follow_up(&conn, id).unwrap();
        assert!(reloaded.bk1_why_amway);
        assert!(reloaded.c1_5000pv);
        assert_eq!(reloaded.done_count(), 2);
        assert!(!reloaded.bk2_5f1f);
    }

    #[test]
    fn delete_cascades_to_scores_and_follow_up() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Cascade")).unwrap();
        let mut score = ProspectScore::new(id);
        score.relationship_closeness = 8;
        upsert_prospect_score(&conn, &score).unwrap();
        // Give it a flow row too.
        set_sponsor_step(&conn, id, SponsorStep::Step2).unwrap();

        delete_contact(&conn, id).unwrap();

        let scores: i64 = conn
            .query_row("SELECT COUNT(*) FROM prospect_scores WHERE contact_id = ?1", [id], |r| r.get(0))
            .unwrap();
        let flows: i64 = conn
            .query_row("SELECT COUNT(*) FROM sponsor_flow_status WHERE contact_id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(scores, 0);
        assert_eq!(flows, 0);
    }

    #[test]
    fn sponsor_must_reference_an_abo() {
        let conn = mem();
        let prospect_id = insert_contact(&conn, &sample_prospect("NotAbo")).unwrap();
        let abo_id = insert_contact(&conn, &sample_abo("RealAbo", Rank::Cl)).unwrap();

        // Pointing at a prospect is rejected.
        let mut bad = sample_abo("Child", Rank::Koc);
        bad.sponsor_id = Some(prospect_id);
        assert!(insert_contact(&conn, &bad).is_err());

        // Pointing at an ABO is accepted.
        let mut good = sample_abo("Child", Rank::Koc);
        good.sponsor_id = Some(abo_id);
        assert!(insert_contact(&conn, &good).is_ok());

        // A non-existent sponsor is rejected.
        let mut ghost = sample_abo("Ghost", Rank::Koc);
        ghost.sponsor_id = Some(99_999);
        assert!(insert_contact(&conn, &ghost).is_err());
    }

    #[test]
    fn abo_rows_resolve_upline_name_and_filter_by_type() {
        let conn = mem();
        let upline = insert_contact(&conn, &sample_abo("พิชัย", Rank::Cl21)).unwrap();
        let mut child = sample_abo("วีระ", Rank::C1);
        child.sponsor_id = Some(upline);
        insert_contact(&conn, &child).unwrap();
        // A prospect must NOT appear in the ABO list.
        insert_contact(&conn, &sample_prospect("ผู้มุ่งหวัง")).unwrap();

        let rows = list_abo_rows(&conn, "").unwrap();
        assert_eq!(rows.len(), 2, "only ABOs are listed");

        let child_row = rows.iter().find(|r| r.contact.name == "วีระ").unwrap();
        assert_eq!(child_row.upline_name.as_deref(), Some("พิชัย"));

        let root_row = rows.iter().find(|r| r.contact.name == "พิชัย").unwrap();
        assert_eq!(root_row.upline_name, None);

        // Search narrows the list.
        let filtered = list_abo_rows(&conn, "วีระ").unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn abo_rows_include_followup_done() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_abo("Up", Rank::Cl)).unwrap();
        // No follow-up sheet yet → 0.
        assert_eq!(list_abo_rows(&conn, "").unwrap()[0].followup_done, 0);

        // Tick three items and save → the count is 3.
        let mut sheet = get_follow_up(&conn, id).unwrap();
        sheet.bk1_why_amway = true;
        sheet.c1_sop = true;
        sheet.conf_spirit = true;
        save_follow_up(&conn, &sheet).unwrap();
        assert_eq!(list_abo_rows(&conn, "").unwrap()[0].followup_done, 3);
    }

    #[test]
    fn abo_leg_counts_and_ppv_round_trip() {
        let conn = mem();
        let up = insert_contact(&conn, &sample_abo("Up", Rank::Cl21)).unwrap();
        // Three direct downlines: two CL, one C1.
        for (n, r) in [("a", Rank::Cl), ("b", Rank::Cl), ("c", Rank::C1)] {
            let mut child = sample_abo(n, r);
            child.sponsor_id = Some(up);
            insert_contact(&conn, &child).unwrap();
        }
        let (c1, cl, cl15) = abo_leg_counts(&conn, up).unwrap();
        assert_eq!(c1, 3); // all three are C1 or above
        assert_eq!(cl, 2); // two are CL or above
        assert_eq!(cl15, 0);

        // PPV persists.
        update_ppv(&conn, up, 12_345).unwrap();
        assert_eq!(get_contact(&conn, up).unwrap().ppv, 12_345);
    }

    #[test]
    fn me_leg_counts_and_ppv_round_trip() {
        let conn = mem();
        // Three ABOs directly under me (no sponsor): two CL, one C1.
        for (n, r) in [("a", Rank::Cl), ("b", Rank::Cl), ("c", Rank::C1)] {
            insert_contact(&conn, &sample_abo(n, r)).unwrap();
        }
        // A deeper ABO (sponsored by one of mine) must NOT count as my own leg.
        let parent = list_abos(&conn).unwrap()[0].id;
        let mut deep = sample_abo("deep", Rank::Cl21);
        deep.sponsor_id = Some(parent);
        insert_contact(&conn, &deep).unwrap();

        let (c1, cl, cl15) = me_leg_counts(&conn).unwrap();
        assert_eq!(c1, 3); // a, b, c are all C1 or above
        assert_eq!(cl, 2); // a, b
        assert_eq!(cl15, 0); // the only CL15+ (deep) is not a direct leg of mine

        // My PPV defaults to 0, then round-trips (upsert overwrites).
        assert_eq!(get_me_ppv(&conn).unwrap(), 0);
        set_me_ppv(&conn, 22_000).unwrap();
        assert_eq!(get_me_ppv(&conn).unwrap(), 22_000);
        set_me_ppv(&conn, 31_000).unwrap();
        assert_eq!(get_me_ppv(&conn).unwrap(), 31_000);
    }

    #[test]
    fn activities_add_list_delete_and_cascade() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Act")).unwrap();
        let a1 = add_activity(&conn, id, "สาธิตสินค้า", "สาธิต Nutrilite").unwrap();
        add_activity(&conn, id, "บอกโปรโมชั่น", "").unwrap();

        let list = list_activities(&conn, id).unwrap();
        assert_eq!(list.len(), 2);
        // Newest first; the id DESC tiebreaker keeps the later insert on top.
        assert_eq!(list[0].kind, "บอกโปรโมชั่น");
        assert_eq!(list[1].kind, "สาธิตสินค้า");
        assert_eq!(list[1].note, "สาธิต Nutrilite");

        delete_activity(&conn, a1).unwrap();
        assert_eq!(list_activities(&conn, id).unwrap().len(), 1);

        // Deleting the contact cascades its activities.
        delete_contact(&conn, id).unwrap();
        assert_eq!(list_activities(&conn, id).unwrap().len(), 0);
    }

    #[test]
    fn list_all_activities_joins_contacts_and_filters() {
        let conn = mem();
        let p = insert_contact(&conn, &sample_prospect("ธนา")).unwrap();
        let mut cust = sample_prospect("มานี");
        cust.contact_type = ContactType::Customer;
        let c = insert_contact(&conn, &cust).unwrap();
        add_activity(&conn, p, "สาธิตสินค้า", "สาธิตสินค้า").unwrap();
        add_activity(&conn, c, "บอกโปรโมชั่น", "โปร 11.11").unwrap();

        // All rows, newest first (id DESC tiebreaker → the promotion on top).
        let all = list_all_activities(&conn, "").unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].activity.kind, "บอกโปรโมชั่น");
        assert_eq!(all[0].contact_type, ContactType::Customer);
        assert_eq!(all[0].contact_name, "มานี");

        // Filter by contact name.
        let by_name = list_all_activities(&conn, "ธนา").unwrap();
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].contact_id, p);

        // Filter by note text.
        assert_eq!(list_all_activities(&conn, "11.11").unwrap().len(), 1);
    }

    #[test]
    fn activity_kinds_crud_and_rename_relabels_activities() {
        let conn = mem();
        // Defaults are seeded by the v5 migration.
        let initial = list_activity_kinds(&conn).unwrap();
        assert!(initial.iter().any(|k| k.name == "สาธิตสินค้า"));

        // Add a kind; blanks and duplicates are rejected.
        let id = add_activity_kind(&conn, "ส่งของ").unwrap();
        assert!(add_activity_kind(&conn, "ส่งของ").is_err());
        assert!(add_activity_kind(&conn, "   ").is_err());

        // Log an activity with that kind, then rename → the activity follows.
        let cid = insert_contact(&conn, &sample_prospect("ก")).unwrap();
        add_activity(&conn, cid, "ส่งของ", "").unwrap();
        rename_activity_kind(&conn, id, "จัดส่ง").unwrap();
        assert_eq!(activity_kind_usage(&conn, "ส่งของ").unwrap(), 0);
        assert_eq!(activity_kind_usage(&conn, "จัดส่ง").unwrap(), 1);

        // Delete the kind; the past activity keeps its (renamed) text.
        delete_activity_kind(&conn, id).unwrap();
        assert!(!list_activity_kinds(&conn).unwrap().iter().any(|k| k.id == id));
        assert_eq!(list_activities(&conn, cid).unwrap()[0].kind, "จัดส่ง");
    }

    #[test]
    fn customer_rows_resolve_upline_name() {
        let conn = mem();
        let up = insert_contact(&conn, &sample_abo("Mentor", Rank::Cl)).unwrap();
        // A customer managed by a downline ABO, and one managed by me directly.
        let mut managed = sample_prospect("ลูกค้า A");
        managed.contact_type = ContactType::Customer;
        managed.sponsor_id = Some(up);
        insert_contact(&conn, &managed).unwrap();
        let mut mine = sample_prospect("ลูกค้า B");
        mine.contact_type = ContactType::Customer;
        insert_contact(&conn, &mine).unwrap();

        let rows = list_customer_rows(&conn, "").unwrap();
        assert_eq!(rows.len(), 2);
        let a = rows.iter().find(|r| r.contact.name == "ลูกค้า A").unwrap();
        let b = rows.iter().find(|r| r.contact.name == "ลูกค้า B").unwrap();
        assert_eq!(a.upline_name.as_deref(), Some("Mentor"));
        assert_eq!(b.upline_name, None);
    }

    #[test]
    fn member_abo_numbers_round_trip() {
        let conn = mem();
        let mut c = sample_abo("Biz", Rank::C1);
        c.member_no = Some("M-001".to_string());
        c.abo_no = Some("ABO-999".to_string());
        let id = insert_contact(&conn, &c).unwrap();

        let got = get_contact(&conn, id).unwrap();
        assert_eq!(got.member_no.as_deref(), Some("M-001"));
        assert_eq!(got.abo_no.as_deref(), Some("ABO-999"));

        // Update clears one and changes the other.
        let mut upd = got.clone();
        upd.member_no = None;
        upd.abo_no = Some("ABO-1000".to_string());
        update_contact(&conn, &upd).unwrap();
        let got2 = get_contact(&conn, id).unwrap();
        assert_eq!(got2.member_no, None);
        assert_eq!(got2.abo_no.as_deref(), Some("ABO-1000"));
    }

    #[test]
    fn prospect_score_out_of_range_is_rejected() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Range")).unwrap();
        let mut score = ProspectScore::new(id);
        score.relationship_closeness = 11; // > 10
        assert!(upsert_prospect_score(&conn, &score).is_err());
    }

    #[test]
    fn sponsor_step_cannot_skip() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Skip")).unwrap();
        // current is Step1; jumping straight to Step5 must fail.
        assert!(set_sponsor_step(&conn, id, SponsorStep::Step5).is_err());
    }

    #[test]
    fn sponsor_step_direct_allows_jumps_for_manual_edit() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Edit")).unwrap();
        // Manual edit may jump forward past several steps...
        set_sponsor_step_direct(&conn, id, SponsorStep::Step6).unwrap();
        assert_eq!(get_sponsor_flow(&conn, id).unwrap().current_step, SponsorStep::Step6);
        // ...and step back down.
        set_sponsor_step_direct(&conn, id, SponsorStep::Step2).unwrap();
        let flow = get_sponsor_flow(&conn, id).unwrap();
        assert_eq!(flow.current_step, SponsorStep::Step2);
        assert!(flow.step_date.contains_key(&SponsorStep::Step6));
    }

    #[test]
    fn rank_cannot_regress_on_update() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_abo("Senior", Rank::Cl)).unwrap();
        let mut c = get_contact(&conn, id).unwrap();
        c.rank = Some(Rank::C1); // regress CL -> C1
        assert!(update_contact(&conn, &c).is_err());
        // Advancing is fine.
        let mut c2 = get_contact(&conn, id).unwrap();
        c2.rank = Some(Rank::Cl21);
        assert!(update_contact(&conn, &c2).is_ok());
    }

    #[test]
    fn changing_type_drops_opposing_score() {
        let conn = mem();
        let id = insert_contact(&conn, &sample_prospect("Switch")).unwrap();
        let score = ProspectScore::new(id);
        upsert_prospect_score(&conn, &score).unwrap();
        assert!(get_prospect_score(&conn, id).unwrap().is_some());

        // Convert to Customer; the prospect score must be cleared.
        let mut c = get_contact(&conn, id).unwrap();
        c.contact_type = ContactType::Customer;
        update_contact(&conn, &c).unwrap();
        assert!(get_prospect_score(&conn, id).unwrap().is_none());
    }

    #[test]
    fn todo_add_list_update_delete() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("ตูน")).unwrap();
        let id = add_todo(&conn, Some(cid), "  โทรนัด  ", Some(d("2026-06-10"))).unwrap();
        // Blank task is rejected.
        assert!(add_todo(&conn, None, "   ", None).is_err());

        let rows = list_todos(&conn, "").unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.todo.task, "โทรนัด"); // trimmed
        assert_eq!(r.todo.due_date, Some(d("2026-06-10")));
        assert!(!r.todo.done);
        assert_eq!(r.contact_name.as_deref(), Some("ตูน"));
        assert_eq!(r.contact_type, Some(ContactType::Prospect));

        // Update: change task, clear due date, unassign contact.
        let mut t = r.todo.clone();
        t.task = "โทรนัดดูสินค้า".into();
        t.due_date = None;
        t.contact_id = None;
        update_todo(&conn, &t).unwrap();
        let rows = list_todos(&conn, "").unwrap();
        assert_eq!(rows[0].todo.task, "โทรนัดดูสินค้า");
        assert_eq!(rows[0].todo.due_date, None);
        assert_eq!(rows[0].contact_name, None);
        assert_eq!(rows[0].contact_type, None);

        delete_todo(&conn, id).unwrap();
        assert!(list_todos(&conn, "").unwrap().is_empty());
    }

    #[test]
    fn todo_list_orders_pending_then_due_date() {
        let conn = mem();
        let done_id = add_todo(&conn, None, "done task", Some(d("2026-01-01"))).unwrap();
        set_todo_done(&conn, done_id, true).unwrap();
        add_todo(&conn, None, "no due", None).unwrap();
        add_todo(&conn, None, "later", Some(d("2026-12-31"))).unwrap();
        add_todo(&conn, None, "soon", Some(d("2026-02-01"))).unwrap();

        let tasks: Vec<String> =
            list_todos(&conn, "").unwrap().into_iter().map(|r| r.todo.task).collect();
        // Pending first by due date asc (soon, later), then no-due-date, then done last.
        assert_eq!(tasks, vec!["soon", "later", "no due", "done task"]);
    }

    #[test]
    fn todo_done_toggle_persists() {
        let conn = mem();
        let id = add_todo(&conn, None, "t", None).unwrap();
        set_todo_done(&conn, id, true).unwrap();
        assert!(list_todos(&conn, "").unwrap()[0].todo.done);
        set_todo_done(&conn, id, false).unwrap();
        assert!(!list_todos(&conn, "").unwrap()[0].todo.done);
    }

    #[test]
    fn todo_contact_set_null_on_delete() {
        let conn = mem();
        let cid = insert_contact(&conn, &sample_prospect("เอ")).unwrap();
        add_todo(&conn, Some(cid), "task for เอ", None).unwrap();
        delete_contact(&conn, cid).unwrap();
        let rows = list_todos(&conn, "").unwrap();
        assert_eq!(rows.len(), 1, "todo survives contact deletion");
        assert_eq!(rows[0].todo.contact_id, None);
        assert_eq!(rows[0].contact_name, None);
        assert_eq!(rows[0].contact_type, None);
    }

    #[test]
    fn overdue_and_due_soon_counts() {
        let conn = mem();
        let today = Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let in_three = today + chrono::Duration::days(3);
        let in_ten = today + chrono::Duration::days(10);

        add_todo(&conn, None, "overdue", Some(yesterday)).unwrap();
        add_todo(&conn, None, "due today", Some(today)).unwrap();
        add_todo(&conn, None, "due soon", Some(in_three)).unwrap();
        add_todo(&conn, None, "far", Some(in_ten)).unwrap();
        let done_overdue = add_todo(&conn, None, "done overdue", Some(yesterday)).unwrap();
        set_todo_done(&conn, done_overdue, true).unwrap();

        assert_eq!(count_overdue_todos(&conn).unwrap(), 1); // only unfinished past-due
        // Inclusive on both ends: "due today" and in_three count; in_ten is beyond 7.
        assert_eq!(count_due_soon_todos(&conn, 7).unwrap(), 2);
    }
}
