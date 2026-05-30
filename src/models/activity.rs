//! A logged interaction with a contact (activity history).

use chrono::{DateTime, Local};

use super::enums::ActivityKind;

/// One entry in a contact's activity history — what was done, an optional
/// free-text detail, and when it was logged.
#[derive(Debug, Clone)]
pub struct Activity {
    pub id: i64,
    pub kind: ActivityKind,
    pub note: String,
    pub created_at: DateTime<Local>,
}
