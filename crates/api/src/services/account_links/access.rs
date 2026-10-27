//! The access token of the person a run serves, at their company's
//! Microsoft 365 application (ADR-117). Each call reads the person's link
//! and the application's secret; a token kept from this link serves until
//! shortly before it expires; otherwise the link's sealed refresh token is
//! renewed at the provider (`renewal.rs`).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_core::account_links::{MICROSOFT365, sealed_as};
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use mimorya_db::repositories::account_links::{AccountLinkRow, get_account_link};
use mimorya_db::repositories::connectors::list_connector_instances;
use uuid::Uuid;

use super::AccountLinkService;
use super::tokens::LinkVersion;
use crate::secrets::SecretRead;
use crate::services::connectors::token_secret_name;

const NOT_ACTIVE: &str = "L’application Microsoft 365 de l’entreprise n’est pas active";
pub(super) const NO_SECRET: &str = "Le secret de l’application Microsoft 365 de l’entreprise est absent \
     ou illisible : l’administration le saisit de nouveau";
const INTERNAL: &str = "Accès au compte Microsoft 365 impossible pour l’instant (erreur interne)";

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
    async fn access_token(&self, connector_type: &str) -> Result<String, AccountAccessError> {
        if connector_type != MICROSOFT365 {
            return Err(AccountAccessError::NotLinked);
        }
        self.service
            .access_token(self.tenant_id, self.user_id)
            .await
    }

    async fn rejected(&self, _connector_type: &str) {
        self.service.tokens.forget(self.tenant_id, self.user_id);
    }
}

/// What a renewal needs, read in one transaction.
pub(super) struct LinkRead {
    pub connector_id: Uuid,
    pub config: Microsoft365Config,
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

pub(super) fn internal(error: impl std::fmt::Display) -> AccountAccessError {
    tracing::error!(error = %error, "Linked account unreachable");
    unavailable(INTERNAL)
}

impl AccountLinkService {
    async fn access_token(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<String, AccountAccessError> {
        let read = self.read_link(tenant_id, user_id).await?;
        let kept = self
            .tokens
            .get(tenant_id, user_id, read.version(), Instant::now());
        match kept {
            Some(token) => Ok(token),
            None => self.renew(tenant_id, user_id, read).await,
        }
    }

    /// The company's enabled application, its secret, the person's link
    /// there and its refresh token, opened.
    async fn read_link(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> Result<LinkRead, AccountAccessError> {
        let cipher = self
            .cipher
            .as_deref()
            .ok_or_else(|| unavailable(NO_SECRET))?;
        let mut tx = self.db.begin(tenant_id).await.map_err(internal)?;
        let instance = list_connector_instances(&mut tx, false)
            .await
            .map_err(internal)?
            .into_iter()
            .find(|i| i.connector_type == MICROSOFT365 && i.enabled)
            .ok_or_else(|| unavailable(NOT_ACTIVE))?;
        let (link, sealed) = get_account_link(&mut tx, instance.id, user_id)
            .await
            .map_err(internal)?
            .ok_or(AccountAccessError::NotLinked)?;
        let secret = self
            .secrets
            .read(&mut tx, tenant_id, &token_secret_name(instance.id))
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        let SecretRead::Value(secret) = secret else {
            return Err(unavailable(NO_SECRET));
        };
        let config = Microsoft365Config::read(&instance.config).map_err(internal)?;
        let refresh_token = cipher
            .open(tenant_id, &sealed_as(instance.id, user_id), &sealed)
            .map_err(internal)?;
        Ok(LinkRead {
            connector_id: instance.id,
            config,
            secret,
            link,
            refresh_token,
        })
    }
}
