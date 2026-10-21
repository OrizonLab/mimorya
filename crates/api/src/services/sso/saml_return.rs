//! A SAML provider's response, posted by the browser (ADR-068): the attempt
//! the browser kept must await a SAML request of an enabled SAML provider
//! of its company; the response must answer that request, its assertion be
//! signed by the declared metadata's certificates, name this application
//! and be valid now. Its assertions are accepted once
//! (`sso_used_assertions`); then the
//! account is found, tied or created, and its session opened, as for an
//! OpenID Connect provider. Every refusal says why, for the audit.

use std::time::SystemTime;

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use mimorya_core::identity_providers::{ProviderKind, federated_display_name, proven_identity};
use mimorya_db::repositories::identity_providers::{IdentityProviderRow, get_identity_provider};
use mimorya_db::repositories::sso_assertions::record_assertion;
use serde::Deserialize;
use uuid::Uuid;

use super::SsoService;
use super::attempt;
use super::saml::{self, ServiceProvider, Statement};
use super::sign_in::{Outcome, SignInFailure};
use crate::plugins::RequestContext;

/// What the provider's form posts.
#[derive(Debug, Default, Deserialize)]
pub struct SamlReturn {
    #[serde(rename = "SAMLResponse")]
    pub saml_response: Option<String>,
    #[serde(rename = "RelayState")]
    pub relay_state: Option<String>,
}

impl SsoService {
    /// The session token of the account signed in.
    pub async fn finish_saml(
        &self,
        back: SamlReturn,
        cookie: Option<&str>,
        context: &RequestContext,
        headers: &HeaderMap,
    ) -> Outcome<String> {
        let unknown = || SignInFailure {
            tenant_id: None,
            provider_id: None,
            user_id: None,
            reason: "Tentative inconnue ou expirée".into(),
        };
        let cipher = self.cipher.as_deref().ok_or_else(unknown)?;
        let (tenant_id, attempt) = cookie
            .and_then(|value| attempt::open(cipher, value, Utc::now()))
            .ok_or_else(unknown)?;
        // An OpenID Connect attempt awaits no SAML response.
        let pending = attempt.saml.as_ref().ok_or_else(unknown)?;
        let provider_id = attempt.provider_id;
        let at = |reason: &str| SignInFailure::at(tenant_id, provider_id, reason);
        let response = back
            .saml_response
            .ok_or_else(|| at("Retour sans réponse SAML"))?;
        let provider = self.saml_provider(tenant_id, provider_id).await?;
        let metadata = provider.saml_metadata.as_deref().unwrap_or_default();
        let mut fields = vec![("SAMLResponse".to_string(), response)];
        if let Some(relay_state) = back.relay_state {
            fields.push(("RelayState".to_string(), relay_state));
        }
        let application = ServiceProvider {
            entity_id: &provider.client_id,
            acs_url: &self.saml_acs_url,
        };
        let (session, carried) =
            saml::verify_response(&application, metadata, pending, fields, SystemTime::now())
                .map_err(|failure| at(&failure))?;
        let statement = Statement::of(&session);
        let said = statement.claims().map_err(at)?;
        let proven = proven_identity(ProviderKind::Saml, &said, &provider.domains).map_err(at)?;
        self.accept_once(&provider, &carried).await?;
        // The account's name, should this sign-in create it: started from
        // the company's link, never from an address (ADR-067).
        let name = proven
            .email
            .as_deref()
            .filter(|_| attempt.from_link)
            .map(|email| federated_display_name(&said, email));
        self.open(&provider, proven, name.as_deref(), context, headers)
            .await
    }

    /// The provider, enabled and SAML.
    async fn saml_provider(
        &self,
        tenant_id: Uuid,
        provider_id: Uuid,
    ) -> Outcome<IdentityProviderRow> {
        let internal = |e: sqlx::Error| SignInFailure::internal(tenant_id, provider_id, e);
        let mut tx = self
            .db
            .begin(tenant_id)
            .await
            .map_err(|e| SignInFailure::internal(tenant_id, provider_id, e))?;
        let provider = get_identity_provider(&mut tx, provider_id)
            .await
            .map_err(internal)?
            .filter(|p| p.enabled && p.saml_metadata.is_some());
        tx.commit().await.map_err(internal)?;
        provider.ok_or_else(|| {
            SignInFailure::at(tenant_id, provider_id, "Fournisseur retiré ou désactivé")
        })
    }

    /// The response's assertions recorded until they expire, together: a
    /// second return with any of them, on any server, is refused, and then
    /// none of this one's is recorded.
    async fn accept_once(
        &self,
        provider: &IdentityProviderRow,
        carried: &[saml::Carried],
    ) -> Outcome<()> {
        let (tenant_id, provider_id) = (provider.tenant_id, provider.id);
        let internal =
            |e: &dyn std::fmt::Display| SignInFailure::internal(tenant_id, provider_id, e);
        let mut tx = self.db.begin(tenant_id).await.map_err(|e| internal(&e))?;
        for assertion in carried {
            let expires_at = DateTime::<Utc>::from(assertion.expires_at);
            let new = record_assertion(
                &mut tx,
                tenant_id,
                provider_id,
                &assertion.assertion_id,
                expires_at,
            )
            .await
            .map_err(|e| internal(&e))?;
            if !new {
                return Err(SignInFailure::at(
                    tenant_id,
                    provider_id,
                    "Assertion SAML déjà utilisée",
                ));
            }
        }
        tx.commit().await.map_err(|e| internal(&e))
    }
}
