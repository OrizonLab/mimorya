//! The fake Slack's methods, as Slack answers them, and what the end-to-end
//! tests read and set there (`/test/…`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{
    BOT_TOKEN, BOT_USER, OTHER_TOKEN, OTHER_WORKSPACE, Shared, SlackPost, WORKSPACE, WORKSPACE_NAME,
};

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

pub(super) async fn auth_test(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
) -> Response {
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
    let mut answer = Json(json!({
        "ok": true,
        "url": "https://neo-maintenance.slack.com/",
        "team": team,
        "user": "mimorya",
        "team_id": team_id,
        "user_id": user_id,
        "bot_id": "B0MIMORYA",
        "is_enterprise_install": false
    }))
    .into_response();
    if let Some(scopes) = &shared.scopes
        && let Ok(scopes) = HeaderValue::from_str(&scopes.join(","))
    {
        answer.headers_mut().insert("x-oauth-scopes", scopes);
    }
    answer
}

pub(super) async fn users_info(
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

/// A direct conversation, `D…`, is the person's of the same name, `U…`;
/// any other is no direct one.
pub(super) async fn conversation_info(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let shared = shared.lock().expect("lock");
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    if let Some(code) = &shared.conversation_refusal {
        return Json(json!({ "ok": false, "error": code })).into_response();
    }
    let id = query.get("channel").cloned().unwrap_or_default();
    let mut channel = json!({ "id": id, "is_im": false });
    if let Some(rest) = id.strip_prefix('D') {
        channel["is_im"] = json!(true);
        channel["user"] = json!(format!("U{rest}"));
    }
    Json(json!({ "ok": true, "channel": channel })).into_response()
}

pub(super) async fn post_message(
    State(shared): State<Arc<Mutex<Shared>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut shared = shared.lock().expect("lock");
    shared.post_attempts += 1;
    if let Some(refusal) = refused(&shared, &headers) {
        return refusal;
    }
    if let Some(code) = &shared.post_refusal {
        return Json(json!({ "ok": false, "error": code })).into_response();
    }
    let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
    let (Some(channel), Some(text)) = (field("channel"), field("text")) else {
        return Json(json!({ "ok": false, "error": "invalid_arguments" })).into_response();
    };
    if shared.blocks_refused && body.get("blocks").is_some() {
        return Json(json!({ "ok": false, "error": "invalid_blocks" })).into_response();
    }
    // A person's id names their direct conversation with the application.
    let channel = match channel.strip_prefix(['U', 'W']) {
        Some(rest) => format!("D{rest}"),
        None => channel,
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

pub(super) async fn update_message(
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
pub(super) async fn posts(State(shared): State<Arc<Mutex<Shared>>>) -> Json<Value> {
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

/// The permissions Slack says the token has from now on, for the
/// end-to-end tests: `{"scopes": [...]}`, or `null` for none said.
pub(super) async fn grant_scopes(
    State(shared): State<Arc<Mutex<Shared>>>,
    Json(body): Json<Value>,
) -> StatusCode {
    let scopes = body.get("scopes").and_then(Value::as_array).map(|scopes| {
        (scopes.iter())
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    });
    shared.lock().expect("lock").scopes = scopes;
    StatusCode::NO_CONTENT
}
