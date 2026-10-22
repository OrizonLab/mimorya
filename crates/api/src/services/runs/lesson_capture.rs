//! The lesson drawn from a finished run (ADR-076), in the background once
//! the answer is given: when the company chose it, a run that used tools is
//! reread by the model that served it (the company's own, else the
//! platform's), and at most one lesson is proposed in the personal memory of
//! whom it served, who alone sees it and validates it. A run whose company
//! did not choose it is reread by nobody. A failure loses the lesson, never
//! the run; what the rereading cost is recorded like the run's.

use std::sync::Arc;

use mimorya_agent::llm::PriceTable;
use mimorya_agent::runtime::{LessonAnswer, RunOutcome, lesson_transcript, propose_lesson};
use mimorya_core::authz::Sensitivity;
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use mimorya_db::repositories::memory_settings::{captures_lessons, personal_lesson_titles};
use mimorya_db::repositories::usage::{NewUsage, append_usage};
use uuid::Uuid;

use super::db_ports::RunScope;
use crate::services::background::BackgroundTasks;
use crate::services::llm::LlmService;

/// Lessons of the person's named to the model, so that none comes twice.
const KNOWN_LESSONS: i64 = 30;

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
        let known = personal_lesson_titles(&mut tx, scope.user_id, KNOWN_LESSONS).await?;
        tx.commit().await?;
        // The model of the company's runs: a company that cannot use its own
        // has none reread either.
        let Ok(Some(provider)) = self.llm.provider_for(scope.tenant_id).await else {
            return Ok(None);
        };
        let answer = propose_lesson(provider.as_ref(), transcript, &known).await?;
        let mut tx = self.db.begin(scope.tenant_id).await?;
        self.record_usage(&mut tx, scope, &answer).await?;
        let lesson = answer.lesson.filter(|lesson| {
            !known
                .iter()
                .any(|title| title.to_lowercase() == lesson.title.to_lowercase())
        });
        let Some(lesson) = lesson else {
            tx.commit().await?;
            return Ok(None);
        };
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

    async fn record_usage(
        &self,
        tx: &mut mimorya_db::TenantTx,
        scope: RunScope,
        answer: &LessonAnswer,
    ) -> anyhow::Result<()> {
        let usage = &answer.usage;
        append_usage(
            tx,
            NewUsage {
                tenant_id: scope.tenant_id,
                user_id: Some(scope.user_id),
                run_id: Some(scope.run_id),
                kind: "llm",
                provider: answer.provider.clone(),
                model: answer.model.clone(),
                // Counted as runs count them: cached tokens are input too.
                input_tokens: i32::try_from(usage.input_total()).unwrap_or(i32::MAX),
                output_tokens: i32::try_from(usage.output_tokens).unwrap_or(i32::MAX),
                cost_micros: self.prices.cost_micros(&answer.model, usage).unwrap_or(0),
            },
        )
        .await?;
        Ok(())
    }
}
