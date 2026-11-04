//! What an instance starts with (ADR-171): an MCP server declared with
//! OAuth is given the access token of its authorization, opened for it,
//! with when it expires, and never a token typed by hand; one not
//! authorized, or authorized under another declaration, none (it then says
//! it must be authorized). Any other connector is given its saved token.
//! Read in the caller's transaction, with the instance.

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use mimorya_connectors::mcp::McpConnectorConfig;
use mimorya_connectors::mcp::oauth::OAuthDeclaration;
use mimorya_db::TenantTx;
use mimorya_db::repositories::connector_authorizations::connector_access_token;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;

use super::{access_sealed_as, opened, token_secret_name};
use crate::secrets::SecretStore;

/// The token an instance starts with, if any, and when it expires.
#[derive(Default)]
pub struct Credential {
    pub token: Option<String>,
    /// If it says: an OAuth access token.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Credential {
    /// A token typed by an administrator: it does not say when it expires.
    pub fn typed(token: Option<String>) -> Self {
        Self {
            token,
            expires_at: None,
        }
    }
}

/// The instance's credential, or why it cannot be used; the outer error is
/// the database's.
pub async fn credential_of(
    tx: &mut TenantTx,
    secrets: &SecretStore,
    instance: &ConnectorInstanceRow,
) -> anyhow::Result<anyhow::Result<Credential>> {
    let tenant_id = instance.tenant_id;
    let Some(declared) = oauth_of(instance) else {
        let read = secrets
            .read(tx, tenant_id, &token_secret_name(instance.id))
            .await?;
        return Ok(opened(read).map(Credential::typed));
    };
    let Some(access) = connector_access_token(tx, instance.id).await? else {
        return Ok(Ok(Credential::default()));
    };
    // Issued under another declaration: not for this server as declared.
    if !given_under(
        &declared,
        &access.issuer,
        &access.client_id,
        &access.resource,
    ) {
        return Ok(Ok(Credential::default()));
    }
    let Ok(cipher) = secrets.cipher() else {
        return Ok(Err(anyhow!(
            "Ce serveur n’a pas de clé maîtresse : le jeton de l’autorisation ne peut pas être lu"
        )));
    };
    let opened = cipher.open(tenant_id, &access_sealed_as(instance.id), &access.sealed);
    Ok(match opened {
        Ok(token) => Ok(Credential {
            token: Some(token),
            expires_at: access.expires_at,
        }),
        Err(_) => Err(anyhow!(
            "Le jeton de l’autorisation OAuth ne peut pas être déchiffré : autorisez de nouveau \
             ce connecteur"
        )),
    })
}

/// Whether an authorization kept for this issuer, client and resource was
/// given under `declared`: its tokens are neither sent nor renewed under
/// another.
pub fn given_under(
    declared: &OAuthDeclaration,
    issuer: &str,
    client_id: &str,
    resource: &str,
) -> bool {
    declared.issuer == issuer && declared.client_id == client_id && declared.resource == resource
}

/// The OAuth declaration of an MCP server reached over HTTP, if it has one.
pub(super) fn oauth_of(instance: &ConnectorInstanceRow) -> Option<OAuthDeclaration> {
    if instance.connector_type != "mcp" {
        return None;
    }
    match serde_json::from_value(instance.config.clone()).ok()? {
        McpConnectorConfig::Http { oauth, .. } => oauth,
        McpConnectorConfig::Stdio { .. } => None,
    }
}
