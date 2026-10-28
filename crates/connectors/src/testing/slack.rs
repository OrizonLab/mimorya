//! Slack's Web API on a local port, for tests (feature `testing`,
//! ADR-125). Its bot's token, `BOT_TOKEN`, is the application of the
//! workspace `WORKSPACE`, whose bot user is `BOT_USER`; `OTHER_TOKEN` is an
//! application's of another workspace; any other token is refused
//! (`invalid_auth`), as one a test revoked; a test may also make it
//! unavailable (503). It knows the people a test
//! adds, by address, deleted or of another workspace, and keeps the messages
//! the application posts, with their blocks, as it rewrites them
//! (`chat.update`, ADR-129), also listed at `GET /test/posts` for the
//! end-to-end tests. A test signs the events it sends as Slack would
//! (`signed`).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use mimorya_core::gateways::slack::signature;
use serde_json::{Value, json};

/// The application's bot token, as Slack issues them.
pub const BOT_TOKEN: &str = "xoxb-fictif-bot-neo-maintenance-0001";
/// Its signing secret.
pub const SIGNING_SECRET: &str = "5ec2e75ec2e75ec2e75ec2e75ec2e75e";
/// Its workspace, as Slack names it, and its name.
pub const WORKSPACE: &str = "T0NEOMAINT";
pub const WORKSPACE_NAME: &str = "Néo Maintenance";
/// Its bot user.
pub const BOT_USER: &str = "U0MIMORYA";
/// An application's bot token in another workspace, `OTHER_WORKSPACE`.
pub const OTHER_TOKEN: &str = "xoxb-fictif-bot-autre-entreprise-0001";
pub const OTHER_WORKSPACE: &str = "T0AUTRE";

/// A message the application posted, as it last wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackPost {
    pub channel: String,
    pub thread_ts: Option<String>,
    /// Its own timestamp, which names it.
    pub ts: String,
    pub text: String,
    /// What it shows, if not its text alone.
    pub blocks: Option<Value>,
    /// How many times the application rewrote it.
    pub updates: u32,
}

#[derive(Debug, Clone)]
struct Person {
    address: Option<String>,
    workspace: String,
    deleted: bool,
}

#[derive(Default)]
struct Shared {
    people: HashMap<String, Person>,
    posts: Vec<SlackPost>,
    revoked: bool,
    /// The error code every method answers with, if a test chose one.
    refusal: Option<String>,
    /// Every method answers 503, as Slack down.
    down: bool,
}

pub struct FakeSlack {
    /// Its address, `http://127.0.0.1:<port>/api`, as the gateway is given it.
    pub url: String,
    shared: Arc<Mutex<Shared>>,
}

impl FakeSlack {
    pub async fn start() -> Self {
        Self::start_on("127.0.0.1:0").await
    }

    /// The same, at `address` (the end-to-end tests read it there).
    pub async fn start_on(address: &str) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let router = Router::new()
            .route("/api/auth.test", post(auth_test))
            .route("/api/users.info", get(users_info))
            .route("/api/chat.postMessage", post(post_message))
            .route("/api/chat.update", post(update_message))
            .route("/test/posts", get(posts))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test Slack");
        });
        Self {
            url: format!("http://{address}/api"),
            shared,
        }
    }

    /// Adds a person of the application's workspace, with this address.
    pub fn add_person(&self, id: &str, address: &str) {
        self.put(id, Some(address), WORKSPACE, false);
    }

    /// Adds a person of another workspace (a shared conversation).
    pub fn add_foreigner(&self, id: &str, address: &str) {
        self.put(id, Some(address), OTHER_WORKSPACE, false);
    }

    /// Adds a person Slack shows without an address, or deleted.
    pub fn add_person_as(&self, id: &str, address: Option<&str>, deleted: bool) {
        self.put(id, address, WORKSPACE, deleted);
    }

    fn put(&self, id: &str, address: Option<&str>, workspace: &str, deleted: bool) {
        let person = Person {
            address: address.map(str::to_owned),
            workspace: workspace.to_owned(),
            deleted,
        };
        self.shared
            .lock()
            .expect("lock")
            .people
            .insert(id.to_owned(), person);
    }

    /// Revokes the bot's token: every method refuses it from now on.
    pub fn revoke(&self) {
        self.shared.lock().expect("lock").revoked = true;
    }

    /// Every method answers with this error code from now on (`None`: as
    /// Slack would).
    pub fn refuse_with(&self, code: Option<&str>) {
        self.shared.lock().expect("lock").refusal = code.map(str::to_owned);
    }

    /// Every method answers 503 from now on (`false`: answers again).
    pub fn unavailable(&self, down: bool) {
        self.shared.lock().expect("lock").down = down;
    }

    /// The messages the application posted, oldest first.
    pub fn posts(&self) -> Vec<SlackPost> {
        self.shared.lock().expect("lock").posts.clone()
    }
}

