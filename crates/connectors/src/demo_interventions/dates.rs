//! Calendar helpers (UTC dates) used by the demo dataset and planning.

use chrono::{DateTime, Datelike, Days, NaiveDate, Utc, Weekday};

/// Monday of the week containing `date`.
pub fn monday_of(date: NaiveDate) -> NaiveDate {
    date.checked_sub_days(Days::new(u64::from(date.weekday().num_days_from_monday())))
        .expect("date in range")
}

/// Monday (UTC) of the week containing an instant.
pub fn monday_of_instant(instant: DateTime<Utc>) -> NaiveDate {
    monday_of(instant.date_naive())
}

pub fn iso_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

pub fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Days from `from` to `to` included, at most 31.
pub fn dates_between(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    from.iter_days().take_while(|d| *d <= to).take(31).collect()
}
