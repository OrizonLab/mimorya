//! A CalDAV server for the tests (ADR-205), over HTTPS with a certificate
//! of a test authority: its accounts given by Basic authentication, a
//! principal, a calendar home holding an agenda of events (« Personnel »)
//! and one of tasks, the events a test gives answered to any `REPORT`
//! whatever the period (a client keeps those it asked for). It notes each
//! request (method, path, whether it asked to expand occurrences), never a
//! credential. It may also be an agenda at its root, name its calendar home
//! there, name neither a principal nor a home (or a principal naming no
//! home), its root then listing the agendas, name a principal or a
//! calendar home on another server, list agendas on other servers beside
//! its own, eleven agendas or none of events, redirect, refuse with a
//! status, or answer at a length past what a client reads.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::{Accounts, Heard, accepts};
use crate::testing::TestAuthority;
use crate::testing::https::serve_https;

/// The agenda path a client is to find.
pub const AGENDA_PATH: &str = "/dav/calendars/paul/personnel/";

/// What the server does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalDavBehaviour {
    Answers,
    /// Names, as the principal, this address on another server.
    PrincipalElsewhere(&'static str),
    /// Names, as the calendar home, this address on another server.
    HomeElsewhere(&'static str),
    /// Lists agendas of events on other servers beside its own.
    ListsElsewhere,
    /// Lists no agenda of events: tasks alone.
    TasksOnly,
    /// Is an agenda of events at its root, the address declared.
    DeclaredIsAgenda,
    /// Names, at its root, this address as the calendar home.
    DeclaredNamesHome(&'static str),
    /// Names neither a principal nor a home at its root, which lists the
    /// agendas.
    RootOnly,
    /// Names a principal that names no home; its root lists the agendas.
    PrincipalWithoutHome,
    /// Lists eleven agendas of events.
    ManyAgendas,
    /// Answers every request, once signed in, with this status.
    Refuses(u16),
    /// Redirects the declared address elsewhere.
    Redirects,
    /// Answers a `REPORT` past what a client reads.
    AnswersAtLength,
}

/// A running CalDAV server and what it heard.
pub struct FakeCalDav {
    pub address: SocketAddr,
    pub heard: Heard,
}

impl FakeCalDav {
    /// The address a company declares: its root.
    pub fn base(&self) -> String {
        format!("https://127.0.0.1:{}/dav/", self.address.port())
    }

    /// A server accepting `accounts`, holding `events` (each a `VEVENT`).
    pub async fn start(
        authority: &TestAuthority,
        accounts: Accounts,
        behaviour: CalDavBehaviour,
        events: Vec<String>,
    ) -> Self {
        let heard = Heard::default();
        let state = Arc::new(Served {
            accounts,
            behaviour,
            events,
            heard: heard.clone(),
        });
        let router = Router::new().fallback(answer).with_state(state);
        let address = serve_https(authority, router).await;
        Self { address, heard }
    }
}

struct Served {
    accounts: Accounts,
    behaviour: CalDavBehaviour,
    events: Vec<String>,
    heard: Heard,
}

async fn answer(
    State(served): State<Arc<Served>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: String,
) -> Response {
    let path = uri.path().to_string();
    let depth = headers
        .get("Depth")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("0")
        .to_string();
    let expand = if body.contains("expand") {
        " expand"
    } else {
        ""
    };
    served.heard.note(format!("{method} {path}{expand}"));
    if !signed_in(&served.accounts, &headers) {
        let challenge = [(header::WWW_AUTHENTICATE, "Basic realm=\"agenda\"")];
        return (StatusCode::UNAUTHORIZED, challenge).into_response();
    }
    if let CalDavBehaviour::Refuses(code) = served.behaviour {
        let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        return status.into_response();
    }
    let principal = match served.behaviour {
        CalDavBehaviour::PrincipalElsewhere(elsewhere) => elsewhere,
        _ => "/dav/principals/paul/",
    };
    let home = match served.behaviour {
        CalDavBehaviour::HomeElsewhere(elsewhere) => elsewhere,
        _ => "/dav/calendars/paul/",
    };
    let lists_at_root = matches!(
        served.behaviour,
        CalDavBehaviour::RootOnly | CalDavBehaviour::PrincipalWithoutHome
    );
    match (method.as_str(), path.as_str(), served.behaviour) {
        ("PROPFIND", "/dav/", behaviour) if lists_at_root && depth == "1" => {
            multistatus(&listing(behaviour))
        }
        ("PROPFIND", "/dav/", CalDavBehaviour::RootOnly) => multistatus(&response(
            "/dav/",
            "<d:resourcetype><d:collection/></d:resourcetype>",
        )),
        ("PROPFIND", "/dav/principals/paul/", CalDavBehaviour::PrincipalWithoutHome) => {
            multistatus(&response(
                "/dav/principals/paul/",
                "<d:displayname>Paul</d:displayname>",
            ))
        }
        ("REPORT", agenda_path, CalDavBehaviour::ManyAgendas)
            if agenda_path.starts_with("/dav/calendars/paul/agenda-") =>
        {
            multistatus("")
        }
        ("PROPFIND", "/dav/", CalDavBehaviour::Redirects) => (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, "https://agenda.ailleurs.example/dav/")],
        )
            .into_response(),
        ("PROPFIND", "/dav/", CalDavBehaviour::DeclaredIsAgenda) => {
            multistatus(&agenda("/dav/", "Agenda", "VEVENT"))
        }
        ("PROPFIND", "/dav/", CalDavBehaviour::DeclaredNamesHome(named)) => multistatus(&response(
            "/dav/",
            &format!("<c:calendar-home-set><d:href>{named}</d:href></c:calendar-home-set>"),
        )),
        ("PROPFIND", "/dav/", _) => multistatus(&response(
            "/dav/",
            &format!(
                "<d:current-user-principal><d:href>{principal}</d:href></d:current-user-principal>\
                 <d:resourcetype><d:collection/></d:resourcetype>"
            ),
        )),
        ("PROPFIND", "/dav/principals/paul/", _) => multistatus(&response(
            "/dav/principals/paul/",
            &format!("<c:calendar-home-set><d:href>{home}</d:href></c:calendar-home-set>"),
        )),
        ("PROPFIND", "/dav/calendars/paul/", behaviour) => multistatus(&listing(behaviour)),
        ("REPORT", AGENDA_PATH, CalDavBehaviour::AnswersAtLength) => {
            multistatus(&"<d:response><d:href>/x</d:href></d:response>".repeat(20_000))
        }
        ("REPORT", AGENDA_PATH, _) | ("REPORT", "/dav/", CalDavBehaviour::DeclaredIsAgenda) => {
            multistatus(&events(&served.events))
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Whether `Authorization` names one of the server's accounts.
fn signed_in(accounts: &Accounts, headers: &HeaderMap) -> bool {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| STANDARD.decode(encoded).ok())
        .and_then(|decoded| String::from_utf8(decoded).ok());
    given
        .as_deref()
        .and_then(|given| given.split_once(':'))
        .is_some_and(|(username, password)| accepts(accounts, username, password))
}

fn multistatus(responses: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<d:multistatus xmlns:d=\"DAV:\" \
         xmlns:c=\"urn:ietf:params:xml:ns:caldav\">{responses}</d:multistatus>"
    );
    let kind = [(header::CONTENT_TYPE, "application/xml; charset=utf-8")];
    (StatusCode::MULTI_STATUS, kind, body).into_response()
}

fn response(href: &str, properties: &str) -> String {
    format!(
        "<d:response><d:href>{href}</d:href><d:propstat><d:prop>{properties}</d:prop>\
         <d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>"
    )
}

/// Agendas of events on other servers: another host, named in full or
/// from the scheme on, and the same host on another port.
pub const ELSEWHERE_AGENDAS: [&str; 3] = [
    "https://agenda.ailleurs.example/dav/x/",
    "//agenda.ailleurs.example/dav/y/",
    "https://127.0.0.1:1/dav/z/",
];

/// The calendar home: itself, an agenda of events, one of tasks, and
/// agendas elsewhere if it lists them.
fn listing(behaviour: CalDavBehaviour) -> String {
    let home = response(
        "/dav/calendars/paul/",
        "<d:resourcetype><d:collection/></d:resourcetype>",
    );
    let events: Vec<String> = match behaviour {
        CalDavBehaviour::TasksOnly => Vec::new(),
        CalDavBehaviour::ManyAgendas => (0..11)
            .map(|at| {
                agenda(
                    &format!("/dav/calendars/paul/agenda-{at}/"),
                    "Agenda",
                    "VEVENT",
                )
            })
            .collect(),
        _ => vec![agenda(AGENDA_PATH, "Personnel", "VEVENT")],
    };
    let tasks = agenda("/dav/calendars/paul/taches/", "Tâches", "VTODO");
    std::iter::once(home)
        .chain(events)
        .chain(std::iter::once(tasks))
        .chain(match behaviour {
            CalDavBehaviour::ListsElsewhere => ELSEWHERE_AGENDAS
                .iter()
                .map(|href| agenda(href, "Ailleurs", "VEVENT"))
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// An agenda holding `component`s.
fn agenda(href: &str, name: &str, component: &str) -> String {
    response(
        href,
        &format!(
            "<d:resourcetype><d:collection/><c:calendar/></d:resourcetype>\
             <d:displayname>{name}</d:displayname><c:supported-calendar-component-set>\
             <c:comp name=\"{component}\"/></c:supported-calendar-component-set>"
        ),
    )
}

/// Each event as a calendar object, its text escaped for XML.
fn events(events: &[String]) -> String {
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
