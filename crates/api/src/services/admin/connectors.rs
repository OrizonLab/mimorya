//! MCP connectors configured by a company's connector administrators
//! (ADR-025): created over HTTPS with an optional access token (sealed in
//! the secret store, never shown nor audited), changed, switched on or off,
//! and deleted, under the administration lock. A token never follows the
//! connector to another address.

use std::collections::HashMap;

use mimorya_connectors::mcp::url_problem;
use mimorya_core::DomainError;
use mimorya_core::authz::PermissionInfo;
use mimorya_db::Database;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, ConnectorPatch, NewConnectorInstance, create_connector_instance,
    delete_connector_instance, get_connector_instance, list_connector_instances,
    update_connector_instance,
};
use serde_json::{Value, json};
use uuid::Uuid;

use super::connector_rules::{ToolPolicyInput, checked_policies, prefix_problem, token_problem};
use super::{audit_entry, begin_change, is_unique_violation};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::connectors::{ConnectorService, token_secret_name};

const PERMISSION: &str = "connector:manage";

pub struct NewMcpConnector {
    pub name: String,
    pub prefix: String,
    pub url: String,
    pub token: Option<String>,
    pub enabled: bool,
}

/// The services a connector change uses.
pub struct ConnectorAdmin<'a> {
    pub db: &'a Database,
    pub connectors: &'a ConnectorService,
    pub secrets: &'a SecretStore,
    pub catalog: &'a [PermissionInfo],
}

/// The prefix of any MCP instance, operator-written ones included.
fn mcp_prefix(instance: &ConnectorInstanceRow) -> Option<&str> {
    (instance.connector_type == "mcp")
        .then(|| instance.config["prefix"].as_str())
        .flatten()
}

/// The prefix and address of an MCP instance configured over HTTP.
pub fn http_config(instance: &ConnectorInstanceRow) -> Option<(String, String)> {
    let config = &instance.config;
    (instance.connector_type == "mcp" && config["transport"] == "http").then(|| {
        (
            config["prefix"].as_str().unwrap_or_default().to_string(),
            config["url"].as_str().unwrap_or_default().to_string(),
        )
    })
}

pub async fn create_mcp_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewMcpConnector,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let problem = prefix_problem(&input.prefix)
        .or_else(|| url_problem(&input.url, admin.connectors.network()))
        .or_else(|| input.token.as_deref().and_then(token_problem));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    if input.token.is_some() {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    // Unique among every MCP server, operator-written ones included.
    let taken = list_connector_instances(&mut tx, false)
        .await?
        .iter()
        .filter_map(mcp_prefix)
        .any(|prefix| prefix == input.prefix);
    if taken {
        return Err(
            DomainError::conflict("Ce préfixe est déjà utilisé par un autre connecteur").into(),
        );
    }
    let config = json!({ "transport": "http", "prefix": input.prefix, "url": input.url });
    let created = create_connector_instance(
        &mut tx,
        NewConnectorInstance {
            tenant_id,
            connector_type: "mcp",
            name: &input.name,
            enabled: input.enabled,
            config: &config,
        },
    )
    .await;
    let id = match created {
        Ok(id) => id,
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict("Un connecteur porte déjà ce nom").into());
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(token) = &input.token {
        admin
            .secrets
            .put(&mut tx, tenant_id, &token_secret_name(id), token)
            .await?;
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.created", "connector", id).metadata(json!({
            "name": input.name,
            "type": "mcp",
            "prefix": input.prefix,
            "url": input.url,
            "enabled": input.enabled,
            "tokenStored": input.token.is_some(),
        })),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// `None` keeps the token, unless the address changes (it never follows).
pub enum TokenChange {
    Replace(String),
    Remove,
}

#[derive(Default)]
pub struct ConnectorChanges {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub url: Option<String>,
    pub token: Option<TokenChange>,
    pub tool_policies: Option<HashMap<String, ToolPolicyInput>>,
}

pub async fn update_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    changes: ConnectorChanges,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let problem = changes
        .url
        .as_deref()
        .and_then(|url| url_problem(url, admin.connectors.network()))
        .or_else(|| match &changes.token {
            Some(TokenChange::Replace(token)) => token_problem(token),
            _ => None,
        });
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    if matches!(changes.token, Some(TokenChange::Replace(_))) {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let Some(instance) = get_connector_instance(&mut tx, id).await? else {
        return Err(DomainError::not_found("Connecteur introuvable").into());
    };
    let mut patch = ConnectorPatch {
        name: changes.name.clone(),
        enabled: changes.enabled,
        ..ConnectorPatch::default()
    };
    let mut token = "kept";
    if changes.url.is_some() || changes.token.is_some() || changes.tool_policies.is_some() {
        let Some((prefix, current_url)) = http_config(&instance) else {
            return Err(DomainError::validation(
                "Ce connecteur est réglé par l’exploitant : seuls son nom et son activation \
                 changent ici",
            )
            .into());
        };
        if let Some(policies) = &changes.tool_policies {
            patch.tool_policies = Some(checked_policies(policies, &prefix, admin.catalog)?);
        }
        let moved = changes.url.as_ref().is_some_and(|url| *url != current_url);
        patch.reachability_changed = moved || changes.token.is_some();
        if let Some(url) = &changes.url {
            patch.config = Some(json!({ "transport": "http", "prefix": prefix, "url": url }));
        }
        let secret = token_secret_name(id);
        token = match &changes.token {
            Some(TokenChange::Replace(value)) => {
                admin
                    .secrets
                    .put(&mut tx, tenant_id, &secret, value)
                    .await?;
                "replaced"
            }
            Some(TokenChange::Remove) => {
                admin.secrets.delete(&mut tx, &secret).await?;
                "removed"
            }
            // A token never follows the connector to another address.
            None if moved && admin.secrets.delete(&mut tx, &secret).await? => "removed",
            None => "kept",
        };
    }
    let metadata = json!({
        "name": changes.name,
        "previousName": changes.name.as_ref().map(|_| &instance.name),
        "enabled": changes.enabled,
        "url": changes.url,
        "previousUrl": changes.url.as_ref().map(|_| &instance.config["url"]),
        "token": token,
        "toolPolicies": patch.tool_policies,
    });
    match update_connector_instance(&mut tx, id, patch).await {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(DomainError::conflict("Un connecteur porte déjà ce nom").into());
        }
        Err(error) => return Err(error.into()),
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.updated", "connector", id).metadata(metadata),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    Ok(())
}

pub async fn delete_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let Some(instance) = get_connector_instance(&mut tx, id).await? else {
        return Err(DomainError::not_found("Connecteur introuvable").into());
    };
    if http_config(&instance).is_none() {
        return Err(DomainError::validation(
            "Seuls les serveurs MCP ajoutés ici se suppriment ici ; désactivez plutôt ce connecteur",
        )
        .into());
    }
    delete_connector_instance(&mut tx, id).await?;
    admin
        .secrets
        .delete(&mut tx, &token_secret_name(id))
        .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.deleted", "connector", id).metadata(json!({
            "name": instance.name,
            "config": public_config(&instance.config),
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    Ok(())
}

/// What an audit entry may keep of a configuration: no environment of a
/// child process (it may carry credentials).
fn public_config(config: &Value) -> Value {
    json!({ "transport": config["transport"], "prefix": config["prefix"], "url": config["url"] })
}
