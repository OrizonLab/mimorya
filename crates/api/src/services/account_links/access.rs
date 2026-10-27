//! The access token of the person a run serves, at their company's
//! Microsoft 365 or Google Workspace application (ADR-117, ADR-121), as the
//! tool's connector type says. Each call reads the person's link there and
//! the application's secret, and refuses a tool whose permissions the
//! person did not grant (ADR-119); a token kept from this link serves until
//! shortly before it expires; otherwise the link's sealed refresh token is
//! renewed at the provider (`renewal.rs`).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mimorya_core::account_links::{LinkKind, sealed_as};
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use mimorya_db::repositories::account_links::{AccountLinkRow, get_account_link};
use mimorya_db::repositories::connectors::list_connector_instances;
use uuid::Uuid;

use super::AccountLinkService;
use super::application::LinkApp;
use super::tokens::LinkVersion;
use crate::secrets::SecretRead;
use crate::services::connectors::token_secret_name;

/// The accounts a run's person linked, as its tools reach them.
pub struct PersonAccounts {
    service: Arc<AccountLinkService>,
    tenant_id: Uuid,
    user_id: Uuid,
}

impl PersonAccounts {
    pub fn new(service: Arc<AccountLinkService>, tenant_id: Uuid, user_id: Uuid) -> Self {
        Self {
            service,
            tenant_id,
            user_id,
        }
    }
}

#[async_trait]
impl LinkedAccounts for PersonAccounts {
    async fn access_token(
        &self,
        connector_type: &str,
        needs: &[&str],
    ) -> Result<String, AccountAccessError> {
        let Some(kind) = LinkKind::of(connector_type) else {
            return Err(AccountAccessError::NotLinked);
        };
        self.service
            .access_token(self.tenant_id, self.user_id, kind, needs)
            .await
    }

    /// The person's tokens are forgotten, at every provider: the next call
    /// renews its own.
    async fn rejected(&self, _connector_type: &str) {
        self.service.tokens.forget(self.tenant_id, self.user_id);
    }
}

/// What a renewal needs, read in one transaction.
pub(super) struct LinkRead {
    pub connector_id: Uuid,
    pub app: LinkApp,
    pub secret: String,
    pub link: AccountLinkRow,
    pub refresh_token: String,
}

impl LinkRead {
    pub fn version(&self) -> LinkVersion {
        LinkVersion {
            connector_id: self.connector_id,
            refreshed_at: self.link.refreshed_at,
        }
    }
}

pub(super) fn unavailable(why: &str) -> AccountAccessError {
    AccountAccessError::Unavailable(why.into())
}

pub(super) fn no_secret(kind: LinkKind) -> AccountAccessError {
    AccountAccessError::Unavailable(format!(
        "Le secret de l’application {} de l’entreprise est absent ou illisible : \
         l’administration le saisit de nouveau",
        kind.product()
    ))
}

/// An internal failure, logged; the model is told only that access is
/// impossible for now.
pub(super) fn internal(kind: LinkKind, error: impl std::fmt::Display) -> AccountAccessError {
    tracing::error!(error = %error, "Linked account unreachable");
    AccountAccessError::Unavailable(format!(
        "Accès au compte {} impossible pour l’instant (erreur interne)",
        kind.product()
    ))
}

impl AccountLinkService {
    async fn access_token(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        kind: LinkKind,
        needs: &[&str],
    ) -> Result<String, AccountAccessError> {
        let read = self.read_link(tenant_id, user_id, kind).await?;
        let missing = kind.missing_scopes(&read.link.scopes, needs);
        if !missing.is_empty() {
            return Err(AccountAccessError::NotGranted(
                missing.into_iter().map(String::from).collect(),
            ));
        }
        let kept = self
            .tokens
            .get(tenant_id, user_id, read.version(), Instant::now());
        match kept {
            Some(token) => Ok(token),
            None => self.renew(tenant_id, user_id, read).await,
        }
    }

    /// The company's enabled application of that kind, its secret, the
    /// person's link there and its refresh token, opened.
    async fn read_link(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        kind: LinkKind,
    ) -> Result<LinkRead, AccountAccessError> {
        let cipher = self.cipher.as_deref().ok_or_else(|| no_secret(kind))?;
        let mut tx = self
            .db
            .begin(tenant_id)
            .await
            .map_err(|e| internal(kind, e))?;
        let instance = list_connector_instances(&mut tx, false)
            .await
            .map_err(|e| internal(kind, e))?
            .into_iter()
            .find(|i| i.connector_type == kind.connector_type() && i.enabled)
            .ok_or_else(|| {
                unavailable(&format!(
                    "L’application {} de l’entreprise n’est pas active",
                    kind.product()
                ))
            })?;
        let (link, sealed) = get_account_link(&mut tx, instance.id, user_id)
            .await
            .map_err(|e| internal(kind, e))?
            .ok_or(AccountAccessError::NotLinked)?;
        let secret = self
            .secrets
            .read(&mut tx, tenant_id, &token_secret_name(instance.id))
            .await
            .map_err(|e| internal(kind, e))?;
        tx.commit().await.map_err(|e| internal(kind, e))?;
        let SecretRead::Value(secret) = secret else {
            return Err(no_secret(kind));
        };
        let app = LinkApp::read(&instance).map_err(|e| internal(kind, e))?;
        let refresh_token = cipher
            .open(tenant_id, &sealed_as(instance.id, user_id), &sealed)
            .map_err(|e| internal(kind, e))?;
        Ok(LinkRead {
            connector_id: instance.id,
            app,
            secret,
            link,
            refresh_token,
        })
    }
}
