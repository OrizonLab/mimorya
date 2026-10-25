//! What a case played for an evaluation records (ADR-104): the tools it
//! called, kept with its result, and what the audit and the company's
//! consumption say of it, in the name of whom it ran for. It is no run:
//! no event streamed, no step saved, no journal of calls.

use std::sync::Mutex;

use async_trait::async_trait;
use mimorya_agent::runtime::{
    AuditSink, EngineAuditEntry, RunCheckpoint, RunEvent, RunEventSink, RunOutcome, RunStore,
    ToolInvocationLog, ToolInvocationRecord, UsageRecord, UsageSink,
};
use mimorya_core::skill_evaluations::{MAX_TOOL_SUMMARY_CHARS, MAX_TOOLS_KEPT, bounded};
use mimorya_db::Database;
use mimorya_db::repositories::{audit, usage};
use serde::Serialize;
use serde_json::{Value, json};

use super::CaseScope;

/// A tool the case called, and what came of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTrace {
    pub name: String,
    pub summary: String,
    pub is_error: bool,
}

pub struct Journal {
    db: Database,
    scope: CaseScope,
    tools: Mutex<Vec<ToolTrace>>,
}

impl Journal {
    pub fn new(db: Database, scope: CaseScope) -> Self {
        Self {
            db,
            scope,
            tools: Mutex::new(Vec::new()),
        }
    }

    /// The tools called, in order, at most `MAX_TOOLS_KEPT`.
    pub fn tools(&self) -> Vec<ToolTrace> {
        self.tools.lock().map(|t| t.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl RunEventSink for Journal {
    async fn emit(&self, event: RunEvent) -> anyhow::Result<()> {
        if let RunEvent::ToolResult {
            tool_name,
            summary,
            is_error,
            ..
        } = event
            && let Ok(mut tools) = self.tools.lock()
            && tools.len() < MAX_TOOLS_KEPT
        {
            tools.push(ToolTrace {
                name: tool_name,
                summary: bounded(&summary, MAX_TOOL_SUMMARY_CHARS),
                is_error,
            });
        }
        Ok(())
    }

    fn publish_delta(&self, _text: &str) {}
}

#[async_trait]
impl AuditSink for Journal {
    async fn record(&self, entry: EngineAuditEntry) -> anyhow::Result<()> {
        let evaluation = json!(self.scope.evaluation_id);
        let metadata = match entry.metadata {
            Value::Object(mut map) => {
                map.insert("evaluationId".into(), evaluation);
                Value::Object(map)
            }
            _ => json!({ "evaluationId": evaluation }),
        };
        let mut record =
            audit::AuditEntry::new(self.scope.tenant_id, "agent", entry.action, entry.outcome)
                .on_behalf_of(self.scope.user_id)
                .metadata(metadata);
        record.resource_type = entry.resource_type;
        record.resource_id = entry.resource_id;
        record.reason = entry.reason;
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        audit::append_audit(&mut tx, record).await?;
        Ok(tx.commit().await?)
    }
}

#[async_trait]
impl UsageSink for Journal {
    async fn record(&self, record: UsageRecord) -> anyhow::Result<()> {
        let mut tx = self.db.begin(self.scope.tenant_id).await?;
        usage::append_usage(
            &mut tx,
            usage::NewUsage {
                tenant_id: self.scope.tenant_id,
                user_id: Some(self.scope.user_id),
                run_id: None,
                kind: record.kind,
                provider: record.provider,
                model: record.model,
                input_tokens: i32::try_from(record.input_tokens).unwrap_or(i32::MAX),
                output_tokens: i32::try_from(record.output_tokens).unwrap_or(i32::MAX),
                cost_micros: record.cost_micros,
            },
        )
        .await?;
        Ok(tx.commit().await?)
    }
}

/// The calls are kept with the case's result (`tools`).
#[async_trait]
impl ToolInvocationLog for Journal {
    async fn record(&self, _entry: ToolInvocationRecord) -> anyhow::Result<()> {
        Ok(())
    }
}

/// A case is played to its end at once: nothing to take up again.
#[async_trait]
impl RunStore for Journal {
    async fn checkpoint(&self, _state: &RunCheckpoint) -> anyhow::Result<()> {
        Ok(())
    }

    async fn finish(&self, _outcome: &RunOutcome) -> anyhow::Result<()> {
        Ok(())
    }
}
