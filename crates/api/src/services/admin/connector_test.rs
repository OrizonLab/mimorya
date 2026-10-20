//! Tries a connector the way a run would start it (a fresh instance) and
//! lists its tools with the policy in force for each, so that an
//! administrator maps them knowingly. How it went is recorded on the
//! instance; the audit keeps the outcome, never the token.

use mimorya_agent::tools::ToolOverride;
use mimorya_core::DomainError;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{get_connector_instance, report_connector_status};
use serde_json::{Value, json};
use uuid::Uuid;

use super::audit_entry;
use super::connectors::ConnectorAdmin;
use crate::services::AccessContext;
use crate::services::connectors::{
    ConnectorService, Policies, failure, opened, read_policies, scoped_overrides, token_secret_name,
};

const PERMISSION: &str = "connector:manage";

pub struct TestedTool {
    pub name: String,
    pub title: String,
    pub description: String,
    pub enabled: bool,
    pub risk: ToolRisk,
    pub permissions: Vec<String>,
}

pub struct ConnectorTestOutcome {
    pub tools: Vec<TestedTool>,
    /// Why it could not start (without the token).
    pub error: Option<String>,
}

pub async fn test_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<ConnectorTestOutcome> {
    access.require_tenant_wide(PERMISSION)?;
    let tenant_id = access.subject.tenant_id;
    // The instance and its token from one snapshot.
    let mut tx = admin.db.begin_snapshot(tenant_id).await?;
    let Some(instance) = get_connector_instance(&mut tx, id).await? else {
        return Err(DomainError::not_found("Connecteur introuvable").into());
    };
    let secret = opened(
        admin
            .secrets
            .read(&mut tx, tenant_id, &token_secret_name(id))
            .await?,
    );
    tx.commit().await?;
    // Once the connector is known to exist (an absent one is answered as
    // such, and traced: ADR-047), held until the outcome is recorded.
    let _start = admin.connectors.claim_admin_start(tenant_id)?;

    let outcome = match (read_policies(&instance), secret) {
        (Err(error), _) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(error),
        },
        (_, Err(error)) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(error.to_string()),
        },
        (Ok(policies), Ok(token)) => {
            let (kind, config) = (&instance.connector_type, &instance.config);
            // As runs apply them.
            let effective = |names: &[String]| scoped_overrides(&instance, policies, names);
            start_and_list(admin.connectors, kind, config, token.as_deref(), effective).await
        }
    };
    if outcome.error.is_none() {
        admin.connectors.retry_now(id).await;
    }
    let mut tx = admin.db.begin(tenant_id).await?;
    report_connector_status(&mut tx, id, instance.updated_at, outcome.error.as_deref()).await?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.tested", "connector", id).metadata(json!({
            "ok": outcome.error.is_none(),
            "tools": outcome.tools.len(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(outcome)
}

/// Starts a fresh instance as a run would, lists its tools with the policy
/// `effective` gives them (from their names), then releases it. The token
/// never appears in the failure.
pub(super) async fn start_and_list(
    connectors: &ConnectorService,
    connector_type: &str,
    config: &Value,
    token: Option<&str>,
    effective: impl FnOnce(&[String]) -> Policies,
) -> ConnectorTestOutcome {
    match connectors.start(connector_type, config, token).await {
        Ok(runtime) => {
            let names: Vec<String> = runtime
                .tools
                .iter()
                .map(|t| t.info().name.clone())
                .collect();
            let policies = effective(&names);
            let tools = runtime
                .tools
                .iter()
                .map(|tool| tested(tool.info(), policies.get(&tool.info().name)))
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
    }
}

/// The tool as runs see it: its declaration, then its effective policy.
fn tested(info: &mimorya_core::tools::ToolInfo, policy: Option<&ToolOverride>) -> TestedTool {
    TestedTool {
        name: info.name.clone(),
        title: info.title.clone(),
        description: info.description.clone(),
        enabled: policy.and_then(|p| p.enabled).unwrap_or(true),
        risk: policy.and_then(|p| p.risk).unwrap_or(info.risk),
        permissions: policy
            .and_then(|p| p.permissions.clone())
            .unwrap_or_else(|| info.required_permissions.clone()),
    }
}
