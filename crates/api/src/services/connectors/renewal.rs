//! The access tokens of MCP servers' OAuth authorizations renewed before
//! they expire (ADR-171). Before a company's instances are read, for a run
//! or the administration's test, each authorization whose token expires
//! within `RENEW_WITHIN` is renewed with its refresh token, under the lock
//! of its row for the call (bounded in time): another process renewing it
//! at once waits, then finds it renewed, so that a refresh token the server
//! rotates is used once. The new tokens are sealed for the connector, a
//! rotated refresh token kept, the connector's version changed. A refusal
//! removes the authorization (audited, never a token): an administrator
//! authorizes the server again. A server that cannot answer leaves it as
//! it is, tried again by the next run.

use anyhow::anyhow;
use chrono::{Duration, Utc};
use mimorya_connectors::mcp::RENEW_WITHIN;
use mimorya_connectors::mcp::oauth::{ClientAuth, RenewalFailure, refresh_tokens};
use mimorya_db::TenantTx;
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connector_authorizations::delete_connector_authorization;
use mimorya_db::repositories::connector_renewals::{
    HeldRenewal, Renewed, connector_authorizations_due, hold_connector_renewal,
    renew_connector_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::credential::oauth_of;
use super::{ConnectorService, access_sealed_as, client_secret_name, refresh_sealed_as};
use crate::secrets::SecretRead;

/// An expiry further than this is read as this.
const MAX_LIFETIME_SECONDS: u64 = 10 * 365 * 24 * 3600;

pub const AUTHORIZATION_LOST: &str = "connector.oauth_authorization_removed";

impl ConnectorService {
    /// Renews the company's authorizations about to expire; one that fails
    /// is said in the logs, without a token, and does not stop the others.
    pub(crate) async fn renew_due(&self, tenant_id: Uuid) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let due = connector_authorizations_due(&mut tx, RENEW_WITHIN).await?;
        tx.commit().await?;
        for connector_id in due {
            if let Err(error) = self.renew(tenant_id, connector_id).await {
                tracing::warn!(
                    connector = %connector_id,
                    error = format!("{error:#}"),
                    "Failed to renew a connector's OAuth authorization"
                );
            }
        }
        Ok(())
    }

    async fn renew(&self, tenant_id: Uuid, connector_id: Uuid) -> anyhow::Result<()> {
        let mut tx = self.db.begin(tenant_id).await?;
        let Some(held) = hold_connector_renewal(&mut tx, connector_id).await? else {
            return Ok(());
        };
        // Renewed meanwhile by another process.
        let horizon = Utc::now() + Duration::seconds(RENEW_WITHIN.as_secs() as i64);
        if held.access_expires_at.is_none_or(|at| at >= horizon) {
            return Ok(());
        }
        let Some(instance) = get_connector_instance(&mut tx, connector_id).await? else {
            return Ok(());
        };
        let Some(declared) = oauth_of(&instance).filter(|declared| {
            declared.issuer == held.issuer
                && declared.client_id == held.client_id
                && declared.resource == held.resource
        }) else {
            // Not used under the declaration in force: nothing to renew.
            return Ok(());
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
        let refresh = self.opened_refresh(tenant_id, connector_id, &held)?;
        let network = self.settings.network;
        match refresh_tokens(&declared, &refresh, secret.as_deref(), network).await {
            Ok(granted) => {
                let cipher = self.secrets.cipher().map_err(|e| anyhow!("{e}"))?;
                let access = cipher.seal(
                    tenant_id,
                    &access_sealed_as(connector_id),
                    &granted.access_token,
                )?;
                let rotated = match &granted.refresh_token {
                    Some(token) => {
                        Some(cipher.seal(tenant_id, &refresh_sealed_as(connector_id), token)?)
                    }
                    None => None,
                };
                let expires_at = granted.expires_in.map(|seconds| {
                    Utc::now() + Duration::seconds(seconds.min(MAX_LIFETIME_SECONDS) as i64)
                });
                let scopes = granted.scopes.map(|scopes| scopes.join(" "));
                let renewed = Renewed {
                    access_token: &access,
                    access_expires_at: expires_at,
                    refresh_token: rotated.as_ref(),
                    scopes: scopes.as_deref(),
                };
                renew_connector_authorization(&mut tx, connector_id, renewed).await?;
            }
            Err(RenewalFailure::Refused(reason)) => {
                lose(&mut tx, tenant_id, connector_id, &reason).await?;
            }
            Err(RenewalFailure::Unavailable(reason)) => {
                tracing::warn!(
                    connector = %connector_id,
                    reason,
                    "A connector's authorization server could not renew its token"
                );
            }
        }
        tx.commit().await?;
        Ok(())
    }

    fn opened_refresh(
        &self,
        tenant_id: Uuid,
        connector_id: Uuid,
        held: &HeldRenewal,
    ) -> anyhow::Result<String> {
        let cipher = self.secrets.cipher().map_err(|e| anyhow!("{e}"))?;
        cipher
            .open(tenant_id, &refresh_sealed_as(connector_id), &held.refresh)
            .map_err(|_| anyhow!("refresh token not opened for its connector"))
    }
}

/// The authorization the server no longer honours, removed and audited.
async fn lose(
    tx: &mut TenantTx,
    tenant_id: Uuid,
    connector_id: Uuid,
    reason: &str,
) -> anyhow::Result<()> {
    delete_connector_authorization(tx, connector_id).await?;
    let reason = format!("Renouvellement refusé : {reason}");
    let entry = AuditEntry::new(tenant_id, "system", AUTHORIZATION_LOST, "success")
        .resource("connector", connector_id)
        .reason(reason.chars().take(300).collect::<String>());
    append_audit(tx, entry).await?;
    Ok(())
}
