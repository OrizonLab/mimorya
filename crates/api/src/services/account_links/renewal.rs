//! An access token renewed at the provider (ADR-117), from the refresh
//! token sealed in the person's link, with the platform's permissions the
//! person granted, no transaction held meanwhile; kept for the next calls.
//! A refresh token the provider rotates is sealed in the link, unless the
//! link changed in between. One the provider no longer honours removes the
//! link (audited): the person links their account again.

use std::time::{Duration, Instant};

use mimorya_core::account_links::{renewal_scopes, sealed_as};
use mimorya_core::tools::AccountAccessError;
use mimorya_db::repositories::account_links::{delete_revoked_account_link, renew_account_link};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use openidconnect::OAuth2TokenResponse;
use openidconnect::core::CoreProviderMetadata;
use uuid::Uuid;

use super::AccountLinkService;
use super::access::{LinkRead, NO_SECRET, internal, unavailable};
use super::tokens::LinkVersion;
use crate::services::sso::{Application, Unrefreshed, discover, refresh};

pub const ACCOUNT_LINK_REVOKED: &str = "account.link.revoked";
const RELINKED: &str = "La liaison du compte Microsoft 365 vient d’être refaite : réessayez";
const UNREACHABLE: &str = "Microsoft 365 ne renouvelle pas l’accès au compte pour l’instant : \
     réessayez plus tard";

impl AccountLinkService {
    /// A fresh access token for the link, kept for the next calls.
    pub(super) async fn renew(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        read: LinkRead,
    ) -> Result<String, AccountAccessError> {
        let metadata = self.declaration(&read).await?;
        let application = Application {
            client_id: &read.config.client_id,
            client_secret: &read.secret,
            redirect_uri: &self.redirect_uri,
        };
        let scopes = renewal_scopes(&read.link.scopes);
        let renewed = refresh(
            &self.http,
            metadata,
            &application,
            &read.refresh_token,
            &scopes,
        )
        .await;
        let tokens = match renewed {
            Ok(tokens) => tokens,
            Err(Unrefreshed::Revoked) => {
                return Err(match self.revoked(tenant_id, user_id, &read).await? {
                    true => AccountAccessError::Revoked,
                    // Made again meanwhile: the new link is renewed next.
                    false => unavailable(RELINKED),
                });
            }
            Err(Unrefreshed::Failed(failure)) => {
                tracing::warn!(failure = %failure, "Linked account renewal failed");
                return Err(unavailable(UNREACHABLE));
            }
        };
        let token = tokens.access_token().secret().clone();
        let rotated = tokens
            .refresh_token()
            .map(|rotated| rotated.secret())
            .filter(|rotated| !rotated.is_empty() && **rotated != read.refresh_token);
        let version = match rotated {
            Some(rotated) => {
                self.keep_rotated(tenant_id, user_id, &read, rotated)
                    .await?
            }
            None => Some(read.version()),
        };
        // A link changed meanwhile: its own token is renewed at the next call.
        if let Some(version) = version {
            let lifetime = tokens.expires_in().filter(|d| *d > Duration::ZERO);
            self.tokens.put(
                tenant_id,
                user_id,
                version,
                token.clone(),
                lifetime,
                Instant::now(),
            );
        }
        Ok(token)
    }

    /// The application's provider declaration, read once an hour.
    async fn declaration(
        &self,
        read: &LinkRead,
    ) -> Result<CoreProviderMetadata, AccountAccessError> {
        if let Some(metadata) = self.declarations.get(read.connector_id, Instant::now()) {
            return Ok(metadata);
        }
        let metadata = discover(&self.http, &read.config.issuer)
            .await
            .map_err(|failure| {
                tracing::warn!(failure = %failure, "Linked account provider unreachable");
                unavailable(UNREACHABLE)
            })?;
        self.declarations
            .put(read.connector_id, metadata.clone(), Instant::now());
        Ok(metadata)
    }

    /// Seals the rotated refresh token in the link, if unchanged since read.
    async fn keep_rotated(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        read: &LinkRead,
        rotated: &str,
    ) -> Result<Option<LinkVersion>, AccountAccessError> {
        let cipher = self
            .cipher
            .as_deref()
            .ok_or_else(|| unavailable(NO_SECRET))?;
        let sealed = cipher
            .seal(tenant_id, &sealed_as(read.connector_id, user_id), rotated)
            .map_err(internal)?;
        let mut tx = self.db.begin(tenant_id).await.map_err(internal)?;
        let renewed = renew_account_link(&mut tx, read.link.id, read.link.refreshed_at, &sealed)
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(renewed.map(|refreshed_at| LinkVersion {
            connector_id: read.connector_id,
            refreshed_at,
        }))
    }

    /// Removes the link the provider no longer honours, audited, unless it
    /// was made again since it was read; returns whether it was removed.
    async fn revoked(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        read: &LinkRead,
    ) -> Result<bool, AccountAccessError> {
        self.tokens.forget(tenant_id, user_id);
        let mut tx = self.db.begin(tenant_id).await.map_err(internal)?;
        let removed = delete_revoked_account_link(&mut tx, read.link.id, read.link.refreshed_at)
            .await
            .map_err(internal)?;
        if removed {
            append_audit(
                &mut tx,
                AuditEntry::new(tenant_id, "system", ACCOUNT_LINK_REVOKED, "success")
                    .on_behalf_of(user_id)
                    .resource("connector", read.connector_id),
            )
            .await
            .map_err(internal)?;
        }
        tx.commit().await.map_err(internal)?;
        Ok(removed)
    }
}
