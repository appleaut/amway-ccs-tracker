//! A logged interaction with a contact (activity history).

use chrono::{DateTime, Local};

/// One entry in a contact's activity history — what was done, an optional
/// free-text detail, and when it was logged. `kind` is the activity-type name
/// (managed in the `activity_kinds` table) stored as text so history survives
/// renaming or deleting a type.
#[derive(Debug, Clone)]
pub struct Activity {
    pub id: i64,
    pub kind: String,
    pub note: String,
    pub created_at: DateTime<Local>,
}
