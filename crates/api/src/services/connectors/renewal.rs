//! The access tokens of MCP servers' OAuth authorizations renewed before
//! they expire (ADR-171). Before a company's instances are read for a run,
//! an enabled connector's authorization whose token expires within
//! `RENEW_WITHIN` is renewed with its refresh token; before the
//! administration tests a connector, whatever its state, so is its own.
//! Its renewal is first claimed for `RENEWAL_HOLD` by one short statement:
//! another process skips it and starts with the current token, which still
//! lasts long enough if the claim was taken more than `MIN_TOKEN_LIFE`
//! before it expires (the first run that finds it due takes it; otherwise
//! runs meanwhile go without the connector). Nothing is held while the
//! token endpoint is called (bounded in time), and a token endpoint that did
//! not answer is not asked again before the claim ends. What it brings is
//! then kept in a short transaction, under the company's administration lock
//! (so that it takes its locks in the administration's order), only if the
//! authorization is still the one claimed: sealed for the connector, a
//! rotated refresh token kept, the connector's version changed. A server
//! saying the grant is gone removes it (audited by the system, never a
//! token): an administrator authorizes the server again. Tokens it brings
//! for an authorization withdrawn or replaced meanwhile, which no one keeps,
//! are revoked (`revocation`, ADR-172). Any other failure of the token endpoint, or a refresh token
//! the platform can no longer open, is said on the authorization, for its
//! administrators, until a renewal is kept.

use std::time::Duration;

