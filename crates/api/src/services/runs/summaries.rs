//! A conversation's summary kept up (ADR-145), in the background once a run
//! of it ends: when the messages its summary does not cover yet grow past
//! what is replayed word for word, the oldest are folded into it by the
//! company's model (none for a company without one), for whom may still use
//! the agent (ADR-035). Passes are chained while it lags, a few at most
//! (`MAX_PASSES`, ADR-149), each its own model call, read again and checked
//! again; the rest waits for the next run's end. A failure loses that pass
//! and stops the others, never the run: the history then replays the latest
//! messages as before. What the folding cost is recorded like the run's;
//! its text stays out of the audit.

use std::sync::Arc;

use mimorya_agent::llm::PriceTable;
use mimorya_agent::runtime::{FoldedMessage, RunOutcome, summarise_conversation};
use mimorya_core::conversation_summary::{MAX_PASSES, Speaker, Unfolded, messages_to_fold};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::conversation_summaries::{
    FoldedSummary, conversation_summary, earliest_messages_after, store_summary,
};
use mimorya_db::repositories::conversations::MessageRow;
use serde_json::json;
use uuid::Uuid;

use super::background_usage::{BackgroundCall, record_background_usage};
use super::db_ports::RunScope;
use super::run_access::stopped_by;
use crate::services::access::AccessService;
use crate::services::background::BackgroundTasks;
use crate::services::llm::LlmService;

/// Messages not covered yet that one pass reads at most: more than it folds
/// and keeps, the rest waiting for the next pass.
const UNFOLDED_READ: i64 = 200;

#[derive(Clone)]
pub struct ConversationSummaries {
    db: Database,
    /// Whom the pass is for may still use the agent (ADR-035).
    access: Arc<AccessService>,
    llm: Arc<LlmService>,
    prices: Arc<PriceTable>,
    background: Arc<BackgroundTasks>,
}

impl ConversationSummaries {
    pub fn new(
        db: Database,
        access: Arc<AccessService>,
        llm: Arc<LlmService>,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        Self {
            prices: llm.prices().clone(),
            db,
            access,
            llm,
            background,
        }
    }

    /// Folds the conversation of a run that ended, in the background. A
    /// paused run has not answered yet: its end will.
    pub fn after_run(&self, scope: RunScope, outcome: &RunOutcome) {
        let Some(conversation_id) = scope.conversation_id else {
            return;
        };
        if let RunOutcome::AwaitingApproval { .. } = outcome {
            return;
        }
        let summaries = self.clone();
        self.background.spawn(async move {
            if let Err(error) = summaries.catch_up(scope, conversation_id).await {
                tracing::warn!(
                    run_id = %scope.run_id,
                    error = format!("{error:#}"),
                    "Conversation not summarised"
                );
            }
        });
    }

    /// Folds the conversation pass after pass while its summary lags, at
    /// most `MAX_PASSES`: how many summaries were stored. Stops at the first
    /// pass that stores none (caught up, no longer allowed, or another pass
    /// first), or fails.
    pub async fn catch_up(&self, scope: RunScope, conversation_id: Uuid) -> anyhow::Result<usize> {
        let mut stored = 0;
        while stored < MAX_PASSES && self.fold(scope, conversation_id).await? {
            stored += 1;
        }
        Ok(stored)
    }

    /// Whether a summary was stored: none while nothing needs folding, nor
    /// when another pass stored one first.
    pub async fn fold(&self, scope: RunScope, conversation_id: Uuid) -> anyhow::Result<bool> {
        let mut tx = self.db.begin(scope.tenant_id).await?;
        let summary = conversation_summary(&mut tx, conversation_id).await?;
        let from = summary.as_ref().map(|s| s.through_message_id);
        let unfolded =
            earliest_messages_after(&mut tx, conversation_id, from, UNFOLDED_READ).await?;
        tx.commit().await?;
        let weighed: Vec<Unfolded> = unfolded
            .iter()
            .map(|m| Unfolded {
                speaker: speaker(m),
                chars: m.content.chars().count(),
            })
            .collect();
        let count = messages_to_fold(&weighed);
        let Some(last) = count.checked_sub(1).map(|i| &unfolded[i]) else {
            return Ok(false);
        };
        // As each step of a run: only for whom may still use the agent, in a
        // company still active.
        let context = self
            .access
            .active_context(scope.tenant_id, scope.user_id)
            .await?;
        if stopped_by(context.as_ref()).is_some() {
            return Ok(false);
        }
        // The model of the company's runs: a company that cannot use its own
        // has none summarised either.
        let Ok(Some(provider)) = self.llm.provider_for(scope.tenant_id).await else {
            return Ok(false);
        };
        let folded: Vec<FoldedMessage<'_>> = unfolded[..count]
            .iter()
            .map(|m| FoldedMessage {
                speaker: speaker(m),
                content: &m.content,
            })
            .collect();
        let previous = summary.as_ref().map(|s| s.content.as_str());
        let answer = summarise_conversation(provider.as_ref(), previous, &folded).await?;
        // Counted on its own: what the call cost stays, whatever follows.
        let call = BackgroundCall {
            provider: &answer.provider,
            model: &answer.model,
            usage: &answer.usage,
        };
        record_background_usage(&self.db, &self.prices, scope, call).await?;
        let Some(content) = answer.summary else {
            return Ok(false);
        };
        let mut tx = self.db.begin(scope.tenant_id).await?;
        let folded = i32::try_from(count)?;
        let stored = store_summary(
            &mut tx,
            FoldedSummary {
                tenant_id: scope.tenant_id,
                conversation_id,
                content,
                through_message_id: last.id,
                folded,
            },
            from,
        )
        .await?;
        if stored {
            // Its text stays in the summary, never in the audit.
            let entry = AuditEntry::new(
                scope.tenant_id,
                "agent",
                "conversation.summarised",
                "success",
            )
            .on_behalf_of(scope.user_id)
            .run(scope.run_id)
            .resource("conversation", conversation_id)
            .metadata(json!({ "foldedMessages": folded }));
            append_audit(&mut tx, entry).await?;
        }
        tx.commit().await?;
        Ok(stored)
    }
}

fn speaker(message: &MessageRow) -> Speaker {
    if message.role == "user" {
        Speaker::Person
    } else {
        Speaker::Assistant
    }
}
