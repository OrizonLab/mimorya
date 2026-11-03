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
//! authorizes it (lot 40d), the connector starts no session.

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

use super::connector_config::{Editable, mcp_config};
use super::connectors::{ConnectorAdmin, PERMISSION};
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

/// The MCP server `instance` is, with its prefix, address and declaration.
fn mcp_server(
    instance: Option<ConnectorInstanceRow>,
) -> Result<(String, String, Option<OAuthDeclaration>), DomainError> {
    let instance = instance.ok_or_else(|| DomainError::not_found("Connecteur introuvable"))?;
    match Editable::of(&instance) {
        Some(Editable::Mcp { prefix, url, oauth }) => Ok((prefix, url, oauth)),
        _ => Err(DomainError::validation(
            "Seul un serveur MCP ajouté ici s’autorise par OAuth",
        )),
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
    let (_, url, _) = mcp_server(get_connector_instance(&mut tx, id).await?)?;
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
    let (prefix, current, previous) = mcp_server(get_connector_instance(&mut tx, id).await?)?;
    if current != url {
        return Err(DomainError::conflict(
            "L’adresse du serveur a changé pendant la recherche de son autorisation : recommencez",
        )
        .into());
    }
    let patch = ConnectorPatch {
        config: Some(mcp_config(&prefix, &url, Some(&declaration))),
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
            "replaced": previous.is_some(),
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
    let (prefix, url, declared) = mcp_server(get_connector_instance(&mut tx, id).await?)?;
    let Some(declared) = declared else {
        return Err(DomainError::validation(
            "Aucune autorisation OAuth n’est déclarée sur ce serveur",
        )
        .into());
    };
    let patch = ConnectorPatch {
        config: Some(mcp_config(&prefix, &url, None)),
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
            "issuer": declared.issuer,
            "clientId": declared.client_id,
        })),
    )
    .await?;
    tx.commit().await?;
    admin.connectors.invalidate(id).await;
    Ok(())
}
