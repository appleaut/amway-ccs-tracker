//! Database schema and forward migrations.
//!
//! Migrations are versioned through SQLite's `PRAGMA user_version`. Bumping the
//! schema means adding a new block guarded by the current version.

use rusqlite::Connection;

use crate::error::Result;

/// Current schema version understood by this build.
const CURRENT_VERSION: i64 = 7;

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

    if version < 4 {
        // Key/value store for app-level settings. "Me" is the implicit network
        // root and has no contact row, so my own PPV (for self rank assessment)
        // lives here under the key 'me_ppv'.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
    }

    if version < 5 {
        // Activity types become user-manageable data instead of a fixed enum.
        // Seed the former built-in kinds (as their Thai labels) and migrate
        // existing activity rows from the old enum keys to those labels.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS activity_kinds (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT    NOT NULL UNIQUE
            );
            INSERT OR IGNORE INTO activity_kinds (name) VALUES
                ('สาธิตสินค้า'), ('บอกโปรโมชั่น'), ('พูดแผนธุรกิจ'),
                ('ติดตามผล'), ('นัดพบ / พูดคุย'), ('อื่นๆ');
            UPDATE activities SET kind = 'สาธิตสินค้า'    WHERE kind = 'Demo';
            UPDATE activities SET kind = 'บอกโปรโมชั่น'   WHERE kind = 'Promotion';
            UPDATE activities SET kind = 'พูดแผนธุรกิจ'   WHERE kind = 'Plan';
            UPDATE activities SET kind = 'ติดตามผล'       WHERE kind = 'FollowUp';
            UPDATE activities SET kind = 'นัดพบ / พูดคุย'  WHERE kind = 'Meeting';
            UPDATE activities SET kind = 'อื่นๆ'           WHERE kind = 'Other';",
        )?;
    }

    if version < 6 {
        // Optional Amway member / ABO numbers, entered for Customers and ABOs.
        conn.execute_batch(
            "ALTER TABLE contacts ADD COLUMN member_no TEXT;
             ALTER TABLE contacts ADD COLUMN abo_no TEXT;",
        )?;
    }

    if version < 7 {
        // Todo List: tasks to do, optionally tied to a contact. Deleting a
        // contact nulls the link (the task is preserved) rather than cascading.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS todos (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                contact_id INTEGER REFERENCES contacts(id) ON DELETE SET NULL,
                task       TEXT    NOT NULL,
                due_date   TEXT,
                done       INTEGER NOT NULL DEFAULT 0,
                created_at TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_todos_contact ON todos(contact_id);
            CREATE INDEX IF NOT EXISTS idx_todos_due     ON todos(due_date);",
        )?;
    }

    if version != CURRENT_VERSION {
        // PRAGMA does not accept bound parameters, so format the constant in.
        conn.execute_batch(&format!("PRAGMA user_version = {CURRENT_VERSION};"))?;
    }
    Ok(())
}
