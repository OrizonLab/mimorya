//! Tries a connector the way a run would start it (a fresh instance) and
//! lists its tools with the policy in force for each, so that an
//! administrator maps them knowingly. How it went is recorded on the
//! instance; the audit keeps the outcome, never the token.

use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_core::DomainError;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{get_connector_instance, report_connector_status};
use serde_json::json;
use uuid::Uuid;

use super::audit_entry;
use super::connectors::ConnectorAdmin;
use crate::services::AccessContext;
use crate::services::connectors::{CONNECTOR_USE, failure, opened, token_secret_name};

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
    let mut tx = admin.db.begin(tenant_id).await?;
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
    let policies: HashMap<String, ToolOverride> =
        serde_json::from_value(instance.tool_policies.clone()).unwrap_or_default();

    let outcome = match secret {
        Err(error) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(error.to_string()),
        },
        Ok(token) => {
            let started = admin
                .connectors
                .start(&instance.connector_type, &instance.config, token.as_deref())
                .await;
            match started {
                Ok(runtime) => {
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
                    error: Some(failure(&error, token.as_deref())),
                },
            }
        }
    };
    let mut tx = admin.db.begin(tenant_id).await?;
    report_connector_status(&mut tx, id, outcome.error.as_deref()).await?;
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

/// The tool as runs would see it: its declaration, then the policy.
fn tested(info: &mimorya_core::tools::ToolInfo, policy: Option<&ToolOverride>) -> TestedTool {
    let mut permissions: Vec<String> = policy
        .and_then(|p| p.permissions.clone())
        .unwrap_or_else(|| info.required_permissions.clone());
    if !permissions.iter().any(|p| p == CONNECTOR_USE) {
        permissions.push(CONNECTOR_USE.into());
    }
    TestedTool {
        name: info.name.clone(),
        title: info.title.clone(),
        description: info.description.clone(),
        enabled: policy.and_then(|p| p.enabled).unwrap_or(true),
        risk: policy.and_then(|p| p.risk).unwrap_or(info.risk),
        permissions,
    }
}
