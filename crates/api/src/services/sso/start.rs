//! The start towards a company's provider (ADR-063): the provider, still
//! enabled; the address the browser is sent to, as the provider declares
//! it (OpenID Connect's authorization, SAML's sign-in with its request,
//! ADR-068), checked like every address a provider names; and the attempt
//! the browser keeps until it comes back.

use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::repositories::identity_providers::{IdentityProviderRow, get_identity_provider};
use uuid::Uuid;

use super::attempt::{self, ATTEMPT_LIFETIME, Attempt};
use super::saml::{self, ServiceProvider};
use super::{SsoService, Started, oidc};

/// Where the browser goes, and what the attempt must hold for the return.
struct Departure {
    url: String,
    state: String,
    nonce: String,
    pkce_verifier: String,
    saml: Option<saml::SamlPending>,
}

impl SsoService {
    /// `None` when the provider no longer serves. The address, when the
    /// sign-in starts from one, is suggested to an OpenID Connect provider;
    /// else it starts from the company's link, and may create the account
    /// (ADR-067).
    pub(super) async fn start_at(
        &self,
        tenant_id: Uuid,
        provider_id: Uuid,
        login_hint: Option<&str>,
    ) -> anyhow::Result<Option<Started>> {
        let Some(cipher) = self.cipher.as_deref() else {
            return Ok(None);
        };
        let mut tx = self.db.begin(tenant_id).await?;
        let provider = get_identity_provider(&mut tx, provider_id)
            .await?
            .filter(|p| p.enabled);
        tx.commit().await?;
        let Some(provider) = provider else {
            return Ok(None);
        };
        let unusable = |failure: String| {
            tracing::warn!(
                provider_id = %provider.id,
                failure = %failure,
                "Identity provider unusable"
            );
            DomainError::unavailable(
                "Le fournisseur d’identité ne répond pas : réessayez, ou connectez-vous \
                 par mot de passe",
            )
        };
        let departure = match &provider.saml_metadata {
            Some(metadata) => self.saml_departure(&provider, metadata),
            None => self.oidc_departure(&provider, login_hint).await,
        }
        .map_err(unusable)?;
        // Where the browser is sent, as the provider declares it.
        if let Some(problem) = self.browser_address_problem(&departure.url) {
            return Err(unusable(format!("adresse de connexion : {problem}")).into());
        }
        let saml = departure.saml.is_some();
        let attempt = Attempt {
            provider_id: provider.id,
            state: departure.state,
            nonce: departure.nonce,
            pkce_verifier: departure.pkce_verifier,
            expires_at: Utc::now() + ATTEMPT_LIFETIME,
            from_link: login_hint.is_none(),
            saml: departure.saml,
        };
        Ok(Some(Started {
            redirect_url: departure.url,
            attempt: attempt::seal(cipher, provider.tenant_id, &attempt)?,
            saml,
        }))
    }

    async fn oidc_departure(
        &self,
        provider: &IdentityProviderRow,
        login_hint: Option<&str>,
    ) -> Result<Departure, String> {
        let metadata = self
            .declaration(provider.id, &provider.issuer)
            .await?
            .metadata;
        let authorization = oidc::authorization(
            metadata,
            &provider.client_id,
            &self.redirect_uri,
            login_hint,
        )?;
        Ok(Departure {
            url: authorization.url,
            state: authorization.state,
            nonce: authorization.nonce,
            pkce_verifier: authorization.pkce_verifier,
            saml: None,
        })
    }

    fn saml_departure(
        &self,
        provider: &IdentityProviderRow,
        metadata: &str,
    ) -> Result<Departure, String> {
        let application = ServiceProvider {
            entity_id: &provider.client_id,
            acs_url: &self.saml_acs_url,
        };
        let request = saml::authn_request(&application, metadata)?;
        Ok(Departure {
            url: request.url,
            state: String::new(),
            nonce: String::new(),
            pkce_verifier: String::new(),
            saml: Some(request.pending),
        })
    }
}
