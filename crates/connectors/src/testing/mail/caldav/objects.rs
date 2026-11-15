//! What the CalDAV server of the tests does with events created there
//! (ADR-205): whether it sends invitations itself (its `DAV` header, RFC
//! 6638), the addresses it knows an account's principal by, and the
//! calendar objects put into an agenda that takes them, each kept as
//! received; one put over another with `If-None-Match: *` refused. It may
//! refuse what is put with a status, or keep it and never answer.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{AGENDA_PATH, CalDavBehaviour};

/// A calendar object put on the server, as received.
#[derive(Debug, Clone)]
pub struct CreatedObject {
    pub path: String,
    pub text: String,
    pub content_type: Option<String>,
    pub if_none_match: Option<String>,
}

/// The calendar objects put on the server.
#[derive(Debug, Clone, Default)]
pub struct Created(Arc<Mutex<Vec<CreatedObject>>>);

impl Created {
    pub fn all(&self) -> Vec<CreatedObject> {
        self.0.lock().expect("created").clone()
    }
}

/// What `OPTIONS` answers: the server's capabilities.
pub fn options(behaviour: CalDavBehaviour) -> Response {
    let dav = match behaviour {
        CalDavBehaviour::NoScheduling => "1, 3, access-control, calendar-access",
        _ => "1, 3, access-control, calendar-access, calendar-auto-schedule",
    };
    let headers = [
        ("DAV", dav),
        (header::ALLOW.as_str(), "OPTIONS, PROPFIND, REPORT, PUT"),
    ];
    (StatusCode::OK, headers).into_response()
}

/// The addresses of the principal of `username`, as a property.
pub fn principal_addresses(behaviour: CalDavBehaviour, username: &str) -> String {
    let addresses: Vec<String> = match behaviour {
        CalDavBehaviour::Addresses(given) => given.iter().map(|a| (*a).to_string()).collect(),
        _ => vec!["/dav/principals/paul/".into(), format!("mailto:{username}")],
    };
    let hrefs: String = addresses
        .iter()
        .map(|address| format!("<d:href>{address}</d:href>"))
        .collect();
    format!("<c:calendar-user-address-set>{hrefs}</c:calendar-user-address-set>")
}

/// Whether `path` is a new object in an agenda that takes events.
fn in_agenda(behaviour: CalDavBehaviour, path: &str) -> bool {
    let inside = |agenda: &str| {
        path.strip_prefix(agenda)
            .is_some_and(|name| !name.is_empty() && !name.contains('/'))
    };
    match behaviour {
        CalDavBehaviour::ReadOnly => false,
        CalDavBehaviour::DeclaredIsAgenda | CalDavBehaviour::AgendaWithPrincipalElsewhere(_) => {
            inside("/dav/")
        }
        _ => inside(AGENDA_PATH),
    }
}

/// A calendar object put at `path`.
pub async fn put(
    created: &Created,
    behaviour: CalDavBehaviour,
    path: &str,
    headers: &HeaderMap,
    text: String,
) -> Response {
    if let CalDavBehaviour::PutRefuses(code) = behaviour {
        return StatusCode::from_u16(code)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response();
    }
    if !in_agenda(behaviour, path) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let given = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(String::from)
    };
    let object = CreatedObject {
        path: path.to_string(),
        text,
        content_type: given(header::CONTENT_TYPE.as_str()),
        if_none_match: given(header::IF_NONE_MATCH.as_str()),
    };
    {
        let mut kept = created.0.lock().expect("created");
        let taken = kept.iter().any(|known| known.path == path);
        if taken && object.if_none_match.as_deref() == Some("*") {
            return StatusCode::PRECONDITION_FAILED.into_response();
        }
        kept.push(object);
    }
    if behaviour == CalDavBehaviour::PutHangs {
        // Kept, and never answered.
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
    StatusCode::CREATED.into_response()
}
