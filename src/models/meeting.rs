//! A meeting/event and a contact's attendance of it.
//!
//! [`Meeting`] is the event (name, dates, description, entry fee).
//! [`MeetingAttendee`] is one cell of the attendance matrix: a contact's RSVP
//! status for a meeting, whether they paid the entry fee, and their actual
//! post-event attendance (`None` = not recorded yet).

use chrono::{DateTime, Local, NaiveDate};

use crate::models::enums::AttendeeStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
    pub id: i64,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub description: String,
    /// Entry fee in baht (0 = free).
    pub fee: i64,
    pub created_at: DateTime<Local>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingAttendee {
    pub meeting_id: i64,
    pub contact_id: i64,
    pub status: AttendeeStatus,
    pub paid: bool,
    /// `None` = not recorded, `Some(true)` = came, `Some(false)` = no-show.
    pub attended: Option<bool>,
}
