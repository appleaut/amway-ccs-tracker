//! A recurring task schedule: a template (task + optional contact) plus a
//! cadence. When a cycle date arrives or has passed, the app materializes a
//! normal `Todo` from it (see `db::queries::generate_due_todos`). The occurrence
//! math here is pure (no DB, no clock) so it can be unit-tested in isolation.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate};

/// How often a schedule fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    /// Every `n` days (n >= 1), phased from the schedule's `start_date`.
    EveryNDays(u32),
    /// On a fixed day-of-month (1..=31), clamped to the month's last day when
    /// that day does not exist (e.g. day 31 in February → 28/29).
    MonthlyDay(u32),
}

impl Recurrence {
    /// The DB discriminator string for `freq_kind`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Recurrence::EveryNDays(_) => "EveryNDays",
            Recurrence::MonthlyDay(_) => "MonthlyDay",
        }
    }

    /// The DB integer for `freq_value`.
    pub fn value(&self) -> i64 {
        match self {
            Recurrence::EveryNDays(n) => *n as i64,
            Recurrence::MonthlyDay(d) => *d as i64,
        }
    }

    /// Rebuild from the stored (`freq_kind`, `freq_value`) pair. Returns `None`
    /// for an unknown kind or an out-of-range value.
    pub fn from_db(kind: &str, value: i64) -> Option<Recurrence> {
        match kind {
            "EveryNDays" if value >= 1 => Some(Recurrence::EveryNDays(value as u32)),
            "MonthlyDay" if (1..=31).contains(&value) => Some(Recurrence::MonthlyDay(value as u32)),
            _ => None,
        }
    }

    /// Thai label for the cadence, e.g. "ทุก 7 วัน" / "ทุกวันที่ 1".
    pub fn label_th(&self) -> String {
        match self {
            Recurrence::EveryNDays(n) => format!("ทุก {n} วัน"),
            Recurrence::MonthlyDay(d) => format!("ทุกวันที่ {d}"),
        }
    }

    /// The most recent occurrence on or before `today`, not earlier than
    /// `start`. `None` when no occurrence has happened yet (`today < start`, or
    /// — for monthly — the first qualifying day-of-month is still in the future).
    pub fn latest_occurrence_on_or_before(
        &self,
        start: NaiveDate,
        today: NaiveDate,
    ) -> Option<NaiveDate> {
        match self {
            Recurrence::EveryNDays(n) => {
                if today < start {
                    return None;
                }
                let n = *n as i64;
                let k = (today - start).num_days() / n; // floor, both >= 0
                Some(start + Duration::days(k * n))
            }
            Recurrence::MonthlyDay(d) => {
                let this = occ_in_month(today.year(), today.month(), *d);
                let occ = if this <= today {
                    this
                } else {
                    let (py, pm) = prev_month(today.year(), today.month());
                    occ_in_month(py, pm, *d)
                };
                if occ < start {
                    None
                } else {
                    Some(occ)
                }
            }
        }
    }

    /// The next occurrence strictly after `after` that is also `>= start` — used
    /// only to show "รอบถัดไป" in the schedule table.
    pub fn next_occurrence_after(&self, start: NaiveDate, after: NaiveDate) -> NaiveDate {
        match self {
            Recurrence::EveryNDays(n) => {
                if start > after {
                    return start;
                }
                let n = *n as i64;
                let k = (after - start).num_days() / n + 1;
                start + Duration::days(k * n)
            }
            Recurrence::MonthlyDay(d) => {
                let (mut y, mut m) = if start > after {
                    (start.year(), start.month())
                } else {
                    (after.year(), after.month())
                };
                loop {
                    let occ = occ_in_month(y, m, *d);
                    if occ > after && occ >= start {
                        return occ;
                    }
                    let (ny, nm) = next_month(y, m);
                    y = ny;
                    m = nm;
                }
            }
        }
    }
}

