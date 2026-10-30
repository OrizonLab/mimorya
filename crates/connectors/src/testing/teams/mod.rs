//! Microsoft on a local port, as a company's Teams bot reaches it, for
//! tests (feature `testing`, ADR-138): Microsoft Entra, which gives a token
//! to the test bot's application (`TEAMS_APP` of `TEAMS_DIRECTORY`, with
//! `TEAMS_SECRET`) or to another company's (`OTHER_TEAMS_APP`) and refuses
//! any other secret; the Bot Framework's OpenID configuration and key set
//! (its test key, endorsed for Teams); and a Teams service under `teams/`,
//! which keeps the messages the bots write, with a token Entra gave, and
//! says who the members of their conversations are; the same service
//! answers under `ailleurs/`, an address no bot may trust (`untrusted_url`).
//! It writes an activity as the Bot Framework sends it (`activity`), with
//! its token (`signing`). A test may make it unavailable (503), slow to say
//! where its keys are, or change what it says of a member. For the end-to-end tests (ADR-139), it lists
//! the messages the bots wrote at `GET /test/messages` and writes, at
//! `POST /test/activity`, the activity a person's message would be with
//! its token (`outside`): whoever reaches the test Microsoft can send the
//! bots what the Bot Framework would.

mod activity;
mod outside;
mod routes;
mod signing;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use serde_json::{Value, json};

pub use signing::{Signer, TEAMS_KEY_ID, signed_token};

/// The test bot's application, its directory and its client secret.
pub const TEAMS_APP: &str = "6f1c2b7a-0d3e-4c5b-9a8f-7e6d5c4b3a21";
pub const TEAMS_DIRECTORY: &str = "0b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9";
pub const TEAMS_SECRET: &str = "Abc8Q~fictif.secret_client-0123456789";
/// Another company's bot.
pub const OTHER_TEAMS_APP: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";
pub const OTHER_TEAMS_DIRECTORY: &str = "1a2b3c4d-5e6f-4789-8abc-def012345678";
pub const OTHER_TEAMS_SECRET: &str = "Zyx8Q~fictif.autre_secret-9876543210";

/// A message a bot wrote in a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamsSent {
    /// The application whose token wrote it.
    pub app: String,
    pub conversation: String,
    pub text: String,
    pub text_format: Option<String>,
}

#[derive(Default)]
struct Shared {
    base: String,
    down: bool,
    /// How late the Bot Framework's configuration is answered.
    delay: Duration,
    /// The tokens Entra gave, and to which application.
    tokens: HashMap<String, String>,
    token_requests: usize,
    configuration_requests: usize,
    key_reads: usize,
    endorsements: Vec<String>,
    /// What Teams says of each member (`29:…`).
    members: HashMap<String, Value>,
    /// The members whose conversations refuse the bot.
    blocked: HashSet<String>,
    sent: Vec<TeamsSent>,
}

pub struct FakeTeams {
    /// Its address, `http://127.0.0.1:<port>`, as the gateways are given it.
    pub url: String,
    shared: Arc<Mutex<Shared>>,
}

impl FakeTeams {
    pub async fn start() -> Self {
        Self::start_on("127.0.0.1:0").await
    }

    /// The same, at `address`: where the end-to-end tests read what the
    /// bots wrote and have an activity signed (ADR-139).
    pub async fn start_on(address: &str) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            endorsements: vec!["msteams".into(), "skype".into()],
            ..Shared::default()
        }));
        let router = Router::new()
            .route("/login/{directory}/oauth2/v2.0/token", post(routes::token))
            .route("/botframework/openid", get(routes::openid))
            .route("/botframework/keys", get(routes::keys))
            .route(
                "/teams/v3/conversations/{conversation}/activities",
                post(routes::send),
            )
            .route(
                "/teams/v3/conversations/{conversation}/members/{member}",
                get(routes::member),
            )
            // The same service at an address no bot may trust: what reaches
            // it is kept alike, so that a test sees nothing does.
            .route(
                "/ailleurs/v3/conversations/{conversation}/activities",
                post(routes::send),
            )
            .route("/test/messages", get(outside::messages))
            .route("/test/activity", post(outside::activity))
            .with_state(shared.clone());
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("local port");
        let address: SocketAddr = listener.local_addr().expect("local address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test Microsoft");
        });
        let url = format!("http://{address}");
        shared.lock().expect("lock").base = url.clone();
        Self { url, shared }
    }

    /// The Teams service its activities name.
    pub fn service_url(&self) -> String {
        activity::service_url(&self.url)
    }

    /// A Teams service at an address no bot may trust.
    pub fn untrusted_url(&self) -> String {
        format!("{}/ailleurs/", self.url)
    }

    /// Every request answers 503 while `down`.
    pub fn unavailable(&self, down: bool) {
        self.lock().down = down;
    }

    /// The Bot Framework's configuration is answered `delay` late from now
    /// on.
    pub fn slow(&self, delay: Duration) {
        self.lock().delay = delay;
    }

    /// The test key is endorsed for `channels` only from now on.
    pub fn endorse(&self, channels: &[&str]) {
        self.lock().endorsements = channels.iter().map(|c| (*c).to_owned()).collect();
    }

    /// A person of the test directory, `aad` (their Entra object), named
    /// `name`, whose address is `email`: Teams says so of their member
    /// `29:<aad>`.
    pub fn add_person(&self, aad: &str, name: &str, email: &str) {
        self.set_member(
            aad,
            json!({
                "id": member_id(aad),
                "name": name,
                "aadObjectId": aad,
                "email": email,
                "userPrincipalName": email,
                "tenantId": TEAMS_DIRECTORY,
                "userRole": "user",
            }),
        );
    }

    /// What Teams says of the member of `aad`, from now on.
    pub fn set_member(&self, aad: &str, profile: Value) {
        self.lock().members.insert(member_id(aad), profile);
    }

    /// The person `aad`'s conversation refuses the bot from now on.
    pub fn block(&self, aad: &str) {
        self.lock().blocked.insert(member_id(aad));
    }

    /// The messages the bots wrote, oldest first.
    pub fn sent(&self) -> Vec<TeamsSent> {
        self.lock().sent.clone()
    }

    /// How many tokens were asked of Entra.
    pub fn token_requests(&self) -> usize {
        self.lock().token_requests
    }

    /// How many times the key set was read.
    pub fn key_reads(&self) -> usize {
        self.lock().key_reads
    }

    /// How many times the Bot Framework's configuration was asked for,
    /// answered or not.
    pub fn configuration_requests(&self) -> usize {
        self.lock().configuration_requests
    }

    /// The claims of a token the Bot Framework sends the bot `app` with an
    /// activity of its service, valid now.
    pub fn claims(&self, app: &str) -> Value {
        activity::claims(&self.service_url(), app)
    }

    /// The token the Bot Framework sends the bot `app` with an activity.
    pub fn token(&self, app: &str) -> String {
        signed_token(Signer::BotFramework, &self.claims(app))
    }

    /// What the Bot Framework sends the bot `app` of the test directory
    /// when the person `aad`, named `name`, writes `text` in their personal
    /// conversation with it, as its activity `id`.
    pub fn message(&self, app: &str, id: &str, person: (&str, &str), text: &str) -> Value {
        activity::message(&self.service_url(), app, id, person, text)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().expect("lock")
    }
}

/// A person's member identifier for the bots: `29:<aad>`.
pub fn member_id(aad: &str) -> String {
    format!("29:{aad}")
}

/// A person's personal conversation with the bots.
pub fn conversation_of(aad: &str) -> String {
    format!("a:conversation-{aad}")
}
