//! The events of an agenda over a period (ADR-205): a `calendar-query`
//! report (RFC 4791, 7.8) asking the server for the events that overlap
//! it, each occurrence of a recurring one apart (`expand`); what it sends
//! read up to a bound, its iCalendar texts read by the platform.

use chrono::{DateTime, Utc};

use super::agendas::Agenda;
use super::client::CalDav;
use super::ical::{Event, events};
use crate::outbound::Cause;

/// What the server may send of one agenda's events.
pub const REPORT_LIMIT: usize = 512 * 1024;

/// The events of `agenda` between `from` and `to`.
pub async fn agenda_events(
    dav: &CalDav<'_>,
    agenda: &Agenda,
    (from, to): (DateTime<Utc>, DateTime<Utc>),
) -> Result<Vec<Event>, Cause> {
    let (start, end) = (stamp(from), stamp(to));
    let query = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
<d:prop><c:calendar-data><c:expand start="{start}" end="{end}"/></c:calendar-data></d:prop>
<c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT">
<c:time-range start="{start}" end="{end}"/>
</c:comp-filter></c:comp-filter></c:filter>
</c:calendar-query>"#
    );
    let found = dav
        .ask("REPORT", &agenda.url, "1", query, REPORT_LIMIT)
        .await?;
    // All of them: the tool keeps those of the period, then bounds them.
    Ok(found
        .iter()
        .filter_map(|found| found.data.as_deref())
        .flat_map(events)
        .collect())
}

/// An instant as CalDAV writes it, in UTC.
fn stamp(at: DateTime<Utc>) -> String {
    at.format("%Y%m%dT%H%M%SZ").to_string()
}
