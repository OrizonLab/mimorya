//! OAuth declared on an MCP server by a company-wide connector
//! administrator (ADR-171), and withdrawn.
//!
//! The declaration first finds where the server is authorized (the
//! discovery, outside any transaction, under a slot of the company's
//! administration starts); then, under the administration lock, it keeps
//! what was found in the connector's configuration if the server's address
//! has not changed meanwhile. The client is the one the administrator
//! typed, or the platform's own, registered at the authorization server
//! once it is found (`oauth_client`, ADR-173). The client's secret is sealed apart, never
//! shown nor audited. A typed token goes: OAuth replaces it. Declared
//! again, everything is replaced, secret included, and the authorization
//! given under the previous declaration goes (`connector_authorization`),
//! the company's and each person's (ADR-174). The declaration says who
//! authorizes the server: the company, or each person for themself. A
//! declaration that can no longer be read is replaced or withdrawn all the
//! same.

use mimorya_connectors::mcp::oauth::{OAuthDeclaration, OAuthHolder, discover, scopes_problem};
use mimorya_core::DomainError;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connectors::{
    ConnectorInstanceRow, ConnectorPatch, get_connector_instance, update_connector_instance,
};
use serde_json::json;
use uuid::Uuid;

use super::connector_authorization::{revocable_under, revoke, take_authorizations};
use super::connectors::{ConnectorAdmin, PERMISSION};
use super::mcp_config::{Unreadable, mcp_config, stored_oauth};
use super::oauth_client::{Declared, OAuthClient, declared};
use super::{audit_entry, begin_change};
use crate::services::AccessContext;
use crate::services::connectors::{client_secret_name, token_secret_name};

/// What an administrator declares.
pub struct OAuthInput {
    /// The authorization server's issuer, if the administrator names it.
    pub issuer: Option<String>,
    /// Whether the platform registers its own client, rather than one
    /// typed (ADR-173).
    pub register: bool,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    /// Those to ask for; the server's suggestions if `None`.
    pub scopes: Option<Vec<String>>,
    /// The platform's return address, given a registered client.
    pub redirect_uri: String,
    /// Who authorizes the server (ADR-174).
    pub holder: OAuthHolder,
}

/// An MCP server added here, as its OAuth sees it.
pub(super) struct McpServer {
    pub prefix: String,
    pub url: String,
    pub declared: Option<OAuthDeclaration>,
    /// A declaration is stored but cannot be read: it is replaced or
    /// withdrawn all the same.
    pub unreadable: bool,
}

impl McpServer {
    pub fn of(instance: Option<ConnectorInstanceRow>) -> Result<Self, DomainError> {
        let instance = instance.ok_or_else(|| DomainError::not_found("Connecteur introuvable"))?;
        let config = &instance.config;
        if instance.connector_type != "mcp" || config["transport"] != "http" {
            return Err(DomainError::validation(
                "Seul un serveur MCP ajouté ici s’autorise par OAuth",
            ));
        }
        let text = |key: &str| config[key].as_str().unwrap_or_default().to_string();
        let (declared, unreadable) = match stored_oauth(config) {
            Ok(declared) => (declared, false),
            Err(Unreadable) => (None, true),
        };
        Ok(Self {
            prefix: text("prefix"),
            url: text("url"),
            declared,
            unreadable,
        })
    }
}

pub async fn declare_oauth(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    input: OAuthInput,
) -> anyhow::Result<OAuthDeclaration> {
    access.require_tenant_wide(PERMISSION)?;
    let client = OAuthClient::chosen(input.register, input.client_id, input.client_secret)?;
    let problem = client
        .problem()
        .or_else(|| input.scopes.as_deref().and_then(scopes_problem));
    if let Some(problem) = problem {
        return Err(DomainError::validation(problem).into());
    }
    // Before anything is sent: nothing registered that could not be kept.
    if client.needs_sealing() {
        admin.secrets.require()?;
    }
    let tenant_id = access.subject.tenant_id;
    // The address asked, read before anything is asked of it.
    let mut tx = admin.db.begin(tenant_id).await?;
    let url = McpServer::of(get_connector_instance(&mut tx, id).await?)?.url;
    tx.commit().await?;
    // Found, and the platform's client registered there if it is asked to.
    let (mut declaration, client_secret) = {
        let _start = admin.connectors.claim_admin_start(tenant_id)?;
        let network = admin.connectors.network();
        let found = discover(&url, input.issuer.as_deref(), network)
            .await
            .map_err(DomainError::validation)?;
        let with = Declared {
            scopes: input.scopes,
            redirect_uri: &input.redirect_uri,
            network,
        };
        declared(found, client, with)
            .await
            .map_err(DomainError::validation)?
    };
    declaration.holder = input.holder;
    let has_secret = client_secret.is_some();

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
    // An authorization given under another declaration does not follow:
    // taken before the former secret is replaced, to be revoked under it,
    // at the revocation endpoint its server now names if it named none.
    let previous = current
        .declared
        .as_ref()
        .map(|previous| revocable_under(previous, &declaration));
    let taken = take_authorizations(&mut tx, admin, tenant_id, id, previous.as_ref()).await?;
    let authorization_removed = taken.as_ref().is_some_and(|t| t.company_taken());
    let persons_removed = taken.as_ref().map_or(0, |t| t.persons_taken());
    // OAuth replaces a typed token.
    let token_removed = admin
        .secrets
        .delete(&mut tx, &token_secret_name(id))
        .await?;
    let secret_name = client_secret_name(id);
    match &client_secret {
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
            "registered": declaration.registered.is_some(),
            "tokenRemoved": token_removed,
            "replaced": current.declared.is_some() || current.unreadable,
            "perPerson": declaration.holder == OAuthHolder::Person,
            "authorizationRemoved": authorization_removed,
            "personAuthorizationsRemoved": persons_removed,
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    if let Some(taken) = taken {
        revoke(admin, access, taken).await;
    }
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
    let tenant_id = access.subject.tenant_id;
    let declared = server.declared.as_ref();
    let taken = take_authorizations(&mut tx, admin, tenant_id, id, declared).await?;
    let authorization_removed = taken.as_ref().is_some_and(|t| t.company_taken());
    let persons_removed = taken.as_ref().map_or(0, |t| t.persons_taken());
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
            "authorizationRemoved": authorization_removed,
            "personAuthorizationsRemoved": persons_removed,
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    if let Some(taken) = taken {
        revoke(admin, access, taken).await;
    }
    Ok(())
}
