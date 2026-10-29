//! Slack's Web API on a local port, for tests (feature `testing`,
//! ADR-125). Its bot's token, `BOT_TOKEN`, is the application of the
//! workspace `WORKSPACE`, whose bot user is `BOT_USER`; `OTHER_TOKEN` is an
//! application's of another workspace; any other token is refused
//! (`invalid_auth`), as one a test revoked; a test may also make it
//! unavailable (503), refuse the messages posted, or those with blocks,
//! counting the messages the application tries to post, not say whose a
//! conversation is, or give the token other permissions than the
//! application asks for (also at `POST /test/scopes`, ADR-132). It knows the
//! people a test adds, by address, deleted or of another workspace, and
//! keeps the messages the application posts (to a person's id: in their
//! direct conversation, `D…` after their `U…`, which is the person's whom
//! `conversations.info` names), with their blocks, as it rewrites them
//! (`chat.update`, ADR-129), also listed at `GET /test/posts` for the
//! end-to-end tests. A test signs the events it sends as Slack would
//! (`signed`).

mod routes;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::{get, post};
use mimorya_core::gateways::slack::{BOT_SCOPES, signature};
use serde_json::Value;

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
    /// A message with blocks is refused (`invalid_blocks`).
    blocks_refused: bool,
    /// The error code a message posted is answered with, if a test chose
    /// one.
    post_refusal: Option<String>,
    /// How many messages the application tried to post.
    post_attempts: usize,
    /// The error code `conversations.info` is answered with, if a test
    /// chose one.
    conversation_refusal: Option<String>,
    /// The permissions the token has, said with `auth.test`
    /// (`x-oauth-scopes`), if Slack says them.
    scopes: Option<Vec<String>>,
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
        let shared = Arc::new(Mutex::new(Shared {
            scopes: Some(BOT_SCOPES.iter().map(|s| (*s).to_owned()).collect()),
            ..Shared::default()
        }));
        let router = Router::new()
            .route("/api/auth.test", post(routes::auth_test))
            .route("/api/users.info", get(routes::users_info))
            .route("/api/conversations.info", get(routes::conversation_info))
            .route("/api/chat.postMessage", post(routes::post_message))
            .route("/api/chat.update", post(routes::update_message))
            .route("/test/posts", get(routes::posts))
            .route("/test/scopes", post(routes::grant_scopes))
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

    /// A message posted with blocks is refused from now on, as blocks Slack
    /// would not show (`false`: taken again).
    pub fn refuse_blocks(&self, refused: bool) {
        self.shared.lock().expect("lock").blocks_refused = refused;
    }

    /// A message posted is answered with this error code from now on, the
    /// other methods as Slack would (`None`: taken again).
    pub fn refuse_posts_with(&self, code: Option<&str>) {
        self.shared.lock().expect("lock").post_refusal = code.map(str::to_owned);
    }

    /// `conversations.info` is refused with this error code from now on, as
    /// without `im:read` or for a conversation the application cannot see
    /// (`None`: answered again).
    pub fn refuse_conversations_with(&self, code: Option<&str>) {
        self.shared.lock().expect("lock").conversation_refusal = code.map(str::to_owned);
    }

    /// The permissions Slack says the token has from now on, those the
    /// application asks for at first (`None`: Slack says none).
    pub fn grant_scopes(&self, scopes: Option<&[&str]>) {
        self.shared.lock().expect("lock").scopes =
            scopes.map(|scopes| scopes.iter().map(|s| (*s).to_owned()).collect());
    }

    /// How many messages the application tried to post, refused included.
    pub fn post_attempts(&self) -> usize {
        self.shared.lock().expect("lock").post_attempts
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
