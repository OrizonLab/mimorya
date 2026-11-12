//! Connectors configured by a company's connector administrators: MCP
//! servers created over HTTPS (ADR-025; REST APIs: `rest_connectors`,
//! ADR-112) with an optional access token (sealed in the secret store, never
//! shown nor audited), changed, switched on or off, and deleted, under the
//! administration lock. A token never follows the connector to another
//! address, nor does an OAuth declaration and its client's secret
//! (`connector_oauth`, ADR-171), which no typed token goes with.

use std::collections::HashMap;
use std::sync::Arc;

use mimorya_connectors::outbound::{token_problem, url_problem};
use mimorya_connectors::standard_mail::MAIL_SERVER;
use mimorya_core::DomainError;
use mimorya_core::authz::PermissionInfo;
use mimorya_db::repositories::account_links::take_account_links_of_connector;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, ConnectorPatch, NewConnectorInstance, create_connector_instance,
    delete_connector_instance, get_connector_instance, update_connector_instance,
};
use mimorya_db::{Database, is_unique_violation};
use serde_json::{Value, json};
use uuid::Uuid;

use super::added_connectors::{added_here, operation_names, prefix_taken, unchangeable};
use super::connector_authorization::{revoke, take_authorizations};
use super::connector_config::{ConfigChanges, Editable, RestChanges};
use super::connector_rules::{ToolPolicyInput, mcp_addition_problem};
use super::mail_server::MailChanges;
use super::mcp_config::mcp_config;
use super::policy_changes::policy_changes;
use super::{audit_entry, begin_change};
use crate::secrets::SecretStore;
use crate::services::AccessContext;
use crate::services::account_links::AccountLinkService;
use crate::services::connectors::{ConnectorService, client_secret_name, token_secret_name};

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
    /// Revokes the grants of the accounts linked at a connector deleted,
    /// or by a person disabled (ADR-194).
    pub account_links: &'a Arc<AccountLinkService>,
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
    let config = mcp_config(&input.prefix, &input.url, None);
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
    /// Whether the tools of a Microsoft 365 application, a Google Workspace
    /// client or the standard mail may write (ADR-119, ADR-123, ADR-205).
    pub write_tools: Option<bool>,
    /// The servers of the standard mail (ADR-205).
    pub mail: Option<MailChanges>,
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
        .and_then(|url| url_problem(url, admin.connectors.network()));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
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
    let mut oauth = None;
    let mut taken = None;
    let mail_servers = changes
        .mail
        .as_ref()
        .and_then(|mail| mail.audited(&instance.config));
    let configured = changes.url.is_some()
        || changes.rest.is_some()
        || changes.write_tools.is_some()
        || changes.mail.is_some();
    if configured || changes.token.is_some() || changes.tool_policies.is_some() {
        let Some(editable) = Editable::of(&instance) else {
            return Err(DomainError::validation(unchangeable(&instance)).into());
        };
        if editable.oauth().is_some() && matches!(changes.token, Some(TokenChange::Replace(_))) {
            return Err(DomainError::validation(
                "Ce serveur est déclaré avec OAuth : retirez la déclaration OAuth avant de saisir \
                 un jeton",
            )
            .into());
        }
        // Before any rule of a token: none is kept here, whatever it is.
        if editable.holds_no_secret() && changes.token.is_some() {
            return Err(DomainError::validation(
                "La messagerie standard n’a pas de jeton : chacun s’y connecte avec son propre \
                 compte",
            )
            .into());
        }
        if let Some(TokenChange::Replace(token)) = &changes.token {
            if let Some(problem) = token_problem(token) {
                return Err(DomainError::validation(problem).into());
            }
            admin.secrets.require()?;
        }
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
        if moved && editable.oauth().is_some() {
            // Taken before the secret goes, to be revoked under it.
            let declared = editable.oauth();
            taken = take_authorizations(&mut tx, admin, tenant_id, id, declared).await?;
            admin
                .secrets
                .delete(&mut tx, &client_secret_name(id))
                .await?;
            oauth = Some("removed");
        }
        let rehomed = header
            .as_ref()
            .is_some_and(|header| header.as_deref() != editable.token_header());
        patch.reachability_changed =
            moved || rehomed || changes.token.is_some() || mail_servers.is_some();
        if configured {
            let network = admin.connectors.network();
            let config_changes = ConfigChanges {
                url: changes.url.clone(),
                rest: changes.rest,
                mail: changes.mail,
                write_tools: changes.write_tools,
            };
            let config = editable.changed(config_changes, network)?;
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
        "oauth": oauth,
        "tokenHeader": header,
        "operations": operations,
        "writeTools": changes.write_tools,
        "mailServers": mail_servers,
        // Only what changed, a description only when it did (ADR-160).
        "toolPolicies": patch
            .tool_policies
            .as_ref()
            .and_then(|after| policy_changes(&instance.tool_policies, after)),
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
    if let Some(taken) = taken {
        revoke(admin, access, taken).await;
    }
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
            "Seuls les serveurs MCP, les API REST, les applications Microsoft 365 et Google \
             Workspace et la messagerie standard ajoutés ici se suppriment ici ; désactivez \
             plutôt ce connecteur",
        )
        .into());
    }
    // Its authorizations, the company's and each person's, taken before
    // the client's secret goes.
    let declared = Editable::of(&instance).and_then(|editable| editable.oauth().cloned());
    let tenant_id = access.subject.tenant_id;
    let taken = take_authorizations(&mut tx, admin, tenant_id, id, declared.as_ref()).await?;
    // The accounts linked there, taken while it is read, their grants
    // revoked once committed where its provider revokes one (ADR-194).
    let links = take_account_links_of_connector(&mut tx, id).await?;
    let links = admin.account_links.let_go_of(&mut tx, links).await?;
    delete_connector_instance(&mut tx, id).await?;
    for secret in [token_secret_name(id), client_secret_name(id)] {
        admin.secrets.delete(&mut tx, &secret).await?;
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.deleted", "connector", id).metadata(json!({
            "name": instance.name,
            "config": public_config(&instance),
        })),
    )
    .await?;
    tx.commit().await?;
    // The revocations' tasks started at once, before the kept instance is
    // let go: a request abandoned meanwhile leaves them going.
    let servers = async {
        if let Some(taken) = taken {
            revoke(admin, access, taken).await;
        }
    };
    let by = Some(access.subject.user_id);
    let accounts = admin.account_links.revoke(tenant_id, by, links);
    futures::join!(servers, accounts, admin.connectors.invalidate(id));
    Ok(())
}

/// What an audit entry may keep of a configuration: no environment of a
/// child process (it may carry credentials); the standard mail's servers,
/// in which no credential is kept.
fn public_config(instance: &ConnectorInstanceRow) -> Value {
    let config = &instance.config;
    if instance.connector_type == MAIL_SERVER {
        return json!({ "imap": config["imap"], "smtp": config["smtp"], "caldav": config["caldav"] });
    }
    json!({ "transport": config["transport"], "prefix": config["prefix"], "url": config["url"] })
}
