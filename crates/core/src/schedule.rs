//! When a scheduled task runs (ADR-080): on chosen days of the week, at a
//! time of day, in a time zone, as its author reads the clock. A time the
//! clocks skip (moved forward) runs as soon as it exists again; a time they
//! go through twice (moved back) runs the first time.

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Days, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::DomainError;

const NO_DAY: &str = "Choisissez au moins un jour de la semaine.";
const BAD_DAY: &str = "Les jours vont de 1 (lundi) à 7 (dimanche).";
const BAD_TIME: &str = "L’heure s’écrit HH:MM, de 00:00 à 23:59.";
const BAD_ZONE: &str = "Fuseau horaire inconnu.";

/// Days of the week (ISO: 1 is Monday), a time of day and a time zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklySchedule {
    days: BTreeSet<u8>,
    at: NaiveTime,
    zone: Tz,
}

impl WeeklySchedule {
    /// Checks a schedule as written: `time` as `HH:MM`, `zone` an IANA name.
    pub fn parse(days: &[u8], time: &str, zone: &str) -> Result<Self, DomainError> {
        if days.is_empty() {
            return Err(DomainError::validation(NO_DAY));
        }
        if days.iter().any(|d| !(1..=7).contains(d)) {
            return Err(DomainError::validation(BAD_DAY));
        }
        let at = (time.len() == 5)
            .then(|| NaiveTime::parse_from_str(time, "%H:%M").ok())
            .flatten()
            .ok_or_else(|| DomainError::validation(BAD_TIME))?;
        let zone: Tz = zone
            .parse()
            .map_err(|_| DomainError::validation(BAD_ZONE))?;
        Ok(Self {
            days: days.iter().copied().collect(),
            at,
            zone,
        })
    }

    /// Its days, from Monday, once each.
    pub fn days(&self) -> Vec<u8> {
        self.days.iter().copied().collect()
    }

    /// Its time, as `HH:MM`.
    pub fn time(&self) -> String {
        self.at.format("%H:%M").to_string()
    }

    pub fn zone(&self) -> &str {
        self.zone.name()
    }

    /// The first moment it runs strictly after `after`.
    pub fn next_after(&self, after: DateTime<Utc>) -> DateTime<Utc> {
        let today = after.with_timezone(&self.zone).date_naive();
        // A week and a day ahead: every chosen day is met.
        (0..=8u64)
            .filter_map(|ahead| today.checked_add_days(Days::new(ahead)))
            .filter(|day| self.days.contains(&iso_day(*day)))
            .map(|day| self.moment(day))
            .find(|moment| *moment > after)
            .unwrap_or_else(|| after + chrono::Duration::weeks(1))
    }

    /// The moment of `day` at its time, in its zone.
    fn moment(&self, day: NaiveDate) -> DateTime<Utc> {
        let mut local = day.and_time(self.at);
        // A gap lasts at most a few hours: the first minute that exists.
        for _ in 0..=24 * 60 {
            match self.zone.from_local_datetime(&local) {
                LocalResult::Single(at) => return at.with_timezone(&Utc),
                LocalResult::Ambiguous(first, _) => return first.with_timezone(&Utc),
                LocalResult::None => local += chrono::Duration::minutes(1),
            }
        }
        Utc.from_utc_datetime(&day.and_time(self.at))
    }
}

fn iso_day(day: NaiveDate) -> u8 {
    u8::try_from(day.weekday().number_from_monday()).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(text: &str) -> DateTime<Utc> {
        text.parse().unwrap()
    }

    fn weekdays_at(time: &str) -> WeeklySchedule {
        WeeklySchedule::parse(&[1, 2, 3, 4, 5], time, "Europe/Paris").unwrap()
    }

    #[test]
    fn checks_what_is_written() {
        let parsed = WeeklySchedule::parse(&[5, 1, 1], "07:30", "Europe/Paris").unwrap();
        assert_eq!(parsed.days(), [1, 5]);
        assert_eq!(parsed.time(), "07:30");
        assert_eq!(parsed.zone(), "Europe/Paris");
        let message = |r: Result<WeeklySchedule, DomainError>| r.unwrap_err().message;
        assert_eq!(message(WeeklySchedule::parse(&[], "07:30", "UTC")), NO_DAY);
        assert_eq!(
            message(WeeklySchedule::parse(&[0], "07:30", "UTC")),
            BAD_DAY
        );
        assert_eq!(
            message(WeeklySchedule::parse(&[8], "07:30", "UTC")),
            BAD_DAY
        );
        for time in ["7:30", "24:00", "07:60", "07:30:00", "midi"] {
            assert_eq!(message(WeeklySchedule::parse(&[1], time, "UTC")), BAD_TIME);
        }
        assert_eq!(
            message(WeeklySchedule::parse(&[1], "07:30", "Paris")),
            BAD_ZONE
        );
    }

    #[test]
    fn runs_on_the_next_chosen_day_at_its_time() {
        let schedule = weekdays_at("08:00");
        // Thursday 22 October 2026, 07:59 in Paris (summer time, UTC+2).
        assert_eq!(
            schedule.next_after(utc("2026-10-22T05:59:00Z")),
            utc("2026-10-22T06:00:00Z")
        );
        // Not twice: strictly after, the next working day.
        assert_eq!(
            schedule.next_after(utc("2026-10-22T06:00:00Z")),
            utc("2026-10-23T06:00:00Z")
        );
        // Friday evening: Monday, after the clocks went back (UTC+1).
        assert_eq!(
            schedule.next_after(utc("2026-10-23T18:00:00Z")),
            utc("2026-10-26T07:00:00Z")
        );
        // One day a week, the week after.
        let sundays = WeeklySchedule::parse(&[7], "08:00", "UTC").unwrap();
        assert_eq!(
            sundays.next_after(utc("2026-10-25T08:00:00Z")),
            utc("2026-11-01T08:00:00Z")
        );
    }

    #[test]
    fn a_skipped_time_runs_once_it_exists_and_a_repeated_one_the_first_time() {
        // 29 March 2026: 02:00 becomes 03:00 in Paris; 02:30 does not exist.
        let every_day =
            WeeklySchedule::parse(&[1, 2, 3, 4, 5, 6, 7], "02:30", "Europe/Paris").unwrap();
        assert_eq!(
            every_day.next_after(utc("2026-03-28T23:00:00Z")),
            utc("2026-03-29T01:00:00Z")
        );
        // 25 October 2026: 02:30 happens twice; the first one (UTC+2).
        assert_eq!(
            every_day.next_after(utc("2026-10-24T23:00:00Z")),
            utc("2026-10-25T00:30:00Z")
        );
        // Not again at the second 02:30: the next day.
        assert_eq!(
            every_day.next_after(utc("2026-10-25T00:30:00Z")),
            utc("2026-10-26T01:30:00Z")
        );
    }
}