/// A recurring schedule row. `last_generated` is the occurrence date of the most
/// recent `Todo` created from it (`None` = none yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoSchedule {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub task: String,
    pub recurrence: Recurrence,
    pub start_date: NaiveDate,
    pub last_generated: Option<NaiveDate>,
    pub created_at: DateTime<Local>,
}

/// Day-of-month `day` in (`year`, `month`), clamped to the month's last day.
fn occ_in_month(year: i32, month: u32, day: u32) -> NaiveDate {
    let d = day.min(last_day_of_month(year, month));
    NaiveDate::from_ymd_opt(year, month, d).expect("clamped day is valid")
}

/// Number of days in (`year`, `month`): the day before the first of next month.
fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = next_month(year, month);
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .expect("first of month is valid")
        .pred_opt()
        .expect("has a previous day")
        .day()
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn every_n_days_latest_occurrence() {
        let r = Recurrence::EveryNDays(7);
        let start = d("2026-06-01");
        // Before start → none.
        assert_eq!(r.latest_occurrence_on_or_before(start, d("2026-05-31")), None);
        // Exactly on start → start.
        assert_eq!(r.latest_occurrence_on_or_before(start, start), Some(start));
        // Mid-cycle (start 1, N=7, today 23) → 22.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-23")),
            Some(d("2026-06-22"))
        );
        // Exactly on a cycle boundary → that day.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-15")),
            Some(d("2026-06-15"))
        );
    }

    #[test]
    fn every_n_days_next_occurrence() {
        let r = Recurrence::EveryNDays(7);
        let start = d("2026-06-01");
        // Future start → start itself.
        assert_eq!(r.next_occurrence_after(start, d("2026-05-20")), start);
        // On start → next cycle.
        assert_eq!(r.next_occurrence_after(start, start), d("2026-06-08"));
        // Mid-cycle → next boundary.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-23")), d("2026-06-29"));
    }

    #[test]
    fn monthly_day_latest_occurrence() {
        let r = Recurrence::MonthlyDay(15);
        let start = d("2026-01-15");
        // Today after the 15th → this month's 15th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-20")),
            Some(d("2026-06-15"))
        );
        // Today before the 15th → previous month's 15th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-06-10")),
            Some(d("2026-05-15"))
        );
        // today < start → none.
        assert_eq!(r.latest_occurrence_on_or_before(start, d("2026-01-10")), None);
    }

    #[test]
    fn monthly_day_clamps_to_month_end() {
        let r = Recurrence::MonthlyDay(31);
        let start = d("2026-01-01");
        // February 2026 has 28 days → the 31st clamps to the 28th.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-02-28")),
            Some(d("2026-02-28"))
        );
        // On Feb 27 the latest is January's 31st.
        assert_eq!(
            r.latest_occurrence_on_or_before(start, d("2026-02-27")),
            Some(d("2026-01-31"))
        );
    }

    #[test]
    fn monthly_day_next_occurrence() {
        let r = Recurrence::MonthlyDay(10);
        let start = d("2026-01-10");
        // Mid-month, after the 10th → next month's 10th.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-25")), d("2026-07-10"));
        // Before the 10th → this month's 10th.
        assert_eq!(r.next_occurrence_after(start, d("2026-06-05")), d("2026-06-10"));
        // Future start → the first occurrence on/after start.
        let future = d("2027-03-10");
        assert_eq!(r.next_occurrence_after(future, d("2026-06-25")), d("2027-03-10"));
    }

    #[test]
    fn from_db_round_trips_and_rejects_bad_values() {
        let a = Recurrence::EveryNDays(7);
        let b = Recurrence::MonthlyDay(15);
        assert_eq!(Recurrence::from_db(a.kind_str(), a.value()), Some(a));
        assert_eq!(Recurrence::from_db(b.kind_str(), b.value()), Some(b));
        assert_eq!(Recurrence::from_db("EveryNDays", 0), None);
        assert_eq!(Recurrence::from_db("MonthlyDay", 32), None);
        assert_eq!(Recurrence::from_db("Bogus", 5), None);
    }
}
