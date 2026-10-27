//! The start of a link (ADR-116, ADR-121): the person may use the
//! company's connectors, the connector is its Microsoft 365 or Google
//! Workspace application, enabled; the browser is sent to the provider's
//! sign-in with the connector's delegated permissions (writing too when its
//! tools may write, ADR-119), the person's address as a hint, the
//! provider's own parameters (`LinkKind::authorization_params`), and an
//! attempt sealed for it.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::application::LinkApp;
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
        let app = LinkApp::read(&instance)
            .map_err(|error| DomainError::unavailable(error.to_string()))?;
        let kind = app.kind;
        let unreachable = |failure: String| {
            tracing::warn!(connector = %connector_id, failure = %failure, "Link start failed");
            DomainError::unavailable(format!(
                "{} ne répond pas comme attendu : réessayez plus tard",
                kind.product()
            ))
        };
        let metadata = discover(&self.http, self.issuer_of(&app))
            .await
            .map_err(unreachable)?;
        let email = &access.profile.email;
        let departure = authorization(
            metadata,
            &app.client_id,
            &self.redirect_uri,
            Some(email),
            &kind.link_scopes(app.write_tools),
            &kind.authorization_params(email),
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
