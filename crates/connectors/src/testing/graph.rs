//! Microsoft Graph on a local port, for tests (feature `testing`,
//! ADR-117). A token `acces-<owner>-<n>`, as the test provider mints them,
//! opens `<owner>`'s mailbox and calendar: three e-mails and two events
//! naming their owner, so that a test sees whose account was read. It
//! records what it received, refuses the tokens a test chose (401), and
//! answers certain e-mails' identifiers with a refusal or an oversized
//! body. Its errors carry a detail the tools must not pass on.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};

/// What a Graph error says beyond its status: never shown to the model.
pub const GRAPH_DETAIL: &str = "détail-interne-de-graph";
/// The e-mail whose reading Graph refuses (403), and the one too long.
pub const FORBIDDEN_MESSAGE: &str = "interdit";
pub const OVERSIZED_MESSAGE: &str = "enorme";

/// A request Graph received.
#[derive(Debug, Clone)]
pub struct GraphRequest {
    pub token: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub prefer: Option<String>,
}

#[derive(Default)]
struct Received {
    requests: Vec<GraphRequest>,
    refused: HashSet<String>,
}

type Shared = Arc<Mutex<Received>>;

pub struct FakeGraph {
    /// Its address, `http://127.0.0.1:<port>/v1.0`, as a configuration names it.
    pub url: String,
    shared: Shared,
}

impl FakeGraph {
    pub async fn start() -> Self {
        let shared = Shared::default();
        let router = Router::new()
            .route("/v1.0/me/messages", get(messages))
            .route("/v1.0/me/messages/{id}", get(message))
            .route("/v1.0/me/calendarView", get(events))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test Graph");
        });
        Self {
            url: format!("http://{address}/v1.0"),
            shared,
        }
    }

    pub fn requests(&self) -> Vec<GraphRequest> {
        self.shared.lock().expect("graph").requests.clone()
    }

    /// Answers 401 to this token from now on (revoked before it expired).
    pub fn refuse(&self, token: &str) {
        self.shared
            .lock()
            .expect("graph")
            .refused
            .insert(token.into());
    }

    /// The e-mails of `owner`'s mailbox, newest first.
    pub fn mailbox(owner: &str) -> Vec<Value> {
        vec![
            mail(owner, "m3", "Planning de novembre", "2026-10-26T09:00:00Z"),
            mail(owner, "m2", "Facture de mars", "2026-10-20T09:00:00Z"),
            mail(owner, "m1", "Bienvenue", "2026-09-01T09:00:00Z"),
        ]
    }
}

fn mail(owner: &str, n: &str, subject: &str, received: &str) -> Value {
    json!({
        "id": format!("{owner}/{n}="),
        "subject": format!("{subject} — {owner}"),
        "from": { "emailAddress": { "name": "Fournisseur", "address": "contact@fournisseur.example" } },
        "toRecipients": [{ "emailAddress": { "name": owner, "address": format!("{owner}@exemple.fr") } }],
        "ccRecipients": [],
        "receivedDateTime": received,
        "bodyPreview": format!("Bonjour {owner}"),
        "body": { "contentType": "text", "content": format!("Bonjour {owner},\n\n{subject}.") },
        "isRead": false,
        "hasAttachments": n == "m2",
        "webLink": format!("https://outlook.example/{owner}/{n}"),
    })
}

fn event(owner: &str, n: &str, subject: &str, start: &str, end: &str) -> Value {
    json!({
        "id": format!("{owner}-{n}"),
        "subject": format!("{subject} — {owner}"),
        "start": { "dateTime": start, "timeZone": "Europe/Paris" },
        "end": { "dateTime": end, "timeZone": "Europe/Paris" },
        "isAllDay": false,
        "isCancelled": false,
        "location": { "displayName": "Salle Atlas" },
        "organizer": { "emailAddress": { "name": owner, "address": format!("{owner}@exemple.fr") } },
        "attendees": [{ "type": "required", "emailAddress": { "name": "Léa Martin", "address": "lea@exemple.fr" } }],
        "bodyPreview": "Ordre du jour",
        "webLink": format!("https://outlook.example/{owner}/{n}"),
    })
}

fn failure(status: StatusCode) -> Response {
    let body = json!({ "error": { "code": "Refused", "message": GRAPH_DETAIL } });
    (status, axum::Json(body)).into_response()
}

/// The owner of the token received, recorded; Graph's status if refused.
fn owner(
    shared: &Shared,
    headers: &HeaderMap,
    path: String,
    query: &HashMap<String, String>,
) -> Result<String, StatusCode> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    let mut received = shared.lock().expect("graph");
    received.requests.push(GraphRequest {
        token: token.clone(),
        path,
        query: query.clone(),
        prefer: headers
            .get("prefer")
            .and_then(|value| value.to_str().ok())
            .map(String::from),
    });
    let owner = token
        .strip_prefix("acces-")
        .and_then(|rest| rest.rsplit_once('-'))
        .map(|(owner, _)| owner.to_string());
    match owner {
        Some(owner) if !received.refused.contains(&token) => Ok(owner),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn messages(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let owner = match owner(&shared, &headers, "/me/messages".into(), &query) {
        Ok(owner) => owner,
        Err(refused) => return failure(refused),
    };
    let words = query
        .get("$search")
        .map(|w| w.trim_matches('"').to_lowercase());
    let found: Vec<Value> = FakeGraph::mailbox(&owner)
        .into_iter()
        .filter(|m| {
            let subject = m["subject"].as_str().unwrap_or_default().to_lowercase();
            words
                .as_ref()
                .is_none_or(|words| words.split(' ').all(|word| subject.contains(word)))
        })
        .collect();
    let top = query.get("$top").and_then(|t| t.parse().ok()).unwrap_or(10);
    let mut body = json!({ "value": found.iter().take(top).collect::<Vec<_>>() });
    if found.len() > top {
        body["@odata.nextLink"] = json!("https://graph.example/suite");
    }
    axum::Json(body).into_response()
}

async fn message(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let owner = match owner(&shared, &headers, format!("/me/messages/{id}"), &query) {
        Ok(owner) => owner,
        Err(refused) => return failure(refused),
    };
    if id == FORBIDDEN_MESSAGE {
        return failure(StatusCode::FORBIDDEN);
    }
    if id == OVERSIZED_MESSAGE {
        let mut huge = mail(&owner, "m9", "Énorme", "2026-10-01T09:00:00Z");
        huge["body"]["content"] = json!("x".repeat(2 * 1024 * 1024));
        return axum::Json(huge).into_response();
    }
    match FakeGraph::mailbox(&owner)
        .into_iter()
        .find(|m| m["id"] == id.as_str())
    {
        Some(found) => axum::Json(found).into_response(),
        // Another person's e-mail is not found in this mailbox.
        None => failure(StatusCode::NOT_FOUND),
    }
}

async fn events(
    State(shared): State<Shared>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let owner = match owner(&shared, &headers, "/me/calendarView".into(), &query) {
        Ok(owner) => owner,
        Err(refused) => return failure(refused),
    };
    let body = json!({ "value": [
        event(&owner, "e1", "Point d’équipe", "2026-10-27T09:00:00.0000000", "2026-10-27T10:00:00.0000000"),
        event(&owner, "e2", "Visite client", "2026-10-28T14:00:00.0000000", "2026-10-28T16:00:00.0000000"),
    ]});
    axum::Json(body).into_response()
}
