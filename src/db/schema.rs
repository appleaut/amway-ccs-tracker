//! Database schema and forward migrations.
//!
//! Migrations are versioned through SQLite's `PRAGMA user_version`. Bumping the
//! schema means adding a new block guarded by the current version.

use rusqlite::Connection;

use crate::error::Result;

/// Current schema version understood by this build.
const CURRENT_VERSION: i64 = 3;

/// Initial schema. Foreign keys cascade scores / follow-up rows when a contact
/// is deleted, but a deleted sponsor only nulls its downline's `sponsor_id`
/// (the downline records are preserved).
const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS contacts (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT    NOT NULL,
    nickname         TEXT,
    phone            TEXT,
    line_id          TEXT,
    age              INTEGER,
    gender           TEXT    NOT NULL DEFAULT 'Male',
    address          TEXT,
    network_category TEXT    NOT NULL DEFAULT 'Friend',
    contact_type     TEXT    NOT NULL DEFAULT 'Prospect',
    rank             TEXT,
    sponsor_id       INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
    created_at       TEXT    NOT NULL,
    notes            TEXT
);

CREATE INDEX IF NOT EXISTS idx_contacts_type    ON contacts(contact_type);
CREATE INDEX IF NOT EXISTS idx_contacts_sponsor ON contacts(sponsor_id);

CREATE TABLE IF NOT EXISTS prospect_scores (
    contact_id            INTEGER PRIMARY KEY REFERENCES contacts(id) ON DELETE CASCADE,
    relationship_closeness INTEGER NOT NULL,
    financial_stability    INTEGER NOT NULL,
    leadership             INTEGER NOT NULL,
    financial_status       INTEGER NOT NULL,
    accessibility          INTEGER NOT NULL,
    total                  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS customer_scores (
    contact_id        INTEGER PRIMARY KEY REFERENCES contacts(id) ON DELETE CASCADE,
    relationship_level INTEGER NOT NULL,
    financial_status   INTEGER NOT NULL,
    decision_power     INTEGER NOT NULL,
    problems           TEXT    NOT NULL DEFAULT '',
    total              INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sponsor_flow_status (
    contact_id   INTEGER PRIMARY KEY REFERENCES contacts(id) ON DELETE CASCADE,
    current_step INTEGER NOT NULL DEFAULT 1,
    step_date    TEXT    NOT NULL DEFAULT '{}',
    notes        TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS follow_up_sheets (
    contact_id          INTEGER PRIMARY KEY REFERENCES contacts(id) ON DELETE CASCADE,
    bk1_jumpstart1      INTEGER NOT NULL DEFAULT 0,
    bk1_core_plan       INTEGER NOT NULL DEFAULT 0,
    bk1_why_amway       INTEGER NOT NULL DEFAULT 0,
    bk1_why_nutrilite   INTEGER NOT NULL DEFAULT 0,
    bk1_closed          INTEGER NOT NULL DEFAULT 0,
    bk1_jumpstart2      INTEGER NOT NULL DEFAULT 0,
    bk1_why_artistry    INTEGER NOT NULL DEFAULT 0,
    bk1_smart_home_tech INTEGER NOT NULL DEFAULT 0,
    bk1_aec_health      INTEGER NOT NULL DEFAULT 0,
    bk2_jumpstart3      INTEGER NOT NULL DEFAULT 0,
    bk2_space_to_grow   INTEGER NOT NULL DEFAULT 0,
    bk2_100_dreams      INTEGER NOT NULL DEFAULT 0,
    bk2_5f1f            INTEGER NOT NULL DEFAULT 0,
    bk2_name_list       INTEGER NOT NULL DEFAULT 0,
    bk2_study_table     INTEGER NOT NULL DEFAULT 0,
    bk2_analysis        INTEGER NOT NULL DEFAULT 0,
    c1_link3            INTEGER NOT NULL DEFAULT 0,
    c1_weekly_meeting   INTEGER NOT NULL DEFAULT 0,
    c1_ccs_seminar      INTEGER NOT NULL DEFAULT 0,
    c1_auto_renewal     INTEGER NOT NULL DEFAULT 0,
    c1_sop              INTEGER NOT NULL DEFAULT 0,
    c1_1abo             INTEGER NOT NULL DEFAULT 0,
    c1_5000pv           INTEGER NOT NULL DEFAULT 0,
    conf_crack_code     INTEGER NOT NULL DEFAULT 0,
    conf_5stars         INTEGER NOT NULL DEFAULT 0,
    conf_spirit         INTEGER NOT NULL DEFAULT 0,
    updated_at          TEXT    NOT NULL
);
"#;

/// Apply all pending migrations to bring `conn` up to [`CURRENT_VERSION`].
pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
    }

    if version < 2 {
        // Personal Point Value, used for ABO rank qualification.
        conn.execute_batch("ALTER TABLE contacts ADD COLUMN ppv INTEGER NOT NULL DEFAULT 0;")?;
    }

    if version < 3 {
        // Activity history: a log of interactions with each contact.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS activities (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
                kind       TEXT    NOT NULL,
                note       TEXT    NOT NULL DEFAULT '',
                created_at TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_activities_contact ON activities(contact_id);",
        )?;
    }

    if version != CURRENT_VERSION {
        // PRAGMA does not accept bound parameters, so format the constant in.
        conn.execute_batch(&format!("PRAGMA user_version = {CURRENT_VERSION};"))?;
    }
    Ok(())
}
