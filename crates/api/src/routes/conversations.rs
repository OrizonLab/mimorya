//! Conversations of the signed-in user and the messages that start agent runs.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::repositories::conversation_summaries::{
    conversation_summary, count_messages_after,
};
use mimorya_db::repositories::conversations::{
    ConversationPatch, ConversationRow, create_conversation, get_conversation, list_conversations,
    list_messages, update_conversation,
};
use mimorya_db::repositories::gateway_threads::conversations_held;
use mimorya_db::repositories::runs::list_runs_for_conversation;

use crate::app::AppState;
use crate::contract::{
    ConversationChanges, ConversationCreated, ConversationDetail, ConversationList,
    ConversationSummary, Done, HistorySummary, Message, MessageAccepted, NewConversation,
    NewMessage,
};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::AuthIdentity;
use crate::services::runs::TurnSender;

const MESSAGE_PAGE: i64 = 200;
const RUN_PAGE: i64 = 20;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/conversations", get(list).post(create))
        .route("/conversations/{id}", get(detail).patch(update))
        .route("/conversations/{id}/messages", post(post_message))
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<ConversationList>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_conversations(&mut tx, identity.user_id).await?;
    let ids: Vec<_> = rows.iter().map(|c| c.id).collect();
    let mut held = conversations_held(&mut tx, &ids).await?;
    tx.commit().await?;
    Ok(Json(ConversationList {
        conversations: rows
            .iter()
            .map(|c| ConversationSummary::held(c, held.remove(&c.id)))
            .collect(),
    }))
}

async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewConversation>,
) -> ApiResult<(StatusCode, Json<ConversationCreated>)> {
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
        Json(ConversationCreated {
            conversation: (&row).into(),
        }),
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
) -> ApiResult<Json<ConversationDetail>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let conversation = owned(&mut tx, identity, id).await?;
    let messages = list_messages(&mut tx, id, MESSAGE_PAGE).await?;
    let runs = list_runs_for_conversation(&mut tx, id, RUN_PAGE).await?;
    let gateway = conversations_held(&mut tx, &[id]).await?.remove(&id);
    let summary = match conversation_summary(&mut tx, id).await? {
        Some(summary) => {
            let after = Some(summary.through_message_id);
            let count = count_messages_after(&mut tx, id, after).await?;
            Some(HistorySummary::of(&summary, count))
        }
        None => None,
    };
    tx.commit().await?;
    Ok(Json(ConversationDetail {
        conversation: ConversationSummary::held(&conversation, gateway),
        messages: messages
            .iter()
            .map(Message::from_row)
            .collect::<anyhow::Result<_>>()?,
        runs: runs.iter().map(Into::into).collect(),
        history_summary: summary,
    }))
}

async fn update(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<ConversationChanges>,
) -> ApiResult<Json<Done>> {
    let mut tx = state.db.begin(identity.tenant_id).await?;
    owned(&mut tx, identity, id).await?;
    let patch = ConversationPatch {
        title: body.title,
        archived_at: body.archived.map(|archived| archived.then(Utc::now)),
    };
    update_conversation(&mut tx, id, patch).await?;
    tx.commit().await?;
    Ok(Json(Done::OK))
}

async fn post_message(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<NewMessage>,
) -> ApiResult<(StatusCode, Json<MessageAccepted>)> {
    let started = state
        .runs
        .start_conversation_turn(identity, id, &body.content, TurnSender::User)
        .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(MessageAccepted {
            run_id: started.run_id,
            message_id: started.message_id,
        }),
    ))
}
