//! The lesson drawn from a finished run (ADR-076), in the background once
//! the answer is given: when the company chose it, a run that used tools is
//! reread by the model that served it (the company's own, else the
//! platform's), and at most one lesson is proposed in the personal memory of
//! whom it served, who alone sees it and validates it. A run whose company
//! did not choose it is reread by nobody. A failure loses the lesson, never
//! the run; what the rereading cost is recorded like the run's.

use std::sync::Arc;

use mimorya_agent::llm::PriceTable;
use mimorya_agent::runtime::{MAX_KNOWN_LESSONS, RunOutcome, lesson_transcript, propose_lesson};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use mimorya_db::repositories::memory_settings::{
    captures_lessons, personal_lesson_titled, personal_lesson_titles,
};
use uuid::Uuid;

use super::background_usage::{BackgroundCall, record_background_usage};
use super::db_ports::RunScope;
use crate::services::background::BackgroundTasks;
use crate::services::llm::LlmService;

#[derive(Clone)]
pub struct LessonCapture {
    db: Database,
    llm: Arc<LlmService>,
    prices: Arc<PriceTable>,
    background: Arc<BackgroundTasks>,
}

impl LessonCapture {
    pub fn new(db: Database, llm: Arc<LlmService>, background: Arc<BackgroundTasks>) -> Self {
        Self {
            prices: llm.prices().clone(),
            db,
            llm,
            background,
        }
    }

    /// Rereads a run that completed having used tools, in the background.
    pub fn after_run(&self, scope: RunScope, outcome: &RunOutcome) {
        let RunOutcome::Completed { .. } = outcome else {
            return;
        };
        let Some(transcript) = lesson_transcript(&outcome.checkpoint().messages) else {
            return;
        };
        let capture = self.clone();
        self.background.spawn(async move {
            if let Err(error) = capture.capture(scope, &transcript).await {
                tracing::warn!(run_id = %scope.run_id, error = format!("{error:#}"), "Lesson not drawn");
            }
        });
    }

    /// The lesson proposed, if any.
    pub async fn capture(&self, scope: RunScope, transcript: &str) -> anyhow::Result<Option<Uuid>> {
        let mut tx = self.db.begin(scope.tenant_id).await?;
        if !captures_lessons(&mut tx).await? {
            return Ok(None);
        }
        let limit = i64::try_from(MAX_KNOWN_LESSONS)?;
        let known = personal_lesson_titles(&mut tx, scope.user_id, limit).await?;
        tx.commit().await?;
        // The model of the company's runs: a company that cannot use its own
        // has none reread either.
        let Ok(Some(provider)) = self.llm.provider_for(scope.tenant_id).await else {
            return Ok(None);
        };
        let answer = propose_lesson(provider.as_ref(), transcript, &known).await?;
        // Counted on its own: what the call cost stays, whatever follows.
        let call = BackgroundCall {
            provider: &answer.provider,
            model: &answer.model,
            usage: &answer.usage,
        };
        record_background_usage(&self.db, &self.prices, scope, call).await?;
        let Some(lesson) = answer.lesson else {
            return Ok(None);
        };
        let mut tx = self.db.begin(scope.tenant_id).await?;
        if personal_lesson_titled(&mut tx, scope.user_id, &lesson.title).await? {
            return Ok(None);
        }
        let id = create_memory(
            &mut tx,
            NewMemory {
                tenant_id: scope.tenant_id,
                scope: MemoryScope::User,
                owner_user_id: Some(scope.user_id),
                org_unit_id: None,
                kind: MemoryKind::Lesson,
                title: lesson.title,
                content: lesson.content,
                sensitivity: Sensitivity::Internal,
                status: MemoryStatus::Proposed,
                source: MemorySource::run(scope.run_id),
                created_by_user_id: Some(scope.user_id),
                created_by_run_id: Some(scope.run_id),
            },
        )
        .await?;
        // Its text stays in the memory, never in the audit.
        let entry = AuditEntry::new(
            scope.tenant_id,
            "agent",
            "memory.lesson_proposed",
            "success",
        )
        .on_behalf_of(scope.user_id)
        .run(scope.run_id)
        .resource("memory", id);
        append_audit(&mut tx, entry).await?;
        // Indexed by meaning once approved, like any proposal.
        tx.commit().await?;
        Ok(Some(id))
    }
}
