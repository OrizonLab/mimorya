//! The provider's return (ADR-063): the attempt the browser kept must match
//! it; the code is exchanged and the identity token verified; the account
//! is the one tied to the person's subject at this provider, or, at the
//! first sign-in, the account of the company with the address the provider
//! vouches for, in a declared domain, which gets tied. Active, or invited
//! and then accepted (ADR-064), its session opens as with a password.
//! Every refusal says why, for the audit.

use axum::http::HeaderMap;
use chrono::Utc;
use mimorya_core::identity_providers::{ProviderKind, email_domain, proven_identity};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::identity_providers::{IdentityProviderRow, get_identity_provider};
use mimorya_db::repositories::tenants::tenant_status;
use mimorya_db::repositories::user_identities::{
    NewIdentity, find_identity, find_user_by_email, has_identity_at, link_identity, touch_identity,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::admission::{Admission, admit};
use super::attempt::{self, Attempt};
use super::oidc::{self, Exchange};
use super::{SsoService, client_secret_name};
use crate::auth::sessions::open_session;
use crate::plugins::RequestContext;
use crate::secrets::SecretRead;

/// What the provider sent back in the address.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Return {
    pub code: Option<String>,
    pub state: Option<String>,
    /// The provider's refusal (`access_denied`…).
    pub error: Option<String>,
}

/// A refused sign-in: why (in French, never a token nor a code), and as
/// much as is known of where and who, for the audit.
#[derive(Debug)]
pub struct SignInFailure {
    pub tenant_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub reason: String,
}

impl SignInFailure {
    fn at(tenant_id: Uuid, provider_id: Uuid, reason: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id),
            provider_id: Some(provider_id),
            user_id: None,
            reason: reason.into(),
        }
    }

    fn of(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// The database failing: still a refusal for the browser.
    fn internal(tenant_id: Uuid, provider_id: Uuid, error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "Single sign-on failed");
        Self::at(tenant_id, provider_id, "Erreur interne")
    }
}

type Outcome<T> = Result<T, SignInFailure>;

/// Same length, compared whole: the state never leaks by timing.
fn same_state(a: &str, b: &str) -> bool {
    Sha256::digest(a.as_bytes()) == Sha256::digest(b.as_bytes())
}

impl SsoService {
    /// The session token of the account signed in.
    pub async fn finish(
        &self,
        back: Return,
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
        let provider_id = attempt.provider_id;
        let at = |reason: &str| SignInFailure::at(tenant_id, provider_id, reason);
        // Another attempt's return, a sign-in forced from elsewhere: the
        // state comes back with a refusal too (RFC 6749, 4.1.2.1).
        if !back
            .state
            .is_some_and(|state| same_state(&state, &attempt.state))
        {
            return Err(at("Retour d’une autre tentative"));
        }
        if let Some(error) = back.error {
            let code = oidc::error_code(&error);
            return Err(at(&format!("Refusée par le fournisseur ({code})")));
        }
        let code = back.code.ok_or_else(|| at("Retour sans code"))?;
        let (provider, secret) = self.provider(tenant_id, provider_id).await?;
        let claims = self
            .verified_claims(&provider, &secret, code, attempt)
            .await
            .map_err(|failure| at(&failure))?;
        let kind = ProviderKind::parse(&provider.kind).ok_or_else(|| at("Type inconnu"))?;
        let proven =
            proven_identity(kind, &oidc::token_claims(&claims), &provider.domains).map_err(at)?;
        self.open(&provider, proven, context, headers).await
    }

    /// The provider, enabled, and its client secret.
    async fn provider(
        &self,
        tenant_id: Uuid,
        provider_id: Uuid,
    ) -> Outcome<(IdentityProviderRow, String)> {
        let internal =
            |e: &dyn std::fmt::Display| SignInFailure::internal(tenant_id, provider_id, e);
        let mut tx = self.db.begin(tenant_id).await.map_err(|e| internal(&e))?;
        let provider = get_identity_provider(&mut tx, provider_id)
            .await
            .map_err(|e| internal(&e))?
            .filter(|p| p.enabled)
            .ok_or_else(|| {
                SignInFailure::at(tenant_id, provider_id, "Fournisseur retiré ou désactivé")
            })?;
        let secret = self
            .secrets
            .read(&mut tx, tenant_id, &client_secret_name(provider_id))
            .await
            .map_err(|e| internal(&e))?;
        tx.commit().await.map_err(|e| internal(&e))?;
        match secret {
            SecretRead::Value(secret) => Ok((provider, secret)),
            _ => Err(SignInFailure::at(
                tenant_id,
                provider_id,
                "Secret client absent ou illisible",
            )),
        }
    }

