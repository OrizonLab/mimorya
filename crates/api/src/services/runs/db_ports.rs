//! PostgreSQL-backed implementation of the engine ports for one run. Every
//! operation runs in its own tenant-bound transaction (RLS applies); one
//! that changes the run (journal, saved step, requests, outcome) holds the
//! execution's lease first (`fence.rs`, ADR-086), until the run has ended.
//! The audit and the usage are recorded whatever: they say what happened.
//! Memory and skills live in `knowledge_ports.rs`, deliverables in
//! `deliverables.rs`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use mimorya_agent::runtime::{
    ApprovalPlacement, ApprovalRequest, ApprovalStore, AuditSink, EngineAuditEntry, EnginePorts,
    RunCheckpoint, RunEvent, RunEventSink, RunOutcome, RunStore, ToolInvocationLog,
    ToolInvocationRecord, UsageRecord, UsageSink,
};
use mimorya_db::repositories::approvals::ApprovalRouting;
use mimorya_db::repositories::{approvals, audit, runs, tool_invocations, usage};
use mimorya_db::{Database, TenantTx};
use uuid::Uuid;

use super::approval_notice::ApprovalNotifier;
use super::approval_routing::{NOBODY_VALIDATES, Route, route};
use super::deliverables::DbDeliverables;
use super::events::append_in;
use super::executor::RunExecutor;
use super::fence::LeaseFence;
use super::lesson_capture::LessonCapture;
use super::outcome::persist_outcome;
use crate::services::access::AccessService;
use crate::services::embeddings::EmbeddingService;
use crate::services::event_bus::RunEventBus;

/// No event recorded yet by this execution.
const NO_EVENT: i32 = -1;

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
    /// The last event this execution recorded (`NO_EVENT` before the first):
    /// a text delta follows it (ADR-084).
    last_event: Arc<AtomicI32>,
    /// Who may validate a request for the run's user (ADR-059).
    access: Arc<AccessService>,
    /// Tells the manager a request awaits them (ADR-060).
    notifier: ApprovalNotifier,
    /// Searches the company's memory by meaning too (ADR-074).
    pub(super) embeddings: Arc<EmbeddingService>,
    /// Draws a lesson from the run once completed (ADR-076).
    lessons: LessonCapture,
    /// The execution's lease on the run, held by each change (ADR-086).
    fence: LeaseFence,
    /// Whether this execution ended the run: what follows (its last event)
    /// no longer runs under a lease.
    ended: Arc<AtomicBool>,
}

/// The ports of `scope`'s run, executed by `executor` under `fence`.
pub fn engine_ports(executor: &RunExecutor, scope: RunScope, fence: LeaseFence) -> EnginePorts {
    let db = executor.db.clone();
    let ports = Arc::new(DbPorts {
        db: db.clone(),
        scope,
        bus: executor.bus.clone(),
        last_event: Arc::new(AtomicI32::new(NO_EVENT)),
        access: executor.access.clone(),
        notifier: executor.notifier.clone(),
        embeddings: executor.embeddings.clone(),
        lessons: executor.lessons.clone(),
        fence: fence.clone(),
        ended: Arc::new(AtomicBool::new(false)),
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
        deliverables: Arc::new(DbDeliverables::new(db, scope, fence)),
    }
}

impl DbPorts {
    /// A transaction to read in, or to record what happened.
    pub(super) async fn tx(&self) -> anyhow::Result<TenantTx> {
        Ok(self.db.begin(self.scope.tenant_id).await?)
    }

    /// A transaction to change the run in, under the execution's lease
    /// while the run has not ended.
    pub(super) async fn held_tx(&self) -> anyhow::Result<TenantTx> {
        let mut tx = self.tx().await?;
        if !self.ended.load(Ordering::Relaxed) {
            self.fence.hold_in(&mut tx, self.scope.run_id).await?;
        }
        Ok(tx)
    }
}

fn saturating_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[async_trait]
impl RunEventSink for DbPorts {
    async fn emit(&self, event: RunEvent) -> anyhow::Result<()> {
        let run_id = self.scope.run_id;
        let mut tx = self.held_tx().await?;
        let recorded = append_in(&mut tx, &self.bus, run_id, event).await?;
        tx.commit().await?;
        if let Some(seq) = recorded.seq {
            self.last_event.fetch_max(seq, Ordering::Relaxed);
        }
        self.bus.publish(run_id, recorded);
        Ok(())
    }

    fn publish_delta(&self, text: &str) {
        let last = self.last_event.load(Ordering::Relaxed);
        let after = (last != NO_EVENT).then_some(last);
        self.bus.publish_delta(self.scope.run_id, after, text);
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
        let mut tx = self.held_tx().await?;
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
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<ApprovalPlacement> {
        let RunScope {
            tenant_id, user_id, ..
        } = self.scope;
        let routing = match route(&self.db, &self.access, tenant_id, user_id, request.risk).await? {
            Route::Requester => ApprovalRouting::Requester,
            Route::Manager(manager) => ApprovalRouting::Manager(manager),
            Route::Nobody => return Ok(ApprovalPlacement::Unroutable(NOBODY_VALIDATES.into())),
        };
        let mut tx = self.held_tx().await?;
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
                routing,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(ApprovalPlacement::Pending(row.id))
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
        let mut tx = self.held_tx().await?;
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
        let tx = self.held_tx().await?;
        persist_outcome(tx, self.scope, outcome).await?;
        self.ended.store(true, Ordering::Relaxed);
        self.lessons.after_run(self.scope, outcome);
        // Paused, and so decidable: the managers of its requests are told,
        // once per pause (ADR-060).
        if let RunOutcome::AwaitingApproval { approvals, .. } = outcome {
            let ids = approvals.iter().map(|a| a.approval_id).collect();
            let RunScope {
                tenant_id, run_id, ..
            } = self.scope;
            self.notifier.notify(tenant_id, run_id, ids);
        }
        Ok(())
    }
}
