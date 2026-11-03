//! OAuth declared on an MCP server by a company-wide connector
//! administrator (ADR-171), and withdrawn.
//!
//! The declaration first finds where the server is authorized (the
//! discovery, outside any transaction, under a slot of the company's
//! administration starts); then, under the administration lock, it keeps
//! what was found in the connector's configuration if the server's address
//! has not changed meanwhile. The client's secret is sealed apart, never
//! shown nor audited. A typed token goes: OAuth replaces it. Declared
//! again, everything is replaced, secret included. Until an administrator
//! authorizes it (lot 40d), the connector starts no session. A declaration
//! that can no longer be read is replaced or withdrawn all the same.

use mimorya_connectors::mcp::oauth::{
    OAuthDeclaration, client_id_problem, client_secret_problem, discover, scopes_problem,
};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, ConnectorPatch, get_connector_instance, update_connector_instance,
};
use serde_json::json;
use uuid::Uuid;

use super::connector_config::Editable;
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::mcp_config::{mcp_config, oauth_unreadable};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::{client_secret_name, token_secret_name};

/// What an administrator declares.
pub struct OAuthInput {
    /// The authorization server's issuer, if the administrator names it.
    pub issuer: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    /// Those to ask for; the server's suggestions if `None`.
    pub scopes: Option<Vec<String>>,
}

/// An MCP server added here, as its OAuth sees it.
struct McpServer {
    prefix: String,
    url: String,
    declared: Option<OAuthDeclaration>,
    /// A declaration is stored but cannot be read: it is replaced or
    /// withdrawn all the same.
    unreadable: bool,
}

impl McpServer {
    fn of(instance: Option<ConnectorInstanceRow>) -> Result<Self, DomainError> {
        let instance = instance.ok_or_else(|| DomainError::not_found("Connecteur introuvable"))?;
        if let Some(Editable::Mcp { prefix, url, oauth }) = Editable::of(&instance) {
            return Ok(Self {
                prefix,
                url,
                declared: oauth,
                unreadable: false,
            });
        }
        if oauth_unreadable(&instance) {
            let text = |key: &str| {
                instance.config[key]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            };
            return Ok(Self {
                prefix: text("prefix"),
                url: text("url"),
                declared: None,
                unreadable: true,
            });
        }
        Err(DomainError::validation(
            "Seul un serveur MCP ajouté ici s’autorise par OAuth",
        ))
    }
}

pub async fn declare_oauth(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    input: OAuthInput,
) -> anyhow::Result<OAuthDeclaration> {
    access.require_tenant_wide(PERMISSION)?;
    let problem = client_id_problem(&input.client_id)
        .or_else(|| {
            input
                .client_secret
                .as_deref()
                .and_then(client_secret_problem)
        })
        .or_else(|| input.scopes.as_deref().and_then(scopes_problem));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    if input.client_secret.is_some() {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    // The address asked, read before anything is asked of it.
    let mut tx = admin.db.begin(tenant_id).await?;
    let url = McpServer::of(get_connector_instance(&mut tx, id).await?)?.url;
    tx.commit().await?;
    let found = {
        let _start = admin.connectors.claim_admin_start(tenant_id)?;
        let network = admin.connectors.network();
        discover(&url, input.issuer.as_deref(), network)
            .await
            .map_err(DomainError::validation)?
    };
    let has_secret = input.client_secret.is_some();
    let declaration = OAuthDeclaration::new(found, &input.client_id, has_secret, input.scopes)
        .map_err(DomainError::validation)?;

    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let current = McpServer::of(get_connector_instance(&mut tx, id).await?)?;
    if current.url != url {
        return Err(DomainError::conflict(
            "L’adresse du serveur a changé pendant la recherche de son autorisation : recommencez",
        )
        .into());
    }
    let patch = ConnectorPatch {
        config: Some(mcp_config(&current.prefix, &url, Some(&declaration))),
        reachability_changed: true,
        ..ConnectorPatch::default()
    };
    update_connector_instance(&mut tx, id, patch).await?;
    // OAuth replaces a typed token.
    let token_removed = admin
        .secrets
        .delete(&mut tx, &token_secret_name(id))
        .await?;
    let secret_name = client_secret_name(id);
    match &input.client_secret {
        Some(secret) => {
            admin
                .secrets
                .put(&mut tx, tenant_id, &secret_name, secret)
                .await?;
        }
        None => {
            admin.secrets.delete(&mut tx, &secret_name).await?;
        }
    }
    append_audit(
        &mut tx,
        audit_entry(access, "connector.oauth_declared", "connector", id).metadata(json!({
            "issuer": declaration.issuer,
            "resource": declaration.resource,
            "clientId": declaration.client_id,
            "clientAuth": declaration.client_auth,
            "scopes": declaration.scopes,
            "clientSecretStored": has_secret,
            "tokenRemoved": token_removed,
            "replaced": current.declared.is_some() || current.unreadable,
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    Ok(declaration)
}

pub async fn remove_oauth(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
) -> anyhow::Result<()> {
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = begin_change(admin.db, access, PERMISSION).await?;
    let server = McpServer::of(get_connector_instance(&mut tx, id).await?)?;
    if server.declared.is_none() && !server.unreadable {
        return Err(DomainError::validation(
            "Aucune autorisation OAuth n’est déclarée sur ce serveur",
        )
        .into());
    }
    let patch = ConnectorPatch {
        config: Some(mcp_config(&server.prefix, &server.url, None)),
        reachability_changed: true,
        ..ConnectorPatch::default()
    };
    update_connector_instance(&mut tx, id, patch).await?;
    admin
        .secrets
        .delete(&mut tx, &client_secret_name(id))
        .await?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.oauth_removed", "connector", id).metadata(json!({
            "issuer": server.declared.as_ref().map(|d| &d.issuer),
            "clientId": server.declared.as_ref().map(|d| &d.client_id),
            "unreadable": server.unreadable,
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    Ok(())
}
