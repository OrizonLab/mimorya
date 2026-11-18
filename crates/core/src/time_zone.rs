//! The platform's time zone: the model is told today's date in it, and the
//! days a tool is given are read in it (ADR-117), so that both agree; a
//! monthly budget runs from the first of its months (ADR-207).

use chrono::{DateTime, Datelike, Days, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

pub const PLATFORM_TIME_ZONE: Tz = chrono_tz::Europe::Paris;

/// The start of the month `now` falls in, in the platform's time zone: a
/// monthly budget (ADR-207) runs from it.
pub fn month_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&PLATFORM_TIME_ZONE).date_naive();
    let first = (local - Days::new(u64::from(local.day0()))).and_time(NaiveTime::MIN);
    // Midnight is never skipped nor repeated in Paris (its clocks change at
    // two or three in the morning).
    PLATFORM_TIME_ZONE
        .from_local_datetime(&first)
        .earliest()
        .map_or_else(|| first.and_utc(), |start| start.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    #[test]
    fn a_month_starts_at_midnight_in_paris_whatever_the_season() {
        assert_eq!(
            month_start(at("2026-11-17T10:00:00Z")),
            at("2026-10-31T23:00:00Z")
        );
        assert_eq!(
            month_start(at("2026-07-15T12:00:00Z")),
            at("2026-06-30T22:00:00Z")
        );
        // Already December in Paris, still November in UTC.
        assert_eq!(
            month_start(at("2026-11-30T23:30:00Z")),
            at("2026-11-30T23:00:00Z")
        );
        // A month whose first day follows a clock change.
        assert_eq!(
            month_start(at("2026-04-01T00:30:00Z")),
            at("2026-03-31T22:00:00Z")
        );
    }
}
