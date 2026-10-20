//! Tries an MCP server before it is saved (ADR-050): started as a run would
//! start it, with the address and the token typed in the form, its tools
//! listed as they declare themselves, then released. Nothing is stored but
//! the audit entry, which keeps the address and the outcome, never the token.

use mimorya_connectors::mcp::{token_problem, url_problem};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;

use super::connector_rules::prefix_problem;
use super::connector_test::{ConnectorTestOutcome, tested};
use super::connectors::ConnectorAdmin;
use crate::services::AccessContext;
use crate::services::connectors::failure;

const PERMISSION: &str = "connector:manage";

/// An MCP server as the form describes it.
pub struct McpTrial {
    pub prefix: String,
    pub url: String,
    pub token: Option<String>,
}

pub async fn try_mcp_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    trial: McpTrial,
) -> anyhow::Result<ConnectorTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    // Checked as when it is added: an address this server may not reach is
    // refused before any connection.
    let problem = prefix_problem(&trial.prefix)
        .or_else(|| url_problem(&trial.url, admin.connectors.network()))
        .or_else(|| trial.token.as_deref().and_then(token_problem));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    let token = trial.token.as_deref();
    let config = json!({ "transport": "http", "prefix": trial.prefix, "url": trial.url });
    let outcome = match admin.connectors.start("mcp", &config, token).await {
        Ok(runtime) => {
            let tools = runtime
                .tools
                .iter()
                .map(|tool| tested(tool.info(), None))
                .collect();
            if let Some(closer) = &runtime.closer {
                closer.close().await;
            }
            ConnectorTestOutcome { tools, error: None }
        }
        Err(error) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(failure(&error, token)),
        },
    };
    let tenant_id = access.subject.tenant_id;
    let entry = AuditEntry::new(tenant_id, "user", "connector.tried", "success")
        .by_user(access.subject.user_id)
        .metadata(json!({
            "type": "mcp",
            "prefix": trial.prefix,
            "url": trial.url,
            "tokenSent": token.is_some(),
            "ok": outcome.error.is_none(),
            "tools": outcome.tools.len(),
        }));
    let mut tx = admin.db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(outcome)
}
