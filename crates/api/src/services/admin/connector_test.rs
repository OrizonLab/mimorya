//! Tries a connector the way a run would start it (a fresh instance) and
//! lists its tools with the policy in force for each, so that an
//! administrator maps them knowingly. How it went is recorded on the
//! instance; the audit keeps the outcome, never the token. A server each
//! person authorizes is tried with the administrator's own authorization,
//! never another person's nor the company's, and refused without it; how
//! it went then says as much of their authorization as of the server: not
//! recorded on the instance nor retried for everyone, unless its policies,
//! the server's own, cannot be read (ADR-174).

use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_core::DomainError;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_renewals::RenewalFor;
use mimorya_db::repositories::connectors::{get_connector_instance, report_connector_status};
use serde_json::{Value, json};
use uuid::Uuid;

use super::audit_entry;
use super::connectors::ConnectorAdmin;
use crate::services::AccessContext;
use crate::services::connectors::{
    ConnectorService, Credential, CredentialFor, Policies, Whose, credential_for, failure,
    read_policies, scoped_overrides,
};

const PERMISSION: &str = "connector:manage";

/// Why a server each person authorizes is not tried by an administrator
/// who has not authorized it (ADR-174).
pub const TEST_OWN_AUTHORIZATION: &str = "Chaque personne autorise ce serveur pour elle-même : \
     autorisez-le d’abord dans « Mon compte », le test se fait avec votre autorisation";

pub struct TestedTool {
    pub name: String,
    pub title: String,
    /// What the model reads unless replaced: the server's own.
    pub description: String,
    /// The administrator's replacement, as written (ADR-057).
    pub replaced_description: Option<String>,
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
    let known = get_connector_instance(&mut tx, id).await?.is_some();
    tx.commit().await?;
    if !known {
        return Err(DomainError::not_found("Connecteur introuvable").into());
    }
    // Once the connector is known to exist (an absent one is answered as
    // such, and traced: ADR-047), held until the outcome is recorded.
    let _start = admin.connectors.claim_admin_start(tenant_id)?;
    // As a run would: its authorization renewed first if about to expire,
    // the company's or, where each person authorizes it, the
    // administrator's own (ADR-174); even disabled, the test starting it
    // all the same.
    let (asker, own) = (RenewalFor::Test, Whose::Person(access.subject.user_id));
    let connectors = admin.connectors;
    connectors
        .renew_if_due(tenant_id, id, Whose::Company, asker)
        .await;
    connectors.renew_if_due(tenant_id, id, own, asker).await;
    // The instance and its token from one snapshot.
    let mut tx = admin.db.begin_snapshot(tenant_id).await?;
    let Some(instance) = get_connector_instance(&mut tx, id).await? else {
        return Err(DomainError::not_found("Connecteur introuvable").into());
    };
    let user_id = access.subject.user_id;
    let credential = credential_for(&mut tx, admin.secrets, &instance, user_id).await?;
    tx.commit().await?;
    let (credential, own) = match credential {
        CredentialFor::Shared(credential) => (credential, false),
        CredentialFor::Own { credential, .. } => (credential, true),
        CredentialFor::NotAuthorized => {
            return Err(DomainError::conflict(TEST_OWN_AUTHORIZATION).into());
        }
    };

    let policies = read_policies(&instance);
    // The server's own: written whoever's authorization it was tried with.
    let unreadable_policies = policies.is_err();
    let outcome = match (policies, credential) {
        (Err(error), _) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(error),
        },
        (_, Err(error)) => ConnectorTestOutcome {
            tools: Vec::new(),
            error: Some(error.to_string()),
        },
        (Ok(policies), Ok(credential)) => {
            let (kind, config) = (&instance.connector_type, &instance.config);
            // Written by an administrator, shown as written: an MCP server's
            // only, as runs read no other (ADR-057).
            let written: HashMap<String, String> = policies
                .iter()
                .filter(|_| instance.connector_type == "mcp")
                .filter_map(|(name, p)| Some((name.clone(), p.description.clone()?)))
                .collect();
            // As runs apply them.
            let effective = |names: &[String]| scoped_overrides(&instance, policies, names);
            let mut outcome =
                start_and_list(admin.connectors, kind, config, &credential, effective).await;
            for tool in &mut outcome.tools {
                tool.replaced_description = written.get(&tool.name).cloned();
            }
            outcome
        }
    };
    // One person's authorization, failing or not, is not the server's
    // status: neither written nor retried for everyone.
    if outcome.error.is_none() && !own {
        admin.connectors.retry_now(id).await;
    }
    let mut tx = admin.db.begin(tenant_id).await?;
    if !own || unreadable_policies {
        let error = outcome.error.as_deref();
        report_connector_status(&mut tx, id, instance.updated_at, error).await?;
    }
    let mut metadata = json!({
        "ok": outcome.error.is_none(),
        "tools": outcome.tools.len(),
    });
    if own {
        metadata["ownAuthorization"] = json!(true);
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.tested", "connector", id).metadata(metadata),
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
    credential: &Credential,
    effective: impl FnOnce(&[String]) -> Policies,
) -> ConnectorTestOutcome {
    match connectors.start(connector_type, config, credential).await {
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
            error: Some(failure(&error, credential.token.as_deref())),
        },
    }
}

/// The tool as runs see it: its declaration, then its effective policy.
fn tested(info: &mimorya_core::tools::ToolInfo, policy: Option<&ToolOverride>) -> TestedTool {
    TestedTool {
        name: info.name.clone(),
        title: info.title.clone(),
        description: info.description.clone(),
        replaced_description: None,
        enabled: policy.and_then(|p| p.enabled).unwrap_or(true),
        risk: policy.and_then(|p| p.risk).unwrap_or(info.risk),
        permissions: policy
            .and_then(|p| p.permissions.clone())
            .unwrap_or_else(|| info.required_permissions.clone()),
    }
}
