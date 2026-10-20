//! Tries an MCP server before it is saved (ADR-050): started as a run would
//! start it, with the address and the token typed in the form, its tools
//! listed as they declare themselves, then released. Nothing is stored but
//! the audit entry, which keeps the address and the outcome, never the token.
//! The trial runs apart from its request: a client that leaves does not stop
//! it before its entry is written.

use std::sync::Arc;

use anyhow::anyhow;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;
use tokio::sync::oneshot;
use uuid::Uuid;

use super::connector_rules::mcp_addition_problem;
use super::connector_test::{ConnectorTestOutcome, start_and_list};
use crate::services::AccessContext;
use crate::services::background::BackgroundTasks;
use crate::services::connectors::{ConnectorService, Policies};

const PERMISSION: &str = "connector:manage";

/// An MCP server as the form describes it.
pub struct McpTrial {
    pub prefix: String,
    pub url: String,
    pub token: Option<String>,
}

/// What a trial needs, owned: it outlives the request that asked for it.
pub struct TrialServices {
    pub db: Database,
    pub connectors: Arc<ConnectorService>,
    pub background: Arc<BackgroundTasks>,
}

pub async fn try_mcp_connector(
    services: TrialServices,
    access: &AccessContext,
    trial: McpTrial,
) -> anyhow::Result<ConnectorTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    let network = services.connectors.network();
    let problem = mcp_addition_problem(&trial.prefix, &trial.url, trial.token.as_deref(), network);
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let start = services.connectors.claim_admin_start(tenant_id)?;
    let (answer, answered) = oneshot::channel();
    let background = services.background.clone();
    background.spawn(async move {
        // Held by the trial, not by its request: freed when the trial ends.
        let _start = start;
        let config = json!({ "transport": "http", "prefix": trial.prefix, "url": trial.url });
        let token = trial.token.as_deref();
        let outcome = start_and_list(&services.connectors, "mcp", &config, token, |_| {
            Policies::new()
        })
        .await;
        let recorded = record(&services.db, (tenant_id, user_id), &trial, &outcome).await;
        if let Err(error) = &recorded {
            tracing::error!(
                error = format!("{error:#}"),
                "Failed to audit a connector trial"
            );
        }
        // The client may have left: the trial is over all the same.
        let _ = answer.send(recorded.map(|()| outcome));
    });
    answered
        .await
        .map_err(|_| anyhow!("the connector trial ended without an answer"))?
}

async fn record(
    db: &Database,
    (tenant_id, user_id): (Uuid, Uuid),
    trial: &McpTrial,
    outcome: &ConnectorTestOutcome,
) -> anyhow::Result<()> {
    let entry = AuditEntry::new(tenant_id, "user", "connector.tried", "success")
        .by_user(user_id)
        .metadata(json!({
            "type": "mcp",
            "prefix": trial.prefix,
            "url": trial.url,
            "tokenSent": trial.token.is_some(),
            "ok": outcome.error.is_none(),
            "tools": outcome.tools.len(),
        }));
    let mut tx = db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}
