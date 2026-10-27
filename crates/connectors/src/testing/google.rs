//! Gmail and Google Calendar on a local port, for tests (feature
//! `testing`, ADR-122). A token `acces-<owner>-<n>`, as the test provider
//! mints them, opens `<owner>`'s mailbox and calendar: three e-mails (one
//! in ISO-8859-1 with an attachment, one in HTML only) and two events
//! naming their owner, so that a test sees whose account was read. It
//! records what it received, refuses the tokens a test chose (401), and
//! answers certain e-mails' identifiers with a refusal or an oversized
//! body. Its errors carry a detail the tools must not pass on.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, RawQuery, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use super::graph::{FORBIDDEN_MESSAGE, OVERSIZED_MESSAGE};

/// What a Google error says beyond its status: never shown to the model.
pub const GOOGLE_DETAIL: &str = "détail-interne-de-google";

/// A request Gmail or Calendar received.
#[derive(Debug, Clone)]
pub struct GoogleRequest {
    pub token: String,
    pub path: String,
    /// In order, a name repeated as sent.
    pub query: Vec<(String, String)>,
}

impl GoogleRequest {
    /// The first value of a query parameter.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Default)]
struct Received {
    requests: Vec<GoogleRequest>,
    refused: Vec<String>,
}

type Shared = Arc<Mutex<Received>>;

pub struct FakeGoogle {
    /// Gmail's address, `http://127.0.0.1:<port>/gmail/v1`.
    pub gmail: String,
    /// Calendar's, `http://127.0.0.1:<port>/calendar/v3`.
    pub calendar: String,
    shared: Shared,
}

impl FakeGoogle {
    pub async fn start() -> Self {
        let shared = Shared::default();
        let router = Router::new()
            .route("/gmail/v1/users/me/messages", get(messages))
            .route("/gmail/v1/users/me/messages/{id}", get(message))
            .route("/calendar/v3/calendars/primary/events", get(events))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test Google");
        });
        Self {
            gmail: format!("http://{address}/gmail/v1"),
            calendar: format!("http://{address}/calendar/v3"),
            shared,
        }
    }

    pub fn requests(&self) -> Vec<GoogleRequest> {
        self.shared.lock().expect("google").requests.clone()
    }

    /// Answers 401 to this token from now on (revoked before it expired).
    pub fn refuse(&self, token: &str) {
        self.shared
            .lock()
            .expect("google")
            .refused
            .push(token.into());
    }
}

fn encoded(text: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(text)
}

/// The e-mails of `owner`'s mailbox, newest first, in full.
fn mailbox(owner: &str) -> Vec<Value> {
    let headers = |subject: &str| {
        json!([
            { "name": "Subject", "value": format!("{subject} — {owner}") },
            { "name": "From", "value": "Fournisseur <contact@fournisseur.example>" },
            { "name": "To", "value": format!("{owner} <{owner}@exemple.fr>") },
            { "name": "Cc", "value": "Compta <compta@exemple.fr>" },
        ])
    };
    // "Préparé pour <owner>." in ISO-8859-1.
    let mut latin = b"Pr\xe9par\xe9 pour ".to_vec();
    latin.extend_from_slice(owner.as_bytes());
    latin.push(b'.');
    vec![
        json!({
            "id": "m3", "internalDate": "1761562800000", "labelIds": ["INBOX"],
            "snippet": format!("Planning de novembre &amp; congés — {owner}"),
            "payload": {
                "mimeType": "text/html", "filename": "", "headers": headers("Planning de novembre"),
                "body": { "data": encoded(format!("<p>Bonjour&nbsp;{owner},</p><p>Le planning.</p>").as_bytes()) },
            },
        }),
        json!({
            "id": "m2", "internalDate": "1760950800000", "labelIds": ["INBOX", "UNREAD"],
            "snippet": format!("Facture de mars — {owner}"),
            "payload": {
                "mimeType": "multipart/mixed", "filename": "", "headers": headers("Facture de mars"),
                "parts": [
                    { "mimeType": "multipart/alternative", "filename": "", "parts": [
                        { "mimeType": "text/plain", "filename": "",
                          "headers": [{ "name": "Content-Type", "value": "text/plain; charset=\"ISO-8859-1\"" }],
                          "body": { "data": encoded(&latin) } },
                        { "mimeType": "text/html", "filename": "",
                          "body": { "data": encoded(b"<p>version HTML</p>") } },
                    ] },
                    { "mimeType": "application/pdf", "filename": "facture-mars.pdf",
                      "body": { "attachmentId": "a1", "size": 1024 } },
                ],
            },
        }),
        json!({
            "id": "m1", "internalDate": "1756717200000", "labelIds": ["INBOX"],
            "snippet": format!("Bienvenue — {owner}"),
            "payload": {
                "mimeType": "text/plain", "filename": "", "headers": headers("Bienvenue"),
                "body": { "data": encoded(format!("Bienvenue {owner}.").as_bytes()) },
            },
        }),
    ]
}

