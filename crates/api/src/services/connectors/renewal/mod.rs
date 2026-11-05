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
//! administrators, until a renewal is kept. A person's own authorization of
//! a server each person authorizes (ADR-174) is renewed the same way, before
//! their runs read their instances and before their own test: claimed,
//! kept, said or removed under their name, sealed for them, the removal
//! audited by the system with whose it was; it makes no new version of the
//! connector (their session follows their authorization), and a removal
//! releases their session here.

use std::time::Duration;

use anyhow::anyhow;
use futures::future::join_all;
use mimorya_connectors::mcp::RENEW_WITHIN;
use mimorya_connectors::mcp::oauth::{Granted, RenewalFailure, refresh_tokens};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_person_renewals::person_authorizations_due;
use mimorya_db::repositories::connector_renewals::{
    RenewalFor, Renewed, connector_authorizations_due,
};
use uuid::Uuid;

use super::ConnectorService;
use super::grants::{seal_grant, seal_person_grant};
use super::revocation::Unkept;
use claim::Call;

mod claim;
mod whose;

pub(crate) use whose::Whose;

/// How long a renewal stays claimed: longer than a call to the token
/// endpoint may take, and the delay before one that did not answer is
/// asked again.
const RENEWAL_HOLD: Duration = Duration::from_secs(60);

impl ConnectorService {
    /// Renews the company's authorizations about to expire; one that fails
    /// is said in the logs, without a token, and does not stop the others.
    pub(crate) async fn renew_due(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let due = connector_authorizations_due(&mut tx, RENEW_WITHIN).await?;
        tx.commit().await?;
        self.renew_all(tenant_id, due, Whose::Company).await;
        Ok(())
    }

    /// Renews the person's own authorizations about to expire, before their
    /// run reads them (ADR-174); never another person's.
    pub(crate) async fn renew_person_due(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let due = person_authorizations_due(&mut tx, user_id, RENEW_WITHIN).await?;
        tx.commit().await?;
        self.renew_all(tenant_id, due, Whose::Person(user_id)).await;
        Ok(())
    }

    /// Side by side: each bounded in time, none waits for another.
    async fn renew_all(&self, tenant_id: Uuid, due: Vec<Uuid>, whose: Whose) {
        let renewals = due
            .into_iter()
            .map(|id| self.renew_if_due(tenant_id, id, whose, RenewalFor::Runs));
        join_all(renewals).await;
    }

    /// Renews the authorization if it is about to expire and no other
    /// renewal of it is under way; for runs, only if the connector is
    /// enabled.
    pub(crate) async fn renew_if_due(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        whose: Whose,
        asker: RenewalFor,
    ) {
        if let Err(error) = self.renew(tenant_id, connector_id, whose, asker).await {
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
        whose: Whose,
        asker: RenewalFor,
    ) -> anyhow::Result<()> {
        let Some(call) = self.claim(tenant_id, connector_id, whose, asker).await? else {
            return Ok(());
        };
        let key = call.claimed.key;
        let opened = self.secrets.cipher().ok().and_then(|cipher| {
            let name = whose.refresh_sealed_as(connector_id);
            cipher.open(tenant_id, &name, &call.claimed.refresh).ok()
        });
        // Said to whoever authorized it: it will not open by waiting.
        let Some(refresh) = opened else {
            let mut tx = self.db.begin(tenant_id).await?;
            whose
                .record_failure(&mut tx, key, whose.refresh_unreadable())
                .await?;
            tx.commit().await?;
            anyhow::bail!("refresh token not opened for its connector");
        };
        let network = self.settings.network;
        let renewed = refresh_tokens(&call.declared, &refresh, call.secret.as_deref(), network);
        match renewed.await {
            Ok(granted) => {
                self.keep(tenant_id, connector_id, whose, call, granted)
                    .await?
            }
            Err(RenewalFailure::Refused(reason)) => {
                let mut tx = self.db.begin(tenant_id).await?;
                tx.lock_administration().await?;
                if whose.remove_claimed(&mut tx, key).await? {
                    let reason = format!("Autorisation retirée au renouvellement : {reason}");
                    let reason: String = reason.chars().take(300).collect();
                    let entry = whose.removed(tenant_id);
                    let entry = entry.resource("connector", connector_id).reason(reason);
                    append_audit(&mut tx, entry).await?;
                }
                tx.commit().await?;
                // Their session goes with it here (ADR-174).
                if let Some(user) = whose.person() {
                    self.forget_person(connector_id, user).await;
                }
            }
            // Said to whoever authorized it, asked again once the claim ends.
            Err(RenewalFailure::Unavailable(reason)) => {
                tracing::warn!(
                    connector = %connector_id,
                    reason,
                    "A connector's authorization server could not renew its token"
                );
                let mut tx = self.db.begin(tenant_id).await?;
                whose.record_failure(&mut tx, key, &reason).await?;
                tx.commit().await?;
            }
        }
        Ok(())
    }

    /// What the renewal brought, sealed for whoever authorized it and kept
    /// under the administration lock if the authorization is still the one
    /// claimed; otherwise no one keeps it, and it is revoked.
    async fn keep(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        whose: Whose,
        call: Call,
        granted: Granted,
    ) -> anyhow::Result<()> {
        let cipher = self.secrets.cipher().map_err(|e| anyhow!("{e}"))?;
        let sealed = match whose {
            Whose::Company => seal_grant(cipher, tenant_id, connector_id, &granted)?,
            Whose::Person(user) => {
                seal_person_grant(cipher, tenant_id, connector_id, user, &granted)?
            }
        };
        let mut tx = self.db.begin(tenant_id).await?;
        tx.lock_administration().await?;
        let renewed = Renewed {
            access_token: &sealed.access,
            access_expires_at: sealed.expires_at,
            refresh_token: sealed.refresh.as_ref(),
            scopes: sealed.scopes.as_deref(),
        };
        let kept = whose.keep(&mut tx, call.claimed.key, renewed).await?;
        tx.commit().await?;
        // Withdrawn or replaced meanwhile: no one keeps these.
        if !kept {
            self.revoke_unkept(Unkept {
                tenant_id,
                connector_id,
                user_id: whose.person(),
                declared: call.declared,
                granted,
                client_secret: call.secret,
            });
        }
        Ok(())
    }
}
