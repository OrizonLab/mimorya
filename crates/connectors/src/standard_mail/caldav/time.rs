//! When an event of an iCalendar text starts, ends or lasts (RFC 5545),
//! as the agenda tool reads it (ADR-205): a whole day, an instant in UTC or
//! at an IANA zone's time (a time in the gap of a change of time read with
//! the offset before it, 3.3.5), or a floating time when its zone is not
//! one the platform knows; a duration read strictly, never past what an
//! instant can hold.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// When an event starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// A whole day (`VALUE=DATE`).
    Day(NaiveDate),
    /// An instant: in UTC, or at a known zone's time.
    At(DateTime<Utc>),
    /// A time with no zone, read in the platform's.
    Floating(NaiveDateTime),
}

/// A start or an end written `value`, of `kind` (`VALUE`) and in `zone`
/// (`TZID`).
pub fn when(value: &str, kind: Option<&str>, zone: Option<&str>) -> Option<When> {
    let value = value.trim();
    let is_day = kind.is_some_and(|kind| kind.eq_ignore_ascii_case("DATE")) || value.len() == 8;
    if is_day {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(When::Day);
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let at = NaiveDateTime::parse_from_str(utc, "%Y%m%dT%H%M%S").ok()?;
        return Some(When::At(at.and_utc()));
    }
    let at = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let zone = zone.and_then(|name| name.parse::<Tz>().ok());
    Some(match zone.and_then(|zone| in_zone(zone, at)) {
        Some(instant) => When::At(instant),
        None => When::Floating(at),
    })
}

/// The instant `at` is in `zone`: the earlier of two in a change's
/// overlap, and in its gap the one the offset before it gives, looked for
/// three hours before (a gap longer than that, as a day skipped, finds
/// none).
pub fn in_zone(zone: Tz, at: NaiveDateTime) -> Option<DateTime<Utc>> {
    match zone.from_local_datetime(&at) {
        LocalResult::Single(local) | LocalResult::Ambiguous(local, _) => {
            Some(local.with_timezone(&Utc))
        }
        LocalResult::None => {
            let before = at.checked_sub_signed(Duration::hours(3))?;
            let offset = zone.offset_from_local_datetime(&before).earliest()?;
            let seconds = i64::from(offset.fix().local_minus_utc());
            Some(at.checked_sub_signed(Duration::seconds(seconds))?.and_utc())
        }
    }
}

/// A duration (RFC 5545, 3.3.6), read strictly: weeks alone, or days then
/// a time of hours, minutes and seconds in that order, each once; none when
/// it is malformed or past what an instant can hold.
pub fn duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    let (negative, value) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value.strip_prefix('+').unwrap_or(value)),
    };
    let rest = value.strip_prefix('P')?;
    let total = match rest.strip_suffix('W') {
        Some(weeks) => Duration::try_weeks(count(weeks)?)?,
        None => days_and_time(rest)?,
    };
    Some(if negative { -total } else { total })
}

/// A length of so many units.
type Length = fn(i64) -> Option<Duration>;

/// `nD`, `TnHnMnS`, or both: each part at most once, in order.
fn days_and_time(text: &str) -> Option<Duration> {
    let (days, time) = match text.split_once('T') {
        Some((days, time)) => (days, Some(time)),
        None => (text, None),
    };
    if days.is_empty() && time.is_none() {
        return None;
    }
    let mut total = Duration::zero();
    if !days.is_empty() {
        total = Duration::try_days(count(days.strip_suffix('D')?)?)?;
    }
    if let Some(mut rest) = time {
        if rest.is_empty() {
            return None;
        }
        let units: [(char, Length); 3] = [
            ('H', Duration::try_hours),
            ('M', Duration::try_minutes),
            ('S', Duration::try_seconds),
        ];
        for (unit, length) in units {
            if let Some(at) = rest.find(unit) {
                total = total.checked_add(&length(count(&rest[..at])?)?)?;
                rest = &rest[at + 1..];
            }
        }
        if !rest.is_empty() {
            return None;
        }
    }
    Some(total)
}

/// A count of digits alone.
fn count(text: &str) -> Option<i64> {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    digits.then(|| text.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn a_time_reads_in_its_zone_even_in_a_change_s_gap() {
        // 09:00 in Paris in November is 08:00 UTC.
        let nine = Utc.with_ymd_and_hms(2026, 11, 16, 8, 0, 0).unwrap();
        let read = when("20261116T090000", None, Some("Europe/Paris"));
        assert_eq!(read, Some(When::At(nine)));
        // 02:30 does not exist in New York on 8 March 2026: read with the
        // offset before the change (-05:00), it is 07:30 UTC.
        let gap = when("20260308T023000", None, Some("America/New_York"));
        let expected = Utc.with_ymd_and_hms(2026, 3, 8, 7, 30, 0).unwrap();
        assert_eq!(gap, Some(When::At(expected)));
        // In the overlap of 1 November, the earlier of the two.
        let overlap = when("20261101T013000", None, Some("America/New_York"));
        let earlier = Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0).unwrap();
        assert_eq!(overlap, Some(When::At(earlier)));
        let day = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(when("20261118", Some("DATE"), None), Some(When::Day(day)));
        // 02:30 does not exist in Paris on 28 March 2027: with the offset
        // before the change (+01:00), it is 01:30 UTC.
        let paris_gap = NaiveDate::from_ymd_opt(2027, 3, 28)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        let expected = Utc.with_ymd_and_hms(2027, 3, 28, 1, 30, 0).unwrap();
        assert_eq!(in_zone(chrono_tz::Europe::Paris, paris_gap), Some(expected));
    }

    #[test]
    fn a_duration_is_read_strictly() {
        assert_eq!(duration("-PT15M"), Some(-Duration::minutes(15)));
        assert_eq!(duration("P1DT2H"), Some(Duration::hours(26)));
        assert_eq!(duration("P2W"), Some(Duration::weeks(2)));
        assert_eq!(duration("PT1H30M"), Some(Duration::minutes(90)));
        // Malformed, repeated, out of order or past an instant's range.
        for refused in [
            "P1W2D",
            "P",
            "PT",
            "P1D1D",
            "P106751991167D1D",
            "PT5M2H",
            "P1H",
            "P+1D",
            "PT1H1H",
            "P99999999999999999999D",
        ] {
            assert_eq!(duration(refused), None, "{refused}");
        }
    }
}
