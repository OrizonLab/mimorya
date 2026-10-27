//! PostgreSQL-backed implementation of the engine ports for one run. Every
//! operation runs in its own tenant-bound transaction (RLS applies); one
//! that changes the run (journal, saved step, requests, outcome) holds the
//! execution's lease first (`fence.rs`, ADR-086), until the run has ended.
//! The audit, the usage and the calls made (recorded once a tool returned)
//! are recorded whatever: they say what happened, even should the lease
//! have gone elsewhere while a tool worked. A sub-agent's run (ADR-098)
//! holds its parent's lease, asks for no validation, draws no lesson and
//! opens no run of its own.
//! Memory and skills live in `knowledge_ports.rs`, deliverables in
//! `deliverables.rs`, sub-agents' runs in `sub_runs.rs`.

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
use super::reads::Reader;
use crate::services::access::AccessService;
use crate::services::account_links::{AccountLinkService, PersonAccounts};
use crate::services::embeddings::{EmbeddingService, UsageBy};
use crate::services::event_bus::RunEventBus;

/// No event recorded yet by this execution.
const NO_EVENT: i32 = -1;

/// Approval requests left unanswered expire after a week.
const APPROVAL_VALIDITY_DAYS: i64 = 7;

/// Why a sub-agent's call that needs a validation does not run (ADR-098).
const SUB_AGENT_ASKS_NONE: &str = "Un sous-agent ne demande pas de validation : seul l’agent principal peut proposer cette action.";

#[derive(Debug, Clone, Copy)]
pub struct RunScope {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    /// User on whose behalf the agent acts.
    pub user_id: Uuid,
    /// Conversation that receives the answer, if the run belongs to one.
    pub conversation_id: Option<Uuid>,
    /// The mission the run is a stage of, if any (ADR-089).
    pub mission_id: Option<Uuid>,
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
    /// The run whose lease each change holds: this one, or its parent for
    /// a sub-agent's run (ADR-098).
    leased: Uuid,
    /// A sub-agent's run (ADR-098).
    pub(super) sub_run: bool,
    /// The accounts the run's user linked, for its tools (ADR-117).
    account_links: Arc<AccountLinkService>,
}

/// The ports of `scope`'s run, executed by `executor` under `fence`.
pub fn engine_ports(executor: &RunExecutor, scope: RunScope, fence: LeaseFence) -> EnginePorts {
    DbPorts {
        db: executor.db.clone(),
        scope,
        bus: executor.bus.clone(),
        last_event: Arc::new(AtomicI32::new(NO_EVENT)),
        access: executor.access.clone(),
        notifier: executor.notifier.clone(),
        embeddings: executor.embeddings.clone(),
        lessons: executor.lessons.clone(),
        fence,
        ended: Arc::new(AtomicBool::new(false)),
        leased: scope.run_id,
        sub_run: false,
        account_links: executor.account_links.clone(),
    }
    .into_engine_ports()
}

impl DbPorts {
    /// The ports of the sub-agent's run `run_id` of this run (ADR-098):
    /// its own journal, under this run's lease.
    pub(super) fn of_sub_run(&self, run_id: Uuid) -> EnginePorts {
        DbPorts {
            scope: RunScope {
                run_id,
                conversation_id: None,
                mission_id: None,
                ..self.scope
            },
            last_event: Arc::new(AtomicI32::new(NO_EVENT)),
            ended: Arc::new(AtomicBool::new(false)),
            leased: self.leased,
            sub_run: true,
            ..self.clone()
        }
        .into_engine_ports()
    }

    fn into_engine_ports(self) -> EnginePorts {
        let deliverables =
            DbDeliverables::new(self.db.clone(), self.scope, self.fence.clone(), self.leased);
        let accounts = PersonAccounts::new(
            self.account_links.clone(),
            self.scope.tenant_id,
            self.scope.user_id,
        );
        let ports = Arc::new(self);
        EnginePorts {
            events: ports.clone(),
            audit: ports.clone(),
            tool_log: ports.clone(),
            approvals: ports.clone(),
            usage: ports.clone(),
            memory: ports.clone(),
            documents: ports.clone(),
            skills: ports.clone(),
            sub_runs: ports.clone(),
            runs: ports,
            deliverables: Arc::new(deliverables),
            accounts: Arc::new(accounts),
        }
    }

    /// A transaction to read in, or to record what happened.
    pub(super) async fn tx(&self) -> anyhow::Result<TenantTx> {
        Ok(self.db.begin(self.scope.tenant_id).await?)
    }

    /// What the run reads of the memory and the documents, its requests'
    /// vectors counted for it.
    pub(super) fn reader(&self) -> Reader<'_> {
        Reader {
            db: &self.db,
            embeddings: &self.embeddings,
            tenant_id: self.scope.tenant_id,
            by: UsageBy {
                user_id: Some(self.scope.user_id),
                run_id: Some(self.scope.run_id),
            },
        }
    }

    /// A transaction to change the run in, under the execution's lease
    /// while the run has not ended.
    pub(super) async fn held_tx(&self) -> anyhow::Result<TenantTx> {
        let mut tx = self.tx().await?;
        if !self.ended.load(Ordering::Relaxed) {
            self.fence.hold_in(&mut tx, self.leased).await?;
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
        // An execution that lost its lease no longer speaks for the run.
        if !self.fence.holds() {
            return;
        }
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
        // What the call did, whoever holds the run now.
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
    async fn create(&self, request: ApprovalRequest) -> anyhow::Result<ApprovalPlacement> {
        if self.sub_run {
            return Ok(ApprovalPlacement::Unroutable(SUB_AGENT_ASKS_NONE.into()));
        }
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
        persist_outcome(tx, &self.bus, self.scope, outcome).await?;
        self.ended.store(true, Ordering::Relaxed);
        if self.sub_run {
            return Ok(());
        }
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