/// An e-mail as Gmail gives it in its metadata: the headers asked only.
fn metadata(message: &Value, asked: &[String]) -> Value {
    let mut kept = message.clone();
    let headers: Vec<Value> = message["payload"]["headers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|h| asked.iter().any(|name| h["name"] == name.as_str()))
        .collect();
    kept["payload"] = json!({ "mimeType": message["payload"]["mimeType"], "headers": headers });
    kept
}

fn failure(status: StatusCode) -> Response {
    let body = json!({ "error": { "code": status.as_u16(), "message": GOOGLE_DETAIL } });
    (status, axum::Json(body)).into_response()
}

/// The owner of the token received, recorded; Google's status if refused.
fn owner(
    shared: &Shared,
    headers: &HeaderMap,
    path: String,
    raw: Option<String>,
) -> Result<(String, Vec<(String, String)>), StatusCode> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    let query: Vec<(String, String)> =
        url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes())
            .into_owned()
            .collect();
    let mut received = shared.lock().expect("google");
    received.requests.push(GoogleRequest {
        token: token.clone(),
        path,
        query: query.clone(),
    });
    let owner = token
        .strip_prefix("acces-")
        .and_then(|rest| rest.rsplit_once('-'))
        .map(|(owner, _)| owner.to_string());
    match owner {
        Some(owner) if !received.refused.contains(&token) => Ok((owner, query)),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn messages(
    State(shared): State<Shared>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let (owner, query) = match owner(&shared, &headers, "/users/me/messages".into(), raw) {
        Ok(found) => found,
        Err(refused) => return failure(refused),
    };
    let param = |name: &str| {
        query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    // Words only: the operators Gmail reads are its own.
    let words: Vec<String> = param("q")
        .unwrap_or_default()
        .split_whitespace()
        .filter(|word| !word.contains(':'))
        .map(str::to_lowercase)
        .collect();
    let found: Vec<Value> = mailbox(&owner)
        .into_iter()
        .filter(|m| {
            let snippet = m["snippet"].as_str().unwrap_or_default().to_lowercase();
            words.iter().all(|word| snippet.contains(word))
        })
        .map(|m| json!({ "id": m["id"], "threadId": m["id"] }))
        .collect();
    let max = param("maxResults")
        .and_then(|m| m.parse().ok())
        .unwrap_or(100);
    let mut body = json!({ "messages": found.iter().take(max).collect::<Vec<_>>() });
    if found.len() > max {
        body["nextPageToken"] = json!("suite");
    }
    axum::Json(body).into_response()
}

async fn message(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(raw): RawQuery,
) -> Response {
    let path = format!("/users/me/messages/{id}");
    let (owner, query) = match owner(&shared, &headers, path, raw) {
        Ok(found) => found,
        Err(refused) => return failure(refused),
    };
    if id == FORBIDDEN_MESSAGE {
        return failure(StatusCode::FORBIDDEN);
    }
    if id == OVERSIZED_MESSAGE {
        let huge = json!({ "id": id, "snippet": "x".repeat(2 * 1024 * 1024) });
        return axum::Json(huge).into_response();
    }
    let Some(found) = mailbox(&owner).into_iter().find(|m| m["id"] == id.as_str()) else {
        // Another person's e-mail is not found in this mailbox.
        return failure(StatusCode::NOT_FOUND);
    };
    let asked: Vec<String> = query
        .iter()
        .filter(|(name, _)| name == "metadataHeaders")
        .map(|(_, value)| value.clone())
        .collect();
    let format = query.iter().find(|(name, _)| name == "format");
    match format.map(|(_, value)| value.as_str()) {
        Some("metadata") => axum::Json(metadata(&found, &asked)).into_response(),
        _ => axum::Json(found).into_response(),
    }
}

async fn events(
    State(shared): State<Shared>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let path = "/calendars/primary/events".to_string();
    let owner = match owner(&shared, &headers, path, raw) {
        Ok((owner, _)) => owner,
        Err(refused) => return failure(refused),
    };
    let body = json!({ "items": [
        {
            "id": "e1", "status": "confirmed", "summary": format!("Point d’équipe — {owner}"),
            "start": { "dateTime": "2026-10-27T09:00:00+01:00" },
            "end": { "dateTime": "2026-10-27T10:00:00+01:00" },
            "location": "Salle Atlas",
            "organizer": { "email": format!("{owner}@exemple.fr"), "displayName": owner },
            "attendees": [{ "email": "lea@exemple.fr", "displayName": "Léa Martin" }],
            "description": "<p>Ordre du jour</p>",
            "htmlLink": format!("https://calendar.example/{owner}/e1"),
        },
        {
            "id": "e2", "status": "confirmed", "summary": format!("Séminaire — {owner}"),
            "start": { "date": "2026-10-28" }, "end": { "date": "2026-10-29" },
            "organizer": { "email": format!("{owner}@exemple.fr") },
            "htmlLink": format!("https://calendar.example/{owner}/e2"),
        },
    ]});
    axum::Json(body).into_response()
}
