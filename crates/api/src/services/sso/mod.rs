//! Signing in through a company's identity provider (ADR-063): OpenID
//! Connect, Microsoft 365 first, Google Workspace and any other provider.
//! The sign-in page learns from an address whether its account's company
//! offers a provider; the start sends the browser there with a sealed
//! attempt; the return proves the person, ties or finds the account, and
//! opens its session as a password sign-in does. No account is created; an
//! invited one accepts its invitation this way (ADR-064).

mod address;
mod admission;
mod attempt;
mod http;
mod oidc;
mod sign_in;

pub use attempt::{ATTEMPT_COOKIE, attempt_cookie, cleared_attempt_cookie};
pub use sign_in::{Return, SignInFailure};

use std::sync::Arc;

use chrono::Utc;
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::identity_providers::{
    ProviderLookup, find_identity_provider, get_identity_provider,
};
use uuid::Uuid;

use crate::secrets::{SecretCipher, SecretStore};
use address::address_problem;
use attempt::{ATTEMPT_LIFETIME, Attempt};
use http::ProviderHttp;

/// Where a provider sends the browser back, under the web origin.
pub const CALLBACK_PATH: &str = "/api/auth/sso/callback";

/// The name of a provider's client secret in the company's secret store.
pub fn client_secret_name(provider_id: Uuid) -> String {
    format!("sso.{provider_id}.client_secret")
}

/// A sign-in sent to its provider: where the browser goes, and the
/// attempt's cookie value.
pub struct Started {
    pub redirect_url: String,
    pub attempt: String,
}

pub struct SsoService {
    db: Database,
    secrets: SecretStore,
    cipher: Option<Arc<SecretCipher>>,
    http: ProviderHttp,
    network: NetworkPolicy,
    redirect_uri: String,
}

impl SsoService {
    pub fn new(
        db: Database,
        cipher: Option<Arc<SecretCipher>>,
        network: NetworkPolicy,
        web_origin: &str,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            db,
            secrets: SecretStore::new(cipher.clone()),
            cipher,
            http: ProviderHttp::new(network)?,
            network,
            redirect_uri: format!("{web_origin}{CALLBACK_PATH}"),
        })
    }

    /// The address to register at the provider.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub fn network(&self) -> NetworkPolicy {
        self.network
    }

    /// The provider the sign-in page offers for this address, if any.
    pub async fn discover(&self, email: &str) -> anyhow::Result<Option<ProviderLookup>> {
        // Without the master key, no attempt can be sealed nor secret read.
        if self.cipher.is_none() {
            return Ok(None);
        }
        Ok(find_identity_provider(self.db.pool(), email).await?)
    }

    /// Sends the browser to the provider of the address's account.
    pub async fn start(&self, email: &str) -> anyhow::Result<Started> {
        let none = || DomainError::not_found("Aucune connexion d’entreprise pour cette adresse");
        let cipher = self.cipher.as_deref().ok_or_else(none)?;
        let found = find_identity_provider(self.db.pool(), email)
            .await?
            .ok_or_else(none)?;
        let mut tx = self.db.begin(found.tenant_id).await?;
        let provider = get_identity_provider(&mut tx, found.provider_id)
            .await?
            .filter(|p| p.enabled)
            .ok_or_else(none)?;
        tx.commit().await?;
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
        let metadata = oidc::discover(&self.http, &provider.issuer)
            .await
            .map_err(unusable)?;
        let authorization =
            oidc::authorization(metadata, &provider.client_id, &self.redirect_uri, email)
                .map_err(unusable)?;
        // Where the browser is sent, as the provider declares it.
        if let Some(problem) = address_problem(&authorization.url, self.network) {
            return Err(unusable(format!("adresse d’autorisation : {problem}")).into());
        }
        let attempt = Attempt {
            provider_id: provider.id,
            state: authorization.state,
            nonce: authorization.nonce,
            pkce_verifier: authorization.pkce_verifier,
            expires_at: Utc::now() + ATTEMPT_LIFETIME,
        };
        Ok(Started {
            redirect_url: authorization.url,
            attempt: attempt::seal(cipher, provider.tenant_id, &attempt)?,
        })
    }

    /// Why the issuer cannot serve sign-ins, if it cannot (in French): its
    /// declaration is fetched as a sign-in would.
    pub async fn issuer_problem(&self, issuer: &str) -> Option<String> {
        oidc::discover(&self.http, issuer)
            .await
            .err()
            .map(|failure| format!("Fournisseur injoignable ou invalide ({failure})"))
    }
}
