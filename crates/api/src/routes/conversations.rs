//! Conversations of the signed-in user and the messages that start agent runs.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::repositories::conversations::{
    ConversationPatch, ConversationRow, create_conversation, get_conversation, list_conversations,
    list_messages, update_conversation,
};
use mimorya_db::repositories::runs::list_runs_for_conversation;
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Body, PathId, iso, trimmed, trimmed_opt};
use crate::plugins::Auth;
use crate::services::AuthIdentity;

const MESSAGE_PAGE: i64 = 200;
const RUN_PAGE: i64 = 20;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list).post(create))
        .route("/conversations/{id}", get(detail).patch(update))
        .route("/conversations/{id}/messages", post(post_message))
}

fn summary(c: &ConversationRow) -> Value {
    json!({ "id": c.id, "title": c.title, "updatedAt": iso(&c.updated_at) })
}

async fn list(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_conversations(&mut tx, identity.user_id).await?;
    tx.commit().await?;
    let conversations: Vec<Value> = rows.iter().map(summary).collect();
    Ok(Json(json!({ "conversations": conversations })))
}

#[derive(Deserialize, Validate)]
struct CreateBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    title: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<CreateBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    state.access.context(identity).await?.require("agent:use")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let row = create_conversation(
        &mut tx,
        identity.tenant_id,
        identity.user_id,
        body.title.as_deref(),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "conversation": summary(&row) })),
    ))
}

/// The conversation if it belongs to the caller; "not found" otherwise.
async fn owned(
    tx: &mut mimorya_db::TenantTx,
    identity: AuthIdentity,
    id: uuid::Uuid,
) -> ApiResult<ConversationRow> {
    get_conversation(tx, id)
        .await?
        .filter(|c| c.owner_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Conversation introuvable").into())
}

async fn detail(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let conversation = owned(&mut tx, identity, id).await?;
    let messages = list_messages(&mut tx, id, MESSAGE_PAGE).await?;
    let runs = list_runs_for_conversation(&mut tx, id, RUN_PAGE).await?;
    tx.commit().await?;
    let messages: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "role": m.role,
                "content": m.content,
                "runId": m.run_id,
                "metadata": m.metadata,
                "createdAt": iso(&m.created_at),
            })
        })
        .collect();
    let runs: Vec<Value> = runs
        .iter()
        .map(|r| json!({ "id": r.id, "status": r.status, "createdAt": iso(&r.created_at) }))
        .collect();
    Ok(Json(json!({
        "conversation": summary(&conversation),
        "messages": messages,
        "runs": runs,
    })))
}

#[derive(Deserialize, Validate)]
struct UpdateBody {
    #[serde(default, deserialize_with = "trimmed_opt")]
    #[validate(length(min = 1, max = 120))]
    title: Option<String>,
    #[serde(default)]
    archived: Option<bool>,
}

async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<UpdateBody>,
) -> ApiResult<Json<Value>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    owned(&mut tx, identity, id).await?;
    let patch = ConversationPatch {
        title: body.title,
        archived_at: body.archived.map(|archived| archived.then(Utc::now)),
    };
    update_conversation(&mut tx, id, patch).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Validate)]
struct MessageBody {
    #[serde(deserialize_with = "trimmed")]
    #[validate(length(min = 1, max = 8000))]
    content: String,
}

async fn post_message(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<MessageBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let started = state
        .runs
        .start_conversation_turn(identity, id, &body.content)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "runId": started.run_id, "messageId": started.message_id })),
    ))
}