    async fn verified_claims(
        &self,
        provider: &IdentityProviderRow,
        secret: &str,
        code: String,
        attempt: Attempt,
    ) -> Result<oidc::VerifiedClaims, String> {
        let metadata = oidc::discover(&self.http, &provider.issuer).await?;
        oidc::verified_claims(
            &self.http,
            metadata,
            Exchange {
                client_id: &provider.client_id,
                client_secret: secret,
                redirect_uri: &self.redirect_uri,
                code,
                pkce_verifier: attempt.pkce_verifier,
                nonce: attempt.nonce,
            },
        )
        .await
    }

    /// Finds or ties the account, checks it may sign in, opens its session.
    async fn open(
        &self,
        provider: &IdentityProviderRow,
        proven: mimorya_core::identity_providers::ProvenIdentity,
        context: &RequestContext,
        headers: &HeaderMap,
    ) -> Outcome<String> {
        let (tenant_id, provider_id) = (provider.tenant_id, provider.id);
        let at = |reason: &str| SignInFailure::at(tenant_id, provider_id, reason);
        let internal =
            |e: &dyn std::fmt::Display| SignInFailure::internal(tenant_id, provider_id, e);
        let mut tx = self.db.begin(tenant_id).await.map_err(|e| internal(&e))?;
        if tenant_status(&mut tx, tenant_id)
            .await
            .map_err(|e| internal(&e))?
            .as_deref()
            != Some("active")
        {
            return Err(at("Entreprise suspendue"));
        }
        let mut audit = Vec::new();
        let (user_id, linked) = match find_identity(&mut tx, provider_id, &proven.subject)
            .await
            .map_err(|e| internal(&e))?
        {
            Some(linked) => {
                touch_identity(&mut tx, linked.id)
                    .await
                    .map_err(|e| internal(&e))?;
                (linked.user_id, true)
            }
            None => {
                let email = proven
                    .email
                    .as_deref()
                    .ok_or_else(|| at("Adresse e-mail non garantie par le fournisseur"))?;
                if !email_domain(email).is_some_and(|d| provider.domains.contains(&d)) {
                    return Err(at("Adresse hors des domaines du fournisseur"));
                }
                let user_id = find_user_by_email(&mut tx, email)
                    .await
                    .map_err(|e| internal(&e))?
                    .ok_or_else(|| at("Aucun compte de l’entreprise pour cette adresse"))?;
                if has_identity_at(&mut tx, provider_id, user_id)
                    .await
                    .map_err(|e| internal(&e))?
                {
                    return Err(
                        at("Compte déjà relié à une autre identité de ce fournisseur").of(user_id),
                    );
                }
                (user_id, false)
            }
        };
        // The account's row first, as a password or an invitation's link
        // takes it: an acceptance, a resend or a disabling meanwhile waits.
        let admission = admit(&mut tx, user_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or_else(|| at("Compte inactif").of(user_id))?;
        if !linked {
            link_identity(
                &mut tx,
                NewIdentity {
                    tenant_id,
                    provider_id,
                    user_id,
                    subject: &proven.subject,
                },
            )
            .await
            .map_err(|e| internal(&e))?
            .ok_or_else(|| at("Identité reliée entre-temps").of(user_id))?;
            audit.push(
                AuditEntry::new(tenant_id, "user", "auth.identity_linked", "success")
                    .resource("identity_provider", provider_id),
            );
        }
        if let Admission::Accepted = admission {
            audit.push(
                AuditEntry::new(tenant_id, "user", "user.invitation_accepted", "success")
                    .resource("user", user_id)
                    .metadata(json!({ "method": "sso", "providerId": provider_id })),
            );
        }
        let token = open_session(&mut tx, user_id, context, headers)
            .await
            .map_err(|e| internal(&e))?;
        audit.push(
            AuditEntry::new(tenant_id, "user", "auth.login", "success")
                .metadata(json!({ "method": "sso", "providerId": provider_id })),
        );
        for entry in audit {
            let mut entry = entry.by_user(user_id).ip(context.ip_string());
            entry.request_id = Some(context.id.to_string());
            append_audit(&mut tx, entry)
                .await
                .map_err(|e| internal(&e))?;
        }
        tx.commit().await.map_err(|e| internal(&e))?;
        Ok(token)
    }
}
