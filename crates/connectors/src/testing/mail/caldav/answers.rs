//! What the CalDAV server of the tests answers (ADR-205): a multi-status of
//! responses, its calendar home and the agendas it lists, each with what
//! the account may do there, and the events it holds.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{AGENDA_PATH, CalDavBehaviour};

/// Agendas of events on other servers: another host, named in full or
/// from the scheme on, and the same host on another port.
pub const ELSEWHERE_AGENDAS: [&str; 3] = [
    "https://agenda.ailleurs.example/dav/x/",
    "//agenda.ailleurs.example/dav/y/",
    "https://127.0.0.1:1/dav/z/",
];
/// The agenda listed first, read-only, when the server lists one.
pub const READ_ONLY_PATH: &str = "/dav/calendars/paul/anniversaires/";
/// The agenda listed after « Personnel », when the server names a default.
pub const WORK_PATH: &str = "/dav/calendars/paul/travail/";
/// The principal's scheduling Inbox, when the server names a default.
pub const INBOX_PATH: &str = "/dav/inbox/paul/";
/// An Inbox that never answers, when the principal names it.
pub const SILENT_INBOX_PATH: &str = "/dav/inbox/silent/";

/// What the account may do in an agenda: read it, or also write there.
#[derive(Clone, Copy)]
pub enum Rights {
    Read,
    Write,
}

pub fn multistatus(responses: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus xmlns:d=\"DAV:\" \
         xmlns:c=\"urn:ietf:params:xml:ns:caldav\">{responses}</d:multistatus>"
    );
    let kind = [(header::CONTENT_TYPE, "application/xml; charset=utf-8")];
    (StatusCode::MULTI_STATUS, kind, body).into_response()
}

pub fn response(href: &str, properties: &str) -> String {
    format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{properties}</d:prop>\
         <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
    )
}

/// The calendar home: itself, its agendas of events, one of tasks, and
/// agendas elsewhere if it lists them.
pub fn listing(behaviour: CalDavBehaviour) -> String {
    let home = response(
        "/dav/calendars/paul/",
        "<d:resourcetype><d:collection/></d:resourcetype>",
    );
    let personal = |rights| agenda(AGENDA_PATH, "Personnel", "VEVENT", rights);
    let events: Vec<String> = match behaviour {
        CalDavBehaviour::TasksOnly => Vec::new(),
        CalDavBehaviour::ManyAgendas => (0..11)
            .map(|at| {
                let path = format!("/dav/calendars/paul/agenda-{at}/");
                agenda(&path, "Agenda", "VEVENT", Rights::Write)
            })
            .collect(),
        CalDavBehaviour::ReadOnlyFirst => vec![
            agenda(READ_ONLY_PATH, "Anniversaires", "VEVENT", Rights::Read),
            personal(Rights::Write),
        ],
        CalDavBehaviour::ReadOnly => vec![personal(Rights::Read)],
        CalDavBehaviour::DefaultAgenda(..) => vec![
            agenda(READ_ONLY_PATH, "Anniversaires", "VEVENT", Rights::Read),
            personal(Rights::Write),
            agenda(WORK_PATH, "Travail", "VEVENT", Rights::Write),
        ],
        _ => vec![personal(Rights::Write)],
    };
    let tasks = agenda(
        "/dav/calendars/paul/taches/",
        "Tâches",
        "VTODO",
        Rights::Write,
    );
    std::iter::once(home)
        .chain(events)
        .chain(std::iter::once(tasks))
        .chain(match behaviour {
            CalDavBehaviour::ListsElsewhere => ELSEWHERE_AGENDAS
                .iter()
                .map(|href| agenda(href, "Ailleurs", "VEVENT", Rights::Write))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// An agenda holding `component`s, where the account has `rights`.
pub fn agenda(href: &str, name: &str, component: &str, rights: Rights) -> String {
    let privileges = match rights {
        Rights::Read => "<d:privilege><d:read/></d:privilege>",
        Rights::Write => {
            "<d:privilege><d:read/></d:privilege><d:privilege><d:write/></d:privilege>"
        }
    };
    response(
        href,
        &format!(
            "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>\
             <d:displayname>{name}</d:displayname><c:supported-calendar-component-set>\
             <c:comp name=\"{component}\"/></c:supported-calendar-component-set>\
             <d:current-user-privilege-set>{privileges}</d:current-user-privilege-set>"
        ),
    )
}

/// Each event as a calendar object, its text escaped for XML.
pub fn events(events: &[String]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(at, event)| {
            let text = format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{event}END:VCALENDAR\r\n");
            let escaped = text.replace('&', "&amp;").replace('<', "&lt;");
            response(
                &format!("{AGENDA_PATH}{at}.ics"),
                &format!("<c:calendar-data>{escaped}</c:calendar-data>"),
            )
        })
        .collect()
}