/// The headers Slack sends with `body`, signed now with the application's
/// secret: its timestamp and its signature.
pub fn signed(body: &str) -> (String, String) {
    signed_at(body, chrono::Utc::now().timestamp())
}

/// The same, signed at `at` (seconds since the epoch).
pub fn signed_at(body: &str, at: i64) -> (String, String) {
    let timestamp = at.to_string();
    let signature = signature(SIGNING_SECRET, &timestamp, body.as_bytes());
    (timestamp, signature)
}

/// Whether the request carries the bot's token, unrevoked, Slack up; the
/// refusal Slack answers otherwise.
fn refused(shared: &Shared, headers: &HeaderMap) -> Option<Response> {
    if shared.down {
        return Some(StatusCode::SERVICE_UNAVAILABLE.into_response());
    }
    if let Some(code) = &shared.refusal {
        return Some(Json(json!({ "ok": false, "error": code })).into_response());
    }
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if !matches!(bearer, Some(BOT_TOKEN | OTHER_TOKEN)) || shared.revoked {
        let code = if shared.revoked {
            "token_revoked"
        } else {
            "invalid_auth"
        };
        return Some(Json(json!({ "ok": false, "error": code })).into_response());
    }
    None
}

async fn auth_test(State(shared): State<Arc<Mutex<Shared>>>, headers: HeaderMap) -> Response {
    let shared = shared.lock().expect("lock");
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    let other = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(&format!("Bearer {OTHER_TOKEN}"));
    let (team, team_id, user_id) = if other {
        ("Autre entreprise", OTHER_WORKSPACE, "U0AUTREBOT")
    } else {
        (WORKSPACE_NAME, WORKSPACE, BOT_USER)
    };
    Json(json!({
        "ok": true,
        "url": "https://neo-maintenance.slack.com/",
        "team": team,
        "user": "mimorya",
        "team_id": team_id,
        "user_id": user_id,
        "bot_id": "B0MIMORYA",
        "is_enterprise_install": false
    }))
    .into_response()
}

async fn users_info(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let shared = shared.lock().expect("lock");
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    let id = query.get("user").cloned().unwrap_or_default();
    let Some(person) = shared.people.get(&id) else {
        return Json(json!({ "ok": false, "error": "user_not_found" })).into_response();
    };
    let mut profile = json!({ "real_name": "Personne de test", "display_name": "test" });
    if let Some(address) = &person.address {
        profile["email"] = json!(address);
    }
    Json(json!({
        "ok": true,
        "user": {
            "id": id,
            "team_id": person.workspace,
            "name": "test",
            "deleted": person.deleted,
            "is_bot": false,
            "is_app_user": false,
            "profile": profile
        }
    }))
    .into_response()
}

async fn post_message(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
    let (Some(channel), Some(text)) = (field("channel"), field("text")) else {
        return Json(json!({ "ok": false, "error": "invalid_arguments" })).into_response();
    };
    let ts = format!("1700000000.{:06}", shared.posts.len() + 1);
    let post = SlackPost {
        channel: channel.clone(),
        thread_ts: field("thread_ts"),
        ts: ts.clone(),
        text,
        blocks: body.get("blocks").cloned(),
        updates: 0,
    };
    shared.posts.push(post);
    Json(json!({ "ok": true, "channel": channel, "ts": ts })).into_response()
}

async fn update_message(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
    let (Some(channel), Some(ts), Some(text)) = (field("channel"), field("ts"), field("text"))
    else {
        return Json(json!({ "ok": false, "error": "invalid_arguments" })).into_response();
    };
    let Some(post) = (shared.posts.iter_mut()).find(|p| p.channel == channel && p.ts == ts) else {
        return Json(json!({ "ok": false, "error": "message_not_found" })).into_response();
    };
    post.text = text;
    post.blocks = body.get("blocks").cloned();
    post.updates += 1;
    Json(json!({ "ok": true, "channel": channel, "ts": ts })).into_response()
}

/// The messages the application posted, oldest first, for the end-to-end
/// tests.
async fn posts(State(shared): State<Arc<Mutex<Shared>>>) -> Json<Value> {
    let shared = shared.lock().expect("lock");
    let posts: Vec<Value> = shared
        .posts
        .iter()
        .map(|post| {
            json!({
                "channel": post.channel,
                "threadTs": post.thread_ts,
                "ts": post.ts,
                "text": post.text,
                "blocks": post.blocks,
                "updates": post.updates,
            })
        })
        .collect();
    Json(json!(posts))
}
