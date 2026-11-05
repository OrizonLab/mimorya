//! What an instance starts with (ADR-171): an MCP server declared with
//! OAuth for the company is given the access token of its authorization,
//! opened for it, with when it expires, and never a token typed by hand;
//! one not authorized, or authorized under another declaration, none (it
//! then says it must be authorized); one each person authorizes for
//! themself, never the company's (ADR-174), but, for that person, their own
//! authorization's (`credential_for`), never another's. Any other connector
//! is given its saved token. Read in the caller's transaction, with the
//! instance.

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use mimorya_connectors::mcp::McpConnectorConfig;
use mimorya_connectors::mcp::oauth::{OAuthDeclaration, OAuthHolder};
use mimorya_db::TenantTx;
use mimorya_db::repositories::connector_authorizations::{
    ConnectorAccessToken, connector_access_token,
};
use mimorya_db::repositories::connector_person_authorizations::person_access_token;
use mimorya_db::repositories::connectors::ConnectorInstanceRow;
use uuid::Uuid;

use super::{access_sealed_as, opened, person_access_sealed_as, token_secret_name};
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
    // Each person authorizes it for themself (ADR-174): never the
    // company's authorization, whatever is kept.
    if declared.holder == OAuthHolder::Person {
        return Ok(Ok(Credential::default()));
    }
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
    let sealed_as = access_sealed_as(instance.id);
    Ok(open_access(
        secrets,
        tenant_id,
        &sealed_as,
        access,
        "ce connecteur",
    ))
}

/// What an instance starts with for one person (ADR-174).
pub enum CredentialFor {
    /// The same for everyone: a token typed by an administrator, or the
    /// company's authorization (`credential_of`).
    Shared(anyhow::Result<Credential>),
    /// The person's own authorization of a server each person authorizes,
    /// or why it cannot be used, and when it was given or last renewed:
    /// their session follows it (ADR-174).
    Own {
        credential: anyhow::Result<Credential>,
        since: DateTime<Utc>,
    },
    /// A server each person authorizes, which this person has not, or not
    /// under its declaration: nothing to start it with.
    NotAuthorized,
}

/// The instance's credential for `user_id`: on a server each person
/// authorizes, their own authorization's access token and never another
/// person's nor the company's; on any other, the shared one. The outer
/// error is the database's.
pub async fn credential_for(
    tx: &mut TenantTx,
    secrets: &SecretStore,
    instance: &ConnectorInstanceRow,
    user_id: Uuid,
) -> anyhow::Result<CredentialFor> {
    let declared = match oauth_of(instance) {
        Some(declared) if declared.holder == OAuthHolder::Person => declared,
        _ => {
            return Ok(CredentialFor::Shared(
                credential_of(tx, secrets, instance).await?,
            ));
        }
    };
    let Some(access) = person_access_token(tx, instance.id, user_id).await? else {
        return Ok(CredentialFor::NotAuthorized);
    };
    if !given_under(
        &declared,
        &access.issuer,
        &access.client_id,
        &access.resource,
    ) {
        return Ok(CredentialFor::NotAuthorized);
    }
    let sealed_as = person_access_sealed_as(instance.id, user_id);
    let (tenant_id, since) = (instance.tenant_id, access.refreshed_at);
    let again = "ce serveur dans « Mon compte »";
    Ok(CredentialFor::Own {
        credential: open_access(secrets, tenant_id, &sealed_as, access, again),
        since,
    })
}

/// Whether each person authorizes the instance for themself (ADR-174).
pub fn each_person(instance: &ConnectorInstanceRow) -> bool {
    oauth_of(instance).is_some_and(|declared| declared.holder == OAuthHolder::Person)
}

/// The access token kept, opened under the name it was sealed as, with
/// when it expires; `again` says what to authorize again if it cannot be.
fn open_access(
    secrets: &SecretStore,
    tenant_id: Uuid,
    sealed_as: &str,
    access: ConnectorAccessToken,
    again: &str,
) -> anyhow::Result<Credential> {
    let Ok(cipher) = secrets.cipher() else {
        return Err(anyhow!(
            "Ce serveur n’a pas de clé maîtresse : le jeton de l’autorisation ne peut pas être lu"
        ));
    };
    match cipher.open(tenant_id, sealed_as, &access.sealed) {
        Ok(token) => Ok(Credential {
            token: Some(token),
            expires_at: access.expires_at,
        }),
        Err(_) => Err(anyhow!(
            "Le jeton de l’autorisation OAuth ne peut pas être déchiffré : autorisez de nouveau \
             {again}"
        )),
    }
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
