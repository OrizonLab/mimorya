//! A run waiting in the queue for a worker (ADR-085), for the end-to-end
//! tests to see it so (`queued-run.spec.ts`): Marc's request, queued as an
//! `api` process queues it. A lock on the run, held from the start, keeps
//! the server's queue from claiming it until Marc's browser has followed
//! it for a while; then the queue claims it, and the model answers it apart
//! (`WithQueued`), so that the reference scenarios keep their order.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mimorya_agent::llm::{
    LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmUsage, StopReason,
};
use mimorya_agent::runtime::{DEFAULT_BUDGET, RunEvent};
use mimorya_api::app::App;
use mimorya_api::contract::MessageMetadata;
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
use mimorya_db::notifications::notify_run_queued;
use mimorya_db::repositories::conversations::{NewMessage, add_message, create_conversation};
use mimorya_db::repositories::run_events::append_run_event;
use mimorya_db::repositories::run_queue::queue_run;
use mimorya_db::repositories::runs::{NewRun, create_run};

/// The queued conversation's title, which the test opens.
const TITLE: &str = "Demande en file";
const REQUEST: &str = "Relève les comptes rendus restés sans réponse";
/// The answer, once a worker executed it, which the test waits for.
const ANSWER: &str = "Deux comptes rendus attendent encore une réponse.";
/// How long Marc's browser follows the run before it is claimed.
const FOLLOWED_FOR: Duration = Duration::from_secs(5);

/// Leaves Marc's run queued, and holds it until followed for a while.
pub async fn leave(app: &App) -> anyhow::Result<()> {
    let db = &app.services.db;
    let mut tx = db.begin(tenant::ID).await?;
    let conversation = create_conversation(&mut tx, tenant::ID, user::MARC, Some(TITLE)).await?;
    let request = add_message(
        &mut tx,
        NewMessage {
            tenant_id: tenant::ID,
            conversation_id: conversation.id,
            role: "user",
            content: REQUEST.into(),
            author_user_id: Some(user::MARC),
            run_id: None,
            metadata: serde_json::to_value(MessageMetadata::default())?,
        },
    )
    .await?;
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id: tenant::ID,
            conversation_id: Some(conversation.id),
            initiated_by_user_id: user::MARC,
            on_behalf_of_user_id: user::MARC,
            objective: REQUEST.into(),
            success_criteria: Vec::new(),
            autonomy_level: AutonomyLevel::ExecuteWithApproval,
            budget: serde_json::to_value(DEFAULT_BUDGET)?,
            request_message_id: Some(request.id),
            mission_id: None,
            channel: "web",
        },
    )
    .await?;
    anyhow::ensure!(queue_run(&mut tx, run.id).await?, "run not queued");
    let (kind, data) = RunEvent::RunQueued {}.into_parts();
    append_run_event(&mut tx, tenant::ID, run.id, &kind, &data).await?;
    tx.commit().await?;

    // Locked before anything claims it: the queue skips it meanwhile.
    let mut held = db.begin(tenant::ID).await?;
    sqlx::query("select id from agent_runs where id = $1 for share")
        .bind(run.id)
        .execute(&mut *held)
        .await?;
    let services = app.services.clone();
    tokio::spawn(async move {
        while !services.bus.follows(run.id) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tokio::time::sleep(FOLLOWED_FOR).await;
        let released = held.commit().await;
        let announced = notify_run_queued(services.db.pool()).await;
        if let Err(error) = released.and(announced) {
            tracing::warn!(error = %error, "Queued run not released");
        }
    });
    Ok(())
}

/// The platform model: the queued request gets its answer, the rest goes
/// to `others`.
pub struct WithQueued {
    others: Arc<dyn LlmProvider>,
}

impl WithQueued {
    pub fn new(others: Arc<dyn LlmProvider>) -> Self {
        Self { others }
    }
}

#[async_trait]
impl LlmProvider for WithQueued {
    fn id(&self) -> &str {
        self.others.id()
    }

    fn model(&self) -> &str {
        self.others.model()
    }

    async fn complete(&self, request: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let queued = request.messages.iter().any(
            |message| matches!(message, LlmMessage::User { content } if content.contains(REQUEST)),
        );
        if !queued {
            return self.others.complete(request).await;
        }
        Ok(LlmResponse {
            text: ANSWER.into(),
            tool_calls: Vec::new(),
            stop_reason: StopReason::EndTurn,
            usage: LlmUsage {
                input_tokens: 100,
                output_tokens: 20,
                ..LlmUsage::default()
            },
            provider: self.others.id().into(),
            model: self.others.model().into(),
            native: None,
        })
    }
}