use anyhow::anyhow;
use futures::future::join_all;
use mimorya_connectors::mcp::RENEW_WITHIN;
use mimorya_connectors::mcp::oauth::{ClientAuth, OAuthHolder, RenewalFailure, refresh_tokens};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connector_renewals::{
    Claimed, RenewalFor, Renewed, claim_connector_renewal, connector_authorizations_due,
    keep_connector_renewal, record_renewal_failure, remove_claimed_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::credential::{given_under, oauth_of};
use super::grants::{AUTHORIZATION_REMOVED, seal_grant};
use super::revocation::Unkept;
use super::{ConnectorService, client_secret_name, refresh_sealed_as};
use crate::secrets::SecretRead;

/// What a refresh token the platform can no longer open says: sealed under
/// another master key, or the master key gone.
const REFRESH_UNREADABLE: &str = "Jeton de renouvellement illisible pour la plateforme (clé de \
                                  chiffrement changée ?) : autorisez de nouveau ce connecteur";

/// How long a renewal stays claimed: longer than a call to the token
/// endpoint may take, and the delay before one that did not answer is
/// asked again.
const RENEWAL_HOLD: Duration = Duration::from_secs(60);

/// What a claimed renewal needs to call the token endpoint.
struct Call {
    claimed: Claimed,
    declared: mimorya_connectors::mcp::oauth::OAuthDeclaration,
    secret: Option<String>,
}

impl ConnectorService {
    /// Renews the company's authorizations about to expire; one that fails
    /// is said in the logs, without a token, and does not stop the others.
    pub(crate) async fn renew_due(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let due = connector_authorizations_due(&mut tx, RENEW_WITHIN).await?;
        tx.commit().await?;
        // Side by side: each bounded in time, none waits for another.
        let renewals = due
            .into_iter()
            .map(|id| self.renew_if_due(tenant_id, id, RenewalFor::Runs));
        join_all(renewals).await;
        Ok(())
    }

    /// Renews the connector's authorization if it is about to expire and no
    /// other renewal of it is under way; for runs, only if it is enabled.
    pub(crate) async fn renew_if_due(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        asker: RenewalFor,
    ) {
        if let Err(error) = self.renew(tenant_id, connector_id, asker).await {
            tracing::warn!(
                connector = %connector_id,
                error = format!("{error:#}"),
                "Failed to renew a connector's OAuth authorization"
            );
        }
    }

    async fn renew(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        asker: RenewalFor,
    ) -> anyhow::Result<()> {
        let Some(call) = self.claim(tenant_id, connector_id, asker).await? else {
            return Ok(());
        };
        let opened = self.secrets.cipher().ok().and_then(|cipher| {
            let name = refresh_sealed_as(connector_id);
            cipher.open(tenant_id, &name, &call.claimed.refresh).ok()
        });
        // Said to its administrators: it will not open by waiting.
        let Some(refresh) = opened else {
            let mut tx = self.db.begin(tenant_id).await?;
            record_renewal_failure(&mut tx, call.claimed.key, REFRESH_UNREADABLE).await?;
            tx.commit().await?;
            anyhow::bail!("refresh token not opened for its connector");
        };
        let network = self.settings.network;
        let renewed = refresh_tokens(&call.declared, &refresh, call.secret.as_deref(), network);
        match renewed.await {
            Ok(granted) => {
                let cipher = self.secrets.cipher().map_err(|e| anyhow!("{e}"))?;
                let sealed = seal_grant(cipher, tenant_id, connector_id, &granted)?;
                let mut tx = self.db.begin(tenant_id).await?;
                tx.lock_administration().await?;
                let renewed = Renewed {
                    access_token: &sealed.access,
                    access_expires_at: sealed.expires_at,
                    refresh_token: sealed.refresh.as_ref(),
                    scopes: sealed.scopes.as_deref(),
                };
                let kept = keep_connector_renewal(&mut tx, call.claimed.key, renewed).await?;
                tx.commit().await?;
                // Withdrawn or replaced meanwhile: no one keeps these.
                if kept.is_none() {
                    self.revoke_unkept(Unkept {
                        tenant_id,
                        connector_id,
                        user_id: None,
                        declared: call.declared,
                        granted,
                        client_secret: call.secret,
                    });
                }
            }
            Err(RenewalFailure::Refused(reason)) => {
                let mut tx = self.db.begin(tenant_id).await?;
                tx.lock_administration().await?;
                if remove_claimed_authorization(&mut tx, call.claimed.key).await? {
                    let reason = format!("Autorisation retirée au renouvellement : {reason}");
                    let entry =
                        AuditEntry::new(tenant_id, "system", AUTHORIZATION_REMOVED, "success")
                            .resource("connector", connector_id)
                            .reason(reason.chars().take(300).collect::<String>());
                    append_audit(&mut tx, entry).await?;
                }
                tx.commit().await?;
            }
            // Said to its administrators, asked again once the claim ends.
            Err(RenewalFailure::Unavailable(reason)) => {
                tracing::warn!(
                    connector = %connector_id,
                    reason,
                    "A connector's authorization server could not renew its token"
                );
                let mut tx = self.db.begin(tenant_id).await?;
                record_renewal_failure(&mut tx, call.claimed.key, &reason).await?;
                tx.commit().await?;
            }
        }
        Ok(())
    }

    /// The renewal claimed, with what calling the token endpoint needs, if
    /// it is due, not claimed already, and still under the declaration in
    /// force.
    async fn claim(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        asker: RenewalFor,
    ) -> anyhow::Result<Option<Call>> {
        let mut tx = self.db.begin(tenant_id).await?;
        let claim =
            claim_connector_renewal(&mut tx, connector_id, RENEW_WITHIN, RENEWAL_HOLD, asker);
        let Some(claimed) = claim.await? else {
            return Ok(None);
        };
        let instance = get_connector_instance(&mut tx, connector_id).await?;
        let declared = instance.as_ref().and_then(oauth_of).filter(|declared| {
            declared.holder == OAuthHolder::Company
                && given_under(
                    declared,
                    &claimed.issuer,
                    &claimed.client_id,
                    &claimed.resource,
                )
        });
        // Never sent under another declaration than the one it was given
        // under, nor under one each person authorizes (ADR-174): the
        // company's would never be used there.
        let Some(declared) = declared else {
            tx.commit().await?;
            return Ok(None);
        };
        let secret = match declared.client_auth {
            ClientAuth::None => None,
            _ => match self
                .secrets
                .read(&mut tx, tenant_id, &client_secret_name(connector_id))
                .await?
            {
                SecretRead::Value(secret) => Some(secret),
                _ => None,
            },
        };
        tx.commit().await?;
        Ok(Some(Call {
            claimed,
            declared,
            secret,
        }))
    }
}
