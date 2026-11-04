//! The tokens of an authorization the administration lets go of, revoked at
//! its authorization server (RFC 7009, ADR-172). They are taken, with what
//! revoking them needs, in the transaction that withdraws the authorization,
//! before the client's secret goes with a declaration: the declaration it
//! was given under, if it still is the one in force. Once that transaction
//! is committed and nothing is held, they are sent to the revocation
//! endpoint the declaration names, the refresh token first, within a bound.
//! How it went is audited, never a token: a server that names no revocation
//! endpoint, does not answer or refuses keeps its grant, to be withdrawn
//! there.

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{
    ClientAuth, HeldToken, NO_REVOCATION_ENDPOINT, OAuthDeclaration, revoke_tokens,
};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connector_authorizations::{
    TakenAuthorization, take_connector_authorization,
};
use uuid::Uuid;

use super::super::connectors::ConnectorAdmin;
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::connectors::{
    access_sealed_as, client_secret_name, given_under, refresh_sealed_as,
};

/// How long the revocation endpoint is waited for, all tokens together.
const REVOCATION_BOUND: Duration = Duration::from_secs(10);

/// What the audit calls a revocation asked of the authorization server.
const AUTHORIZATION_REVOKED: &str = "connector.oauth_authorization_revoked";

/// An authorization taken away, with what revoking its tokens needs.
pub struct Revocation {
    connector_id: Uuid,
    /// The declaration it was given under, if it still was the one in force.
    declared: Option<OAuthDeclaration>,
    secret: Option<String>,
    taken: TakenAuthorization,
}

/// Takes the connector's authorization in `tx`, if it has one, with what
/// revoking it needs under `declared`, the declaration in force before the
/// change: to be called before the client's secret is replaced or removed.
pub async fn take_authorization(
    tx: &mut TenantTx,
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    connector_id: Uuid,
    declared: Option<&OAuthDeclaration>,
) -> anyhow::Result<Option<Revocation>> {
    let Some(taken) = take_connector_authorization(tx, connector_id).await? else {
        return Ok(None);
    };
    // Never sent under another declaration than the one it was given under.
    let declared = declared
        .filter(|declared| given_under(declared, &taken.issuer, &taken.client_id, &taken.resource))
        .cloned();
    let secret = match &declared {
        Some(declared) if declared.client_auth != ClientAuth::None => {
            let name = client_secret_name(connector_id);
            match admin.secrets.read(tx, tenant_id, &name).await? {
                SecretRead::Value(secret) => Some(secret),
                _ => None,
            }
        }
        _ => None,
    };
    Ok(Some(Revocation {
        connector_id,
        declared,
        secret,
        taken,
    }))
}

/// Asks the authorization server to revoke what was taken, once the
/// withdrawal is committed; audited, never a token. An error only when
/// the audit cannot be written.
pub async fn revoke(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    revocation: Revocation,
) -> anyhow::Result<()> {
    let tenant_id = access.subject.tenant_id;
    let id = revocation.connector_id;
    let outcome = match &revocation.declared {
        None => Err(
            "Autorisation donnée sous une autre déclaration : retirez l’accord chez le serveur \
             d’autorisation"
                .to_string(),
        ),
        Some(declared) if declared.revocation_endpoint.is_none() => {
            Err(NO_REVOCATION_ENDPOINT.to_string())
        }
        Some(declared) => send(admin, tenant_id, &revocation, declared).await,
    };
    let entry = |outcome: &'static str| {
        AuditEntry::new(tenant_id, "user", AUTHORIZATION_REVOKED, outcome)
            .by_user(access.subject.user_id)
            .resource("connector", id)
    };
    let entry = match outcome {
        Ok(()) => entry("success"),
        Err(reason) => entry("failure").reason(reason.chars().take(300).collect::<String>()),
    };
    let mut tx = admin.db.begin(tenant_id).await?;
    append_audit(&mut tx, entry).await?;
    tx.commit().await?;
    Ok(())
}

/// The tokens opened and sent, the refresh token first.
async fn send(
    admin: &ConnectorAdmin<'_>,
    tenant_id: Uuid,
    revocation: &Revocation,
    declared: &OAuthDeclaration,
) -> Result<(), String> {
    let unreadable = || {
        "Jetons illisibles pour la plateforme : retirez l’accord chez le \
                         serveur d’autorisation"
            .to_string()
    };
    let cipher = admin.secrets.cipher().map_err(|_| unreadable())?;
    let id = revocation.connector_id;
    let taken = &revocation.taken;
    let access = cipher
        .open(tenant_id, &access_sealed_as(id), &taken.access)
        .map_err(|_| unreadable())?;
    let refresh = match &taken.refresh {
        Some(sealed) => Some(
            cipher
                .open(tenant_id, &refresh_sealed_as(id), sealed)
                .map_err(|_| unreadable())?,
        ),
        None => None,
    };
    let mut held = Vec::new();
    if let Some(refresh) = &refresh {
        held.push(HeldToken {
            token: refresh,
            refresh: true,
        });
    }
    held.push(HeldToken {
        token: &access,
        refresh: false,
    });
    let network = admin.connectors.network();
    let secret = revocation.secret.as_deref();
    let asked = revoke_tokens(declared, &held, secret, network);
    match tokio::time::timeout(REVOCATION_BOUND, asked).await {
        Ok(outcome) => outcome,
        Err(_) => Err("Le point de révocation n’a pas répondu à temps".to_string()),
    }
}
