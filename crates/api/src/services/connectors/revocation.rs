//! Tokens the platform lets go of, revoked at the authorization server that
//! granted them (RFC 7009, ADR-172), within a bound, the refresh token
//! first, under the declaration they were asked under: those of an
//! authorization the administration withdraws (`revoke_within`), and those
//! granted that no one keeps (`revoke_unkept`): a renewal's for an
//! authorization withdrawn meanwhile, an authorization's whose return found
//! its declaration changed. These go in a task of their own, which no one
//! waits for, and not when an authorization of the same client at the same
//! server is held for the connector, or for the same person when they were
//! a person's (ADR-174): revoking them could take it with them at some
//! servers. Why one was not revoked is said, never with a token.

use std::time::Duration;

use mimorya_connectors::mcp::oauth::{Granted, HeldToken, OAuthDeclaration, revoke_tokens};
use mimorya_connectors::outbound::NetworkPolicy;
use mimorya_db::Database;
use mimorya_db::repositories::connector_authorizations::get_connector_authorization;
use mimorya_db::repositories::connector_person_authorizations::get_person_authorization;
use uuid::Uuid;

use super::ConnectorService;

/// How long the revocation endpoint is waited for, all tokens together.
pub const REVOCATION_BOUND: Duration = Duration::from_secs(10);

/// Said when the revocation endpoint did not answer within the bound.
const NOT_IN_TIME: &str = "Le point de révocation n’a pas répondu à temps";

/// The tokens of one grant, as they are let go of.
pub struct LetGo<'a> {
    pub access: &'a str,
    pub refresh: Option<&'a str>,
}

/// Asks the revocation endpoint `declared` names to revoke `tokens`, the
/// refresh token first, within `REVOCATION_BOUND`.
pub async fn revoke_within(
    declared: &OAuthDeclaration,
    tokens: LetGo<'_>,
    client_secret: Option<&str>,
    network: NetworkPolicy,
) -> Result<(), String> {
    let mut held = Vec::new();
    if let Some(refresh) = tokens.refresh {
        held.push(HeldToken {
            token: refresh,
            refresh: true,
        });
    }
    held.push(HeldToken {
        token: tokens.access,
        refresh: false,
    });
    let asked = revoke_tokens(declared, &held, client_secret, network);
    match tokio::time::timeout(REVOCATION_BOUND, asked).await {
        Ok(outcome) => outcome,
        Err(_) => Err(NOT_IN_TIME.to_string()),
    }
}

/// Tokens granted for a connector that no one keeps.
pub struct Unkept {
    pub tenant_id: Uuid,
    pub connector_id: Uuid,
    /// The person they were granted to, for themself (ADR-174); `None`, the
    /// company.
    pub user_id: Option<Uuid>,
    /// The declaration they were asked under.
    pub declared: OAuthDeclaration,
    pub granted: Granted,
    pub client_secret: Option<String>,
}

impl ConnectorService {
    /// Revokes `unkept` in a task of its own, unless an authorization of
    /// the same client at the same server is held for its connector.
    pub(crate) fn revoke_unkept(&self, unkept: Unkept) {
        // Nowhere to send them: nothing to wait for.
        if unkept.declared.revocation_endpoint.is_none() {
            return;
        }
        let (db, network) = (self.db.clone(), self.settings.network);
        tokio::spawn(revoke_if_unheld(db, network, unkept));
    }
}

async fn revoke_if_unheld(db: Database, network: NetworkPolicy, unkept: Unkept) {
    let id = unkept.connector_id;
    // The authorization they would take with them: the company's, or that
    // same person's.
    let held = async {
        let mut tx = db.begin(unkept.tenant_id).await?;
        let held = match unkept.user_id {
            None => get_connector_authorization(&mut tx, id)
                .await?
                .map(|held| (held.issuer, held.client_id)),
            Some(user_id) => get_person_authorization(&mut tx, id, user_id)
                .await?
                .map(|held| (held.issuer, held.client_id)),
        };
        tx.commit().await?;
        anyhow::Ok(held)
    };
    let declared = &unkept.declared;
    match held.await {
        Ok(Some((issuer, client_id)))
            if issuer == declared.issuer && client_id == declared.client_id =>
        {
            return;
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                connector = %id,
                error = format!("{error:#}"),
                "Tokens no one keeps were not revoked: their connector's authorization unread"
            );
            return;
        }
    }
    let tokens = LetGo {
        access: &unkept.granted.access_token,
        refresh: unkept.granted.refresh_token.as_deref(),
    };
    let secret = unkept.client_secret.as_deref();
    if let Err(reason) = revoke_within(declared, tokens, secret, network).await {
        tracing::warn!(
            connector = %id,
            reason,
            "Tokens no one keeps were not revoked"
        );
    }
}
