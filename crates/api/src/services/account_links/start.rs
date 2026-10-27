//! The start of a link (ADR-116): the person may use the company's
//! connectors, the connector is its Microsoft 365 application, enabled;
//! the browser is sent to the directory's sign-in with the connector's
//! delegated permissions (writing too when its tools may write, ADR-119),
//! the person's address as a hint, and an attempt sealed for it.

use chrono::Utc;
use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_core::DomainError;
use mimorya_core::account_links::link_scopes;
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::attempt::{self, LINK_LIFETIME, LinkAttempt};
use super::{AccountLinkService, LinkStart, connector_unknown, linkable};
use crate::services::AccessContext;
use crate::services::connectors::CONNECTOR_USE;
use crate::services::sso::{address_problem, authorization, discover};

impl AccountLinkService {
    pub async fn start(
        &self,
        access: &AccessContext,
        connector_id: Uuid,
    ) -> anyhow::Result<LinkStart> {
        self.secrets.require()?;
        let cipher = self.cipher.as_deref().ok_or_else(connector_unknown)?;
        access.require(CONNECTOR_USE)?;
        let tenant_id = access.subject.tenant_id;
        let mut tx = self.db.begin(tenant_id).await?;
        let instance = linkable(get_connector_instance(&mut tx, connector_id).await?)
            .ok_or_else(connector_unknown)?;
        tx.commit().await?;
        let config = Microsoft365Config::read(&instance.config)
            .map_err(|error| DomainError::unavailable(error.to_string()))?;
        let unreachable = |failure: String| {
            tracing::warn!(connector = %connector_id, failure = %failure, "Link start failed");
            DomainError::unavailable(
                "Microsoft 365 ne répond pas comme attendu : réessayez plus tard",
            )
        };
        let metadata = discover(&self.http, self.issuer_of(&config))
            .await
            .map_err(unreachable)?;
        let departure = authorization(
            metadata,
            &config.client_id,
            &self.redirect_uri,
            Some(&access.profile.email),
            &link_scopes(config.write_tools),
        )
        .map_err(unreachable)?;
        // Where the browser is sent, as the provider declares it.
        if let Some(problem) = address_problem(&departure.url, self.network) {
            return Err(unreachable(format!("adresse de connexion : {problem}")).into());
        }
        let attempt = LinkAttempt {
            connector_id,
            user_id: access.subject.user_id,
            state: departure.state,
            nonce: departure.nonce,
            pkce_verifier: departure.pkce_verifier,
            expires_at: Utc::now() + LINK_LIFETIME,
        };
        Ok(LinkStart {
            redirect_url: departure.url,
            attempt: attempt::seal(cipher, tenant_id, &attempt)?,
        })
    }
}
