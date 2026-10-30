//! What the test Microsoft answers, as Microsoft does (ADR-138): a token
//! from Entra for a known application with its own secret (client
//! credentials), `invalid_client` otherwise; the Bot Framework's OpenID
//! configuration (as late as a test asks) and key set; a message written
//! in a conversation, and who a member is, with a token Entra gave only.

use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use mimorya_core::gateways::teams::BOT_FRAMEWORK_SCOPE;
use serde_json::{Value, json};
use url::form_urlencoded;

use super::signing::key_set;
use super::{
    OTHER_TEAMS_APP, OTHER_TEAMS_DIRECTORY, OTHER_TEAMS_SECRET, Shared, TEAMS_APP, TEAMS_DIRECTORY,
    TEAMS_SECRET, TeamsSent,
};

type SharedState = State<Arc<Mutex<Shared>>>;

fn error(status: StatusCode, code: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": code } })),
    )
        .into_response()
}

pub(super) async fn token(
    State(shared): SharedState,
    Path(directory): Path<String>,
    body: String,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    shared.token_requests += 1;
    let form: Vec<(String, String)> = form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| {
        (form.iter())
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    let known = [
        (TEAMS_APP, TEAMS_DIRECTORY, TEAMS_SECRET),
        (OTHER_TEAMS_APP, OTHER_TEAMS_DIRECTORY, OTHER_TEAMS_SECRET),
    ];
    let app = known.iter().find(|(app, of, secret)| {
        field("client_id") == Some(app)
            && directory == *of
            && field("client_secret") == Some(secret)
    });
    let granted = field("grant_type") == Some("client_credentials")
        && field("scope") == Some(BOT_FRAMEWORK_SCOPE);
    let Some((app, _, _)) = app.filter(|_| granted) else {
        let refusal = json!({
            "error": "invalid_client",
            "error_description": "AADSTS7000215: Invalid client secret provided.",
        });
        return (StatusCode::UNAUTHORIZED, Json(refusal)).into_response();
    };
    let token = format!("jeton-teams-{}", shared.token_requests);
    shared.tokens.insert(token.clone(), (*app).to_owned());
    Json(json!({ "token_type": "Bearer", "expires_in": 3599, "access_token": token }))
        .into_response()
}

pub(super) async fn openid(State(shared): SharedState) -> Response {
    let delay = {
        let mut shared = shared.lock().expect("lock");
        shared.configuration_requests += 1;
        shared.delay
    };
    tokio::time::sleep(delay).await;
    let shared = shared.lock().expect("lock");
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    Json(json!({
        "issuer": "https://api.botframework.com",
        "jwks_uri": format!("{}/botframework/keys", shared.base),
        "id_token_signing_alg_values_supported": ["RS256"],
    }))
    .into_response()
}

pub(super) async fn keys(State(shared): SharedState) -> Response {
    let mut shared = shared.lock().expect("lock");
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    shared.key_reads += 1;
    Json(key_set(&shared.endorsements)).into_response()
}

/// The application whose token `headers` carry, if Entra gave it.
fn bearer_app(shared: &Shared, headers: &HeaderMap) -> Option<String> {
    let header = headers.get("authorization")?.to_str().ok()?;
    let token = header.strip_prefix("Bearer ")?;
    shared.tokens.get(token).cloned()
}

pub(super) async fn send(
    State(shared): SharedState,
    Path(conversation): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    Json(activity): Json<Value>,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    // The service written to: the path before `v3/`.
    let region = uri.path().split("v3/").next().unwrap_or_default();
    let service = format!("{}{region}", shared.base);
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(app) = bearer_app(&shared, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "Unauthorized");
    };
    let member = conversation
        .strip_prefix("a:conversation-")
        .map(|aad| format!("29:{aad}"));
    if member.is_some_and(|member| shared.blocked.contains(&member)) {
        return error(StatusCode::FORBIDDEN, "BotDisabledByAdmin");
    }
    let text = activity.get("text").and_then(Value::as_str);
    let Some(text) =
        text.filter(|_| activity.get("type").and_then(Value::as_str) == Some("message"))
    else {
        return error(StatusCode::BAD_REQUEST, "BadArgument");
    };
    let text_format = (activity.get("textFormat").and_then(Value::as_str)).map(str::to_owned);
    shared.sent.push(TeamsSent {
        app,
        service,
        conversation,
        text: text.to_owned(),
        text_format,
    });
    let id = format!("envoi-{}", shared.sent.len());
    (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
}

pub(super) async fn member(
    State(shared): SharedState,
    Path((conversation, member)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let shared = shared.lock().expect("lock");
    if shared.down {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if bearer_app(&shared, &headers).is_none() {
        return error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    // A personal conversation's member is the one it is with.
    let theirs = conversation
        .strip_prefix("a:conversation-")
        .map(|aad| format!("29:{aad}"));
    match shared.members.get(&member) {
        Some(profile) if theirs.as_deref() == Some(member.as_str()) => {
            Json(profile.clone()).into_response()
        }
        _ => error(StatusCode::NOT_FOUND, "MemberNotFoundInConversation"),
    }
}
