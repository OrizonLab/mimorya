//! Connectors configured by a company's connector administrators: MCP
//! servers created over HTTPS (ADR-025; REST APIs: `rest_connectors`,
//! ADR-112) with an optional access token (sealed in the secret store, never
//! shown nor audited), changed, switched on or off, and deleted, under the
//! administration lock. A token never follows the connector to another
//! address.

use std::collections::HashMap;

use mimorya_connectors::outbound::{token_problem, url_problem};
use mimorya_core::DomainError;
use mimorya_core::account_links::LinkKind;
use mimorya_core::authz::PermissionInfo;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorPatch, NewConnectorInstance, create_connector_instance, delete_connector_instance,
    get_connector_instance, update_connector_instance,
};
use mimorya_db::{Database, is_unique_violation};
use serde_json::{Value, json};
use uuid::Uuid;

use super::connector_config::{Editable, RestChanges, added_here, operation_names, prefix_taken};
use super::connector_rules::{ToolPolicyInput, mcp_addition_problem};
use super::{audit_entry, begin_change};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::connectors::{ConnectorService, token_secret_name};

pub(super) const PERMISSION: &str = "connector:manage";

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

pub async fn create_mcp_connector(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    input: NewMcpConnector,
) -> anyhow::Result<Uuid> {
    access.require_tenant_wide(PERMISSION)?;
    let network = admin.connectors.network();
    let problem = mcp_addition_problem(&input.prefix, &input.url, input.token.as_deref(), network);
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    if input.token.is_some() {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    if prefix_taken(&mut tx, "mcp", &input.prefix).await? {
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
    /// A REST API's token header and operations (ADR-112).
    pub rest: Option<RestChanges>,
    /// Whether the Microsoft 365 application's tools may write (ADR-119).
    pub write_tools: Option<bool>,
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
    let header = changes
        .rest
        .as_ref()
        .and_then(|rest| rest.token_header.clone());
    let operations = changes
        .rest
        .as_ref()
        .and_then(|rest| rest.operations.as_deref().map(operation_names))
        .map(|names| json!(names));
    let mut previous_url = None;
    let mut dropped = Vec::new();
    let configured =
        changes.url.is_some() || changes.rest.is_some() || changes.write_tools.is_some();
    if configured || changes.token.is_some() || changes.tool_policies.is_some() {
        let Some(editable) = Editable::of(&instance) else {
            let refusal = if instance.connector_type == "rest" {
                "Déclaration de cette API illisible : supprimez-la puis ajoutez-la de nouveau"
            } else if let Some(kind) = LinkKind::of(&instance.connector_type) {
                &format!(
                    "Déclaration {} illisible : supprimez le connecteur puis ajoutez-le de \
                     nouveau",
                    kind.product()
                )
            } else {
                "Ce connecteur est réglé par l’exploitant : seuls son nom et son activation \
                 changent ici"
            };
            return Err(DomainError::validation(refusal).into());
        };
        if editable.requires_secret() && matches!(changes.token, Some(TokenChange::Remove)) {
            return Err(DomainError::validation(
                "Le secret de l’application ne se retire pas : remplacez-le, ou supprimez le \
                 connecteur",
            )
            .into());
        }
        let replaced = changes.rest.as_ref().and_then(|r| r.operations.as_deref());
        let policies = editable.policies_after(
            &instance.tool_policies,
            changes.tool_policies.as_ref(),
            replaced,
            admin.catalog,
        )?;
        patch.tool_policies = policies.stored;
        dropped = policies.dropped;
        let current_url = editable.url().to_string();
        let moved = changes.url.as_ref().is_some_and(|url| *url != current_url);
        let rehomed = header
            .as_ref()
            .is_some_and(|header| header.as_deref() != editable.token_header());
        patch.reachability_changed = moved || rehomed || changes.token.is_some();
        if configured {
            let network = admin.connectors.network();
            let config = editable.changed(
                changes.url.clone(),
                changes.rest,
                changes.write_tools,
                network,
            )?;
            patch.config = Some(config);
            previous_url = changes.url.as_ref().map(|_| current_url);
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
        "previousUrl": previous_url,
        "token": token,
        "tokenHeader": header,
        "operations": operations,
        "writeTools": changes.write_tools,
        "toolPolicies": patch.tool_policies,
        "droppedPolicies": (!dropped.is_empty()).then_some(&dropped),
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
    if !added_here(&instance) {
        return Err(DomainError::validation(
            "Seuls les serveurs MCP, les API REST et les applications Microsoft 365 et Google \
             Workspace ajoutés ici se suppriment ici ; désactivez plutôt ce connecteur",
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
