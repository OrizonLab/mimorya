//! Requests nobody decided in time (ADR-061). Every minute, the process
//! that executes runs marks each pending request past its deadline as
//! expired, audited as the platform's, and lets its run go on once nothing
//! else waits: the agent learns the action was not run, as when an expired
//! request is decided. Without it, a run would stay paused until someone
//! tried to decide. The day before, their manager is reminded (ADR-062).

use chrono::{DateTime, Utc};
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, RunCheckpoint};
use mimorya_db::repositories::approvals::{
    ApprovalRow, OverdueRun, expire_approval, list_approvals_for_run, overdue_approval_runs,
};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::runs::lock_run;
use serde_json::json;

use super::approval_decision::settle;
use super::db_ports::RunScope;
use super::service::RunService;

impl RunService {
    /// Expires the requests past their deadline and lets their runs go on;
    /// how many expired. Only in the process that executes runs: another
    /// leaves them to it.
    pub async fn expire_overdue(&self) -> anyhow::Result<usize> {
        if !self.executes().await {
            return Ok(0);
        }
        let now = Utc::now();
        let mut expired = 0;
        for run in overdue_approval_runs(self.db.pool(), now).await? {
            match self.expire_for(run, now).await {
                Ok(count) => expired += count,
                Err(error) => tracing::warn!(
                    run_id = %run.run_id,
                    error = format!("{error:#}"),
                    "Overdue requests not expired"
                ),
            }
        }
        Ok(expired)
    }

    /// Reminds managers of the requests that expire within a day (ADR-062);
    /// how many. Only in the process that executes runs, as expiries.
    pub async fn remind_due(&self) -> anyhow::Result<usize> {
        if !self.executes().await {
            return Ok(0);
        }
        self.executor.notifier.remind_due(Utc::now()).await
    }

    /// Expires the run's overdue requests, under its lock as a decision
    /// would; resumes it, or ends it, once nothing else waits. Whom the run
    /// acts for is read before anything is saved: should that fail, nothing
    /// changes, and the next sweep tries again.
    async fn expire_for(&self, overdue: OverdueRun, now: DateTime<Utc>) -> anyhow::Result<usize> {
        let OverdueRun { tenant_id, run_id } = overdue;
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(run) = lock_run(&mut tx, run_id).await? else {
            return Ok(0);
        };
        let past: Vec<ApprovalRow> = list_approvals_for_run(&mut tx, run_id)
            .await?
            .into_iter()
            .filter(|a| a.status == "pending" && a.expires_at.is_some_and(|at| at <= now))
            .collect();
        for approval in &past {
            expire_approval(&mut tx, approval.id).await?;
            append_audit(&mut tx, expiry_audit(tenant_id, approval)).await?;
        }
        let Some((run, approvals)) = settle(&mut tx, run).await? else {
            tx.commit().await?;
            return Ok(past.len());
        };
        self.hold(&mut tx, run_id).await?;
        // As a decision would: with the rights of whom the agent acts for.
        let access = self
            .access
            .active_context(tenant_id, run.on_behalf_of_user_id)
            .await?;
        tx.commit().await?;
        match access {
            Some(access) => self.resume(access, run, &approvals),
            None => {
                let scope = RunScope {
                    tenant_id,
                    run_id,
                    user_id: run.on_behalf_of_user_id,
                    conversation_id: run.conversation_id,
                };
                let checkpoint: Option<RunCheckpoint> = run
                    .state
                    .clone()
                    .and_then(|state| serde_json::from_value(state).ok());
                // Should this fail, the run, set going, is failed by the
                // stale run sweep.
                self.end(scope, ACCOUNT_INACTIVE, checkpoint).await?;
            }
        }
        Ok(past.len())
    }
}

fn expiry_audit(tenant_id: uuid::Uuid, approval: &ApprovalRow) -> AuditEntry {
    AuditEntry::new(tenant_id, "system", "approval.expired", "failure")
        .on_behalf_of(approval.requested_for_user_id)
        .run(approval.run_id)
        .resource("approval", approval.id)
        .reason("Délai de validation dépassé")
        .metadata(json!({
            "toolName": approval.tool_name,
            "risk": approval.risk,
            "routing": approval.routing,
        }))
}
