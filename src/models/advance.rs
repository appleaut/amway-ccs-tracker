//! An advance payment: money fronted to buy products for a contact, to be
//! collected back later. `collected` is the only status; the collection date
//! and an optional note are recorded when it is collected.

use chrono::{DateTime, Local, NaiveDate};

/// One advance-payment record. `contact_id` is optional at the storage layer
/// (`ON DELETE SET NULL` preserves the money record if the contact is deleted),
/// though the UI requires a contact when creating one. `note` is an optional
/// remark entered at creation; `collected_note` is entered when collecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advance {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub item: String,
    pub amount: i64,
    pub advance_date: NaiveDate,
    pub note: String,
    pub collected: bool,
    pub collected_at: Option<NaiveDate>,
    pub collected_note: Option<String>,
    pub created_at: DateTime<Local>,
}
