//! What a CalDAV server answers (`207 Multi-Status`, RFC 4918 and 4791),
//! read for what the agenda tool asks (ADR-205): each response's address
//! and, from its properties found (`200`), the principal and calendar home
//! it names, whether it is a calendar and of which components, its name,
//! and the iCalendar text of an event. Nothing is fetched or expanded: no
//! entity but XML's own and character references, no DTD. Elements are
//! known by their local name; the number of responses is bounded.

use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};

/// The most responses read from one answer.
pub const MAX_RESPONSES: usize = 1_000;
/// The deepest nesting read: deeper elements are not looked into.
const MAX_DEPTH: usize = 32;

/// What one response says of a resource, its properties found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub href: String,
    /// `current-user-principal`.
    pub principal: Option<String>,
    /// `calendar-home-set`.
    pub home: Option<String>,
    /// Its resource type names a calendar.
    pub calendar: bool,
    /// The components it holds, when it says (`supported-calendar-component-set`).
    pub components: Option<Vec<String>>,
    pub name: Option<String>,
    /// Its iCalendar text (`calendar-data`).
    pub data: Option<String>,
}

impl Found {
    /// A calendar holding events (or not saying what it holds).
    pub fn holds_events(&self) -> bool {
        self.calendar
            && self.components.as_ref().is_none_or(|components| {
                components.iter().any(|c| c.eq_ignore_ascii_case("VEVENT"))
            })
    }
}

/// What a property found holds, read while its `propstat` is open: kept
/// only if its status says found.
#[derive(Default)]
struct Pending {
    found: Found,
    status: String,
}

/// The responses of a multi-status answer; none if it is not one.
pub fn multistatus(xml: &str) -> Option<Vec<Found>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut path: Vec<String> = Vec::new();
    let mut responses = Vec::new();
    let mut response: Option<Found> = None;
    let mut pending: Option<Pending> = None;
    let mut text = String::new();
    let mut saw_multistatus = false;
    loop {
        let event = reader.read_event().ok()?;
        match event {
            Event::Start(element) => {
                let name = local(&element);
                if path.len() < MAX_DEPTH {
                    opened(&name, &element, &mut response, &mut pending);
                }
                saw_multistatus |= name == "multistatus" && path.is_empty();
                path.push(name);
                text.clear();
            }
            Event::Empty(element) => {
                let name = local(&element);
                path.push(name.clone());
                if path.len() <= MAX_DEPTH {
                    opened(&name, &element, &mut response, &mut pending);
                }
                path.pop();
            }
            Event::Text(run) => text.push_str(&run.xml10_content()),
            Event::CData(run) => text.push_str(&run.xml10_content()),
            Event::GeneralRef(reference) => text.extend(predefined(&reference)),
            Event::End(_) => {
                let name = path.pop()?;
                let closed = std::mem::take(&mut text);
                closing(&name, &path, closed, &mut response, &mut pending);
                if name == "response"
                    && let Some(done) = response.take()
                {
                    responses.push(done);
                    if responses.len() == MAX_RESPONSES {
                        break;
                    }
                }
            }
            // An answer cut short is not read.
            Event::Eof if !path.is_empty() => return None,
            Event::Eof => break,
            _ => {}
        }
    }
    saw_multistatus.then_some(responses)
}

fn local(element: &BytesStart<'_>) -> String {
    element.local_name().as_ref().to_ascii_lowercase()
}

/// An element opened: a response, a property's status, a resource type, a
/// component.
fn opened(
    name: &str,
    element: &BytesStart<'_>,
    response: &mut Option<Found>,
    pending: &mut Option<Pending>,
) {
    match name {
        "response" => *response = Some(Found::default()),
        "propstat" if response.is_some() => *pending = Some(Pending::default()),
        "calendar" => {
            if let Some(pending) = pending {
                pending.found.calendar = true;
            }
        }
        "supported-calendar-component-set" => {
            if let Some(pending) = pending {
                pending.found.components = Some(Vec::new());
            }
        }
        "comp" => {
            let component = element
                .attributes()
                .flatten()
                .find(|attribute| attribute.key.local_name().as_ref() == "name")
                .map(|attribute| attribute.value.trim().to_string());
            if let (Some(pending), Some(component)) = (pending, component)
                && let Some(components) = &mut pending.found.components
            {
                components.push(component);
            }
        }
        _ => {}
    }
}

