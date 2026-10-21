//! Signing in through a company's identity provider (ADR-063): OpenID
//! Connect, Microsoft 365 first, Google Workspace and any other provider.
//! The sign-in page learns from an address whether its account's company
//! offers a provider; the start sends the browser there with a sealed
//! attempt; the return proves the person, ties or finds the account, and
//! opens its session as a password sign-in does. An invited account accepts
//! its invitation this way (ADR-064). A provider may be required for its
//! domains, which then sign in through it alone (ADR-066). The company's own
//! sign-in link names its provider, which may then create the account of a
//! person of its domains at the first sign-in (ADR-067): never an address
//! alone, as domains are declared, not proven.

mod account;
mod address;
mod admission;
mod attempt;
mod http;
mod metadata;
mod oidc;
mod requirement;
mod sign_in;

pub use attempt::{ATTEMPT_COOKIE, attempt_cookie, cleared_attempt_cookie};
pub use requirement::{SIGN_IN_ADMINISTRATOR, required_provider};
pub use sign_in::{Return, SignInFailure};

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use mimorya_connectors::mcp::NetworkPolicy;
use mimorya_core::DomainError;
use mimorya_db::Database;
use mimorya_db::repositories::identity_providers::{
    ProviderLink, ProviderLookup, find_identity_provider, find_provider_link, get_identity_provider,
};
use openidconnect::core::CoreProviderMetadata;
use uuid::Uuid;

use crate::secrets::{SecretCipher, SecretStore};
use address::address_problem;
use attempt::{ATTEMPT_LIFETIME, Attempt};
use http::ProviderHttp;
use metadata::MetadataCache;

/// Where a provider sends the browser back, under the web origin.
pub const CALLBACK_PATH: &str = "/api/auth/sso/callback";
/// The page of a company's sign-in link, the provider's id in its fragment
/// (ADR-067).
pub const LINK_PAGE: &str = "/login/entreprise";

/// The name of a provider's client secret in the company's secret store.
pub fn client_secret_name(provider_id: Uuid) -> String {
    format!("sso.{provider_id}.client_secret")
}

/// A company's sign-in link that names no provider serving.
pub fn link_unknown() -> DomainError {
    DomainError::not_found("Lien de connexion inconnu ou désactivé")
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
    link_page: String,
    /// The providers' declarations and keys, kept an hour (ADR-065).
    declarations: MetadataCache<CoreProviderMetadata>,
}

/// A declaration heavier than this, keys included, is not kept: read at
/// each sign-in instead, as the memory holds many providers.
const MAX_KEPT_BYTES: usize = 64 * 1024;

/// What a declaration weighs, keys included (they are not serialised with
/// it).
fn weight(metadata: &CoreProviderMetadata) -> usize {
    let size = |bytes: serde_json::Result<Vec<u8>>| bytes.map_or(usize::MAX, |b| b.len());
    size(serde_json::to_vec(metadata)).saturating_add(size(serde_json::to_vec(metadata.jwks())))
}

/// An issuer's declaration, and whether it came from memory: its keys may
/// then be older than the provider's.
struct Declaration {
    metadata: CoreProviderMetadata,
    kept: bool,
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
            link_page: format!("{web_origin}{LINK_PAGE}"),
            declarations: MetadataCache::default(),
        })
    }

    /// The address to register at the provider.
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    /// The company's sign-in link that names this provider (ADR-067).
    pub fn sign_in_link(&self, provider_id: Uuid) -> String {
        format!("{}#{provider_id}", self.link_page)
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

    /// The provider a company's sign-in link names, and the company, if it
    /// serves (ADR-067).
    pub async fn link(&self, provider_id: Uuid) -> anyhow::Result<Option<ProviderLink>> {
        if self.cipher.is_none() {
            return Ok(None);
        }
        Ok(find_provider_link(self.db.pool(), provider_id).await?)
    }

    /// Sends the browser to the provider of the address's account.
    pub async fn start(&self, email: &str) -> anyhow::Result<Started> {
        let none = || DomainError::not_found("Aucune connexion d’entreprise pour cette adresse");
        let found = self.discover(email).await?.ok_or_else(none)?;
        let started = self
            .start_at(found.tenant_id, found.provider_id, Some(email))
            .await?;
        Ok(started.ok_or_else(none)?)
    }

    /// Sends the browser to the provider a company's sign-in link names.
    pub async fn start_link(&self, provider_id: Uuid) -> anyhow::Result<Started> {
        let found = self.link(provider_id).await?.ok_or_else(link_unknown)?;
        let started = self.start_at(found.tenant_id, provider_id, None).await?;
        Ok(started.ok_or_else(link_unknown)?)
    }

    /// `None` when the provider no longer serves. The address, when the
    /// sign-in starts from one, is suggested to the provider; else it starts
    /// from the company's link, and may create the account (ADR-067).
    async fn start_at(
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
        let metadata = self
            .declaration(provider.id, &provider.issuer)
            .await
            .map_err(unusable)?
            .metadata;
        let authorization = oidc::authorization(
            metadata,
            &provider.client_id,
            &self.redirect_uri,
            login_hint,
        )
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
            from_link: login_hint.is_none(),
        };
        Ok(Some(Started {
            redirect_url: authorization.url,
            attempt: attempt::seal(cipher, provider.tenant_id, &attempt)?,
        }))
    }

    /// Why the issuer cannot serve sign-ins, if it cannot (in French): its
    /// declaration is read anew, never answered from memory nor kept (the
    /// provider is not declared yet).
    pub async fn issuer_problem(&self, issuer: &str) -> Option<String> {
        oidc::discover(&self.http, issuer)
            .await
            .err()
            .map(|failure| format!("Fournisseur injoignable ou invalide ({failure})"))
    }

    /// The provider's declaration and keys: the kept ones while they serve,
    /// else read (ADR-065).
    async fn declaration(
        &self,
        provider_id: Uuid,
        issuer: &str,
    ) -> Result<Declaration, oidc::Failure> {
        if let Some(metadata) = self.declarations.get(provider_id, Instant::now()) {
            return Ok(Declaration {
                metadata,
                kept: true,
            });
        }
        let metadata = self.read_declaration(provider_id, issuer).await?;
        Ok(Declaration {
            metadata,
            kept: false,
        })
    }

    /// The provider's declaration and keys, read anew and kept unless too
    /// heavy; a failure is not kept.
    async fn read_declaration(
        &self,
        provider_id: Uuid,
        issuer: &str,
    ) -> Result<CoreProviderMetadata, oidc::Failure> {
        let metadata = oidc::discover(&self.http, issuer).await?;
        if weight(&metadata) <= MAX_KEPT_BYTES {
            self.declarations
                .put(provider_id, metadata.clone(), Instant::now());
        }
        Ok(metadata)
    }
}
