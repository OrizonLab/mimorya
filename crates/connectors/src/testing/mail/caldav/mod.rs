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
//! its own, eleven agendas or none of events, list a read-only agenda first
//! or only read-only ones, redirect, refuse with a status, answer at a
//! length past what a client reads, or never answer. What it answers is written by
//! `answers`; the events created there are kept by `objects`, which says
//! whether the server invites attendees itself and whom it knows the
//! account as.

mod answers;
mod objects;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub use self::answers::READ_ONLY_PATH;
use self::answers::{Rights, agenda, events, listing, multistatus, response};
pub use self::objects::{Created, CreatedObject};
use self::objects::{options, principal_addresses, put};
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
    /// Lists a read-only agenda of events before « Personnel ».
    ReadOnlyFirst,
    /// Lets the account read its agenda of events, not write there.
    ReadOnly,
    /// Does not send invitations itself: no `calendar-auto-schedule`.
    NoScheduling,
    /// Knows the account's principal by these addresses.
    Addresses(&'static [&'static str]),
    /// Answers what is put there with this status, keeping nothing.
    PutRefuses(u16),
    /// Keeps what is put there, and never answers.
    PutHangs,
    /// Never answers, once signed in.
    Hangs,
}

/// A running CalDAV server and what it heard.
pub struct FakeCalDav {
    pub address: SocketAddr,
    pub heard: Heard,
    pub created: Created,
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
        let created = Created::default();
        let state = Arc::new(Served {
            accounts,
            behaviour,
            events,
            heard: heard.clone(),
            created: created.clone(),
        });
        let router = Router::new().fallback(answer).with_state(state);
        let address = serve_https(authority, router).await;
        Self {
            address,
            heard,
            created,
        }
    }
}

struct Served {
    accounts: Accounts,
    behaviour: CalDavBehaviour,
    events: Vec<String>,
    heard: Heard,
    created: Created,
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
    let Some(username) = signed_in(&served.accounts, &headers) else {
        let challenge = [(header::WWW_AUTHENTICATE, "Basic realm=\"agenda\"")];
        return (StatusCode::UNAUTHORIZED, challenge).into_response();
    };
    if served.behaviour == CalDavBehaviour::Hangs {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
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
    let names_principal = format!(
        "<d:current-user-principal><d:href>{principal}</d:href></d:current-user-principal>"
    );
    let lists_at_root = matches!(
        served.behaviour,
        CalDavBehaviour::RootOnly | CalDavBehaviour::PrincipalWithoutHome
    );
    match (method.as_str(), path.as_str(), served.behaviour) {
        ("OPTIONS", _, behaviour) => options(behaviour),
        ("PUT", _, behaviour) => put(&served.created, behaviour, &path, &headers, body).await,
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
        ("REPORT", READ_ONLY_PATH, CalDavBehaviour::ReadOnlyFirst) => multistatus(""),
        ("PROPFIND", "/dav/", CalDavBehaviour::Redirects) => (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, "https://agenda.ailleurs.example/dav/")],
        )
            .into_response(),
        ("PROPFIND", "/dav/", CalDavBehaviour::DeclaredIsAgenda) => {
            let itself = agenda("/dav/", "Agenda", "VEVENT", Rights::Write);
            multistatus(&itself.replace("<d:prop>", &format!("<d:prop>{names_principal}")))
        }
        ("PROPFIND", "/dav/", CalDavBehaviour::DeclaredNamesHome(named)) => multistatus(&response(
            "/dav/",
            &format!("<c:calendar-home-set><d:href>{named}</d:href></c:calendar-home-set>"),
        )),
        ("PROPFIND", "/dav/", _) => multistatus(&response(
            "/dav/",
            &format!("{names_principal}<d:resourcetype><d:collection/></d:resourcetype>"),
        )),
        ("PROPFIND", "/dav/principals/paul/", behaviour) => multistatus(&response(
            "/dav/principals/paul/",
            &format!(
                "<c:calendar-home-set><d:href>{home}</d:href></c:calendar-home-set>{}",
                principal_addresses(behaviour, &username)
            ),
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

/// The account `Authorization` names, if it is one of the server's.
fn signed_in(accounts: &Accounts, headers: &HeaderMap) -> Option<String> {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| STANDARD.decode(encoded).ok())
        .and_then(|decoded| String::from_utf8(decoded).ok())?;
    let (username, password) = given.split_once(':')?;
    accepts(accounts, username, password).then(|| username.to_string())
}
