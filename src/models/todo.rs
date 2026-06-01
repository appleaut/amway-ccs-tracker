//! A to-do task, optionally tied to a contact, with a due date and done flag.

use chrono::{DateTime, Local, NaiveDate};

/// One task on the Todo List. `contact_id` is optional (a task may target no one,
/// and survives — as unassigned — if its contact is deleted). `due_date` is
/// optional ("ไม่มีกำหนด"). `done` is the only status; "overdue" is derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub task: String,
    pub due_date: Option<NaiveDate>,
    pub done: bool,
    pub created_at: DateTime<Local>,
}

impl Todo {
    /// True when the task is unfinished and its due date is strictly before `today`.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.done && self.due_date.map_or(false, |d| d < today)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn todo(due: Option<NaiveDate>, done: bool) -> Todo {
        Todo {
            id: 1,
            contact_id: None,
            task: "x".into(),
            due_date: due,
            done,
            created_at: Local::now(),
        }
    }

    #[test]
    fn overdue_only_when_unfinished_and_past_due() {
        let today = d("2026-06-01");
        assert!(todo(Some(d("2026-05-31")), false).is_overdue(today)); // past + pending
        assert!(!todo(Some(d("2026-05-31")), true).is_overdue(today)); // past but done
        assert!(!todo(Some(d("2026-06-01")), false).is_overdue(today)); // due today (not past)
        assert!(!todo(Some(d("2026-06-02")), false).is_overdue(today)); // future
        assert!(!todo(None, false).is_overdue(today)); // no due date
    }
}
