//! PostgreSQL-backed implementation of the engine ports for one run. Every
//! operation runs in its own tenant-bound transaction (RLS applies). Memory
//! and skills live in `knowledge_ports.rs`, deliverables in `deliverables.rs`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use mimorya_agent::runtime::{
    ApprovalRequest, ApprovalStore, AuditSink, EngineAuditEntry, EnginePorts, RunCheckpoint,
    RunEvent, RunEventSink, RunOutcome, RunStore, ToolInvocationLog, ToolInvocationRecord,
    UsageRecord, UsageSink,
};
use mimorya_db::repositories::{approvals, audit, runs, tool_invocations, usage};
use mimorya_db::{Database, TenantTx};
use serde_json::json;
use uuid::Uuid;

use super::deliverables::DbDeliverables;
use super::events::record_run_event;
use super::outcome::persist_outcome;
use crate::services::event_bus::{BusEvent, RunEventBus};

/// Approval requests left unanswered expire after a week.
const APPROVAL_VALIDITY_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy)]
pub struct RunScope {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    /// User on whose behalf the agent acts.
    pub user_id: Uuid,
    /// Conversation that receives the answer, if the run belongs to one.
    pub conversation_id: Option<Uuid>,
}

#[derive(Clone)]
pub struct DbPorts {
    db: Database,
    pub(super) scope: RunScope,
    bus: Arc<RunEventBus>,
}

pub fn engine_ports(db: Database, scope: RunScope, bus: Arc<RunEventBus>) -> EnginePorts {
    let ports = Arc::new(DbPorts {
        db: db.clone(),
        scope,
        bus,
    });
    EnginePorts {
        events: ports.clone(),
        audit: ports.clone(),
        tool_log: ports.clone(),
        approvals: ports.clone(),
        usage: ports.clone(),
        memory: ports.clone(),
        skills: ports.clone(),
        runs: ports,
        deliverables: Arc::new(DbDeliverables::new(db, scope)),
    }
}

impl DbPorts {
    pub(super) async fn tx(&self) -> anyhow::Result<TenantTx> {
        Ok(self.db.begin(self.scope.tenant_id).await?)
    }
}

fn saturating_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[async_trait]
impl RunEventSink for DbPorts {
    async fn emit(&self, event: RunEvent) -> anyhow::Result<()> {
        record_run_event(
            &self.db,
            &self.bus,
            self.scope.tenant_id,
            self.scope.run_id,
            event,
        )
        .await
    }

    fn publish_delta(&self, text: &str) {
        self.bus.publish(
            self.scope.run_id,
            BusEvent {
                seq: None,
                event_type: "assistant_delta".into(),
                data: json!({ "text": text }),
            },
        );
    }
}

#[async_trait]
impl AuditSink for DbPorts {
    async fn record(&self, entry: EngineAuditEntry) -> anyhow::Result<()> {
        let mut record =
            audit::AuditEntry::new(self.scope.tenant_id, "agent", entry.action, entry.outcome)
                .on_behalf_of(self.scope.user_id)
                .run(self.scope.run_id)
                .metadata(entry.metadata);
        record.resource_type = entry.resource_type;
        record.resource_id = entry.resource_id;
        record.reason = entry.reason;
        let mut tx = self.tx().await?;
        audit::append_audit(&mut tx, record).await?;
        Ok(tx.commit().await?)
    }
}

#[async_trait]
impl ToolInvocationLog for DbPorts {
    async fn record(&self, entry: ToolInvocationRecord) -> anyhow::Result<()> {
        let mut tx = self.tx().await?;
        tool_invocations::record_tool_invocation(
            &mut tx,
            tool_invocations::NewToolInvocation {
                tenant_id: self.scope.tenant_id,
                run_id: self.scope.run_id,
                tool_call_id: entry.tool_call_id,
                tool_name: entry.tool_name,
                input: Some(entry.input),
                decision: entry.decision.into(),
                decision_reason: entry.decision_reason,
                status: entry.status.into(),
                output_summary: entry.output_summary,
                error: entry.error,
                duration_ms: entry.duration_ms.map(saturating_i32),
                approval_id: entry.approval_id,
            },
        )
        .await?;
        Ok(tx.commit().await?)
    }
}

#[async_trait]
impl ApprovalStore for DbPorts {
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<Uuid> {
        let mut tx = self.tx().await?;
        let row = approvals::create_approval(
            &mut tx,
            approvals::NewApproval {
                tenant_id: self.scope.tenant_id,
                run_id: self.scope.run_id,
                requested_for_user_id: self.scope.user_id,
                tool_name: request.tool_name,
                tool_call_id: request.tool_call_id,
                action_summary: request.action_summary,
                payload: request.payload,
                risk: request.risk.as_str().into(),
                reason: request.reason,
                expires_at: Some(Utc::now() + chrono::Duration::days(APPROVAL_VALIDITY_DAYS)),
            },
        )
        .await?;
        tx.commit().await?;
        Ok(row.id)
    }
}

#[async_trait]
impl UsageSink for DbPorts {
    async fn record(&self, record: UsageRecord) -> anyhow::Result<()> {
        let mut tx = self.tx().await?;
        usage::append_usage(
            &mut tx,
            usage::NewUsage {
                tenant_id: self.scope.tenant_id,
                user_id: Some(self.scope.user_id),
                run_id: Some(self.scope.run_id),
                kind: record.kind,
                provider: record.provider,
                model: record.model,
                input_tokens: saturating_i32(record.input_tokens),
                output_tokens: saturating_i32(record.output_tokens),
                cost_micros: record.cost_micros,
            },
        )
        .await?;
        Ok(tx.commit().await?)
    }
}

#[async_trait]
impl RunStore for DbPorts {
    async fn checkpoint(&self, state: &RunCheckpoint) -> anyhow::Result<()> {
        let mut tx = self.tx().await?;
        runs::update_run(
            &mut tx,
            self.scope.run_id,
            runs::RunPatch {
                state: Some(serde_json::to_value(state)?),
                usage: Some(serde_json::to_value(state.usage)?),
                ..runs::RunPatch::default()
            },
        )
        .await?;
        Ok(tx.commit().await?)
    }

    async fn finish(&self, outcome: &RunOutcome) -> anyhow::Result<()> {
        persist_outcome(&self.db, self.scope, outcome).await
    }
}
