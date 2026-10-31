//! The history a conversation's new run is given (ADR-145): its summary, as
//! data, then the latest messages the summary does not cover, as plain text
//! turns (tool exchanges stay in their runs). Without a summary, the latest
//! messages alone.

use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::summary_message;
use mimorya_db::TenantTx;
use mimorya_db::repositories::conversation_summaries::{
    SummaryRow, conversation_summary, latest_messages_after,
};
use mimorya_db::repositories::conversations::MessageRow;
use mimorya_db::repositories::runs::RunRow;
use uuid::Uuid;

/// Messages replayed word for word at most: what the summary does not
/// cover yet, should it lag.
const HISTORY_LIMIT: i64 = 20;

/// The history of a turn opened now in `conversation_id`.
pub async fn conversation_history(
    tx: &mut TenantTx,
    conversation_id: Uuid,
) -> anyhow::Result<Vec<LlmMessage>> {
    let summary = conversation_summary(tx, conversation_id).await?;
    let through = summary.as_ref().map(|s| s.through_message_id);
    let messages = latest_messages_after(tx, conversation_id, through, HISTORY_LIMIT).await?;
    Ok(history_of(summary, messages))
}

/// What a queued turn's run is first given (ADR-085): its conversation's
/// history before its request, and that request, named on the run. `None`
/// when the request is no longer there.
pub async fn queued_turn_history(
    tx: &mut TenantTx,
    run: &RunRow,
) -> anyhow::Result<Option<(Vec<LlmMessage>, String)>> {
    let (Some(conversation_id), Some(request_id)) = (run.conversation_id, run.request_message_id)
    else {
        return Ok(None);
    };
    // The turn added its request last: nothing else is added until its run
    // runs, and a summary never covers the newest message.
    let summary = conversation_summary(tx, conversation_id).await?;
    let through = summary.as_ref().map(|s| s.through_message_id);
    let mut messages =
        latest_messages_after(tx, conversation_id, through, HISTORY_LIMIT + 1).await?;
    let Some(at) = messages.iter().position(|m| m.id == request_id) else {
        return Ok(None);
    };
    let request = messages.remove(at);
    Ok(Some((history_of(summary, messages), request.content)))
}

fn history_of(summary: Option<SummaryRow>, messages: Vec<MessageRow>) -> Vec<LlmMessage> {
    let summary = summary.map(|s| summary_message(&s.content));
    let turns = messages
        .into_iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| {
            if m.role == "user" {
                LlmMessage::user(m.content)
            } else {
                LlmMessage::assistant(m.content)
            }
        });
    summary.into_iter().chain(turns).collect()
}