/// An element closed, with the text it held: what it says, where.
fn closing(
    name: &str,
    path: &[String],
    text: String,
    response: &mut Option<Found>,
    pending: &mut Option<Pending>,
) {
    let parent = path.last().map(String::as_str);
    let Some(current) = response else {
        return;
    };
    match (name, parent) {
        ("href", Some("response")) => current.href = text.trim().to_string(),
        ("status", Some("propstat")) => {
            if let Some(pending) = pending {
                pending.status = text;
            }
        }
        ("propstat", _) => {
            let Some(done) = pending.take() else {
                return;
            };
            if found_status(&done.status) {
                merge(current, done.found);
            }
        }
        _ => {
            let Some(pending) = pending else {
                return;
            };
            let found = &mut pending.found;
            match (name, parent) {
                ("href", Some("current-user-principal")) => found.principal = Some(trimmed(text)),
                ("href", Some("calendar-home-set")) if found.home.is_none() => {
                    found.home = Some(trimmed(text));
                }
                ("displayname", _) => found.name = Some(trimmed(text)),
                ("calendar-data", _) => found.data = Some(text),
                _ => {}
            }
        }
    }
}

fn trimmed(text: String) -> String {
    text.trim().to_string()
}

/// `HTTP/1.1 200 OK`, or a server saying it with no reason phrase.
fn found_status(status: &str) -> bool {
    status.split_whitespace().nth(1) == Some("200")
}

/// What a `propstat` found, added to its response.
fn merge(into: &mut Found, from: Found) {
    into.principal = into.principal.take().or(from.principal);
    into.home = into.home.take().or(from.home);
    into.calendar |= from.calendar;
    into.components = into.components.take().or(from.components);
    into.name = into.name.take().or(from.name);
    into.data = into.data.take().or(from.data);
}

/// XML's own entities and character references; nothing else.
fn predefined(reference: &BytesRef<'_>) -> Option<char> {
    if reference.is_char_ref() {
        return reference.resolve_char_ref().ok().flatten();
    }
    match reference.xml10_content().as_ref() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/dav/calendars/paul/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/paul/personnel/</d:href>
    <d:propstat><d:prop>
        <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
        <d:displayname>Agenda &amp; rendez-vous</d:displayname>
        <c:supported-calendar-component-set><c:comp name="VEVENT"/></c:supported-calendar-component-set>
      </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
    <d:propstat><d:prop><c:calendar-home-set/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status></d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/calendars/paul/taches/</d:href>
    <d:propstat><d:prop>
        <d:resourcetype><d:collection/><c:calendar/></d:resourcetype>
        <c:supported-calendar-component-set><c:comp name="VTODO"/></c:supported-calendar-component-set>
      </d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat>
  </d:response>
</d:multistatus>"#;

    #[test]
    fn reads_each_response_and_its_properties_found() {
        let read = multistatus(HOME).unwrap();
        assert_eq!(read.len(), 3);
        assert!(!read[0].calendar);
        assert_eq!(read[1].href, "/dav/calendars/paul/personnel/");
        assert!(read[1].holds_events());
        assert_eq!(read[1].name.as_deref(), Some("Agenda & rendez-vous"));
        assert_eq!(read[1].home, None);
        assert!(read[2].calendar && !read[2].holds_events());
    }

    #[test]
    fn reads_a_principal_a_home_and_calendar_data() {
        let principal = r#"<multistatus xmlns="DAV:"><response><href>/dav/</href><propstat>
            <prop><current-user-principal><href>/dav/principals/paul/</href></current-user-principal></prop>
            <status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"#;
        let read = multistatus(principal).unwrap();
        assert_eq!(read[0].principal.as_deref(), Some("/dav/principals/paul/"));
        let data = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
            <D:response><D:href>/e.ics</D:href><D:propstat><D:prop>
            <C:calendar-data><![CDATA[BEGIN:VCALENDAR
SUMMARY:a &amp; b
END:VCALENDAR]]></C:calendar-data></D:prop><D:status>HTTP/1.1 200 OK</D:status>
            </D:propstat></D:response></D:multistatus>"#;
        let read = multistatus(data).unwrap();
        assert!(
            read[0]
                .data
                .as_deref()
                .unwrap()
                .contains("SUMMARY:a &amp; b")
        );
    }

    #[test]
    fn what_is_not_a_multistatus_is_refused() {
        assert_eq!(multistatus("<html><body>Bienvenue</body></html>"), None);
        assert_eq!(multistatus("<multistatus><response>"), None);
        assert_eq!(multistatus("pas du XML"), None);
        // A declared entity is never expanded.
        let entity = r#"<!DOCTYPE m [<!ENTITY x "piège">]><multistatus xmlns="DAV:">
            <response><href>&x;/a</href></response></multistatus>"#;
        assert_eq!(multistatus(entity).unwrap()[0].href, "/a");
    }
}
