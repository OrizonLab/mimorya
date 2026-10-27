//! The provider's return (ADR-116): the attempt the browser kept must be
//! the signed-in person's own and match the return; the code is redeemed,
//! the identity token verified, and it must name the person's own account
//! (`linked_identity`). The refresh token is then sealed for this company,
//! connector and person, and kept in their link. Every refusal says why,
//! for the audit; the browser learns only whether another account was
//! chosen.

use chrono::Utc;
use mimorya_connectors::microsoft365::Microsoft365Config;
use mimorya_core::account_links::{LinkRefusal, linked_identity, sealed_as};
use mimorya_db::repositories::account_links::{NewAccountLink, put_account_link};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use mimorya_db::repositories::connectors::get_connector_instance;
use uuid::Uuid;

use super::attempt::{self, LinkAttempt};
use super::grant::{Granted, redeem};
use super::{AccountLinkService, link_metadata, linkable};
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::connectors::{CONNECTOR_USE, token_secret_name};
use crate::services::sso::Return;
use crate::services::sso::oidc::{self, Application};
use crate::services::sso::sign_in::same_state;

/// What the browser is told of a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefused {
    Failed,
    /// The person signed in at Microsoft with another account than theirs.
    OtherAccount,
}

/// A refused link: why (in French, never a token nor a code), for the audit.
#[derive(Debug)]
pub struct LinkFailure {
    pub connector_id: Option<Uuid>,
    pub reason: String,
    pub refused: LinkRefused,
}

impl LinkFailure {
    fn new(connector_id: Option<Uuid>, reason: impl Into<String>) -> Self {
        Self {
            connector_id,
            reason: reason.into(),
            refused: LinkRefused::Failed,
        }
    }

    /// The database failing: still a refusal for the browser.
    fn internal(connector_id: Uuid, error: impl std::fmt::Display) -> Self {
        tracing::error!(error = %error, "Account link failed");
        Self::new(Some(connector_id), "Erreur interne")
    }
}

type Outcome<T> = Result<T, LinkFailure>;

impl AccountLinkService {
    pub async fn finish(
        &self,
        access: &AccessContext,
        back: Return,
        cookie: Option<&str>,
    ) -> Outcome<()> {
        let attempt = self.own_attempt(access, cookie)?;
        let connector_id = attempt.connector_id;
        let at = |reason: &str| LinkFailure::new(Some(connector_id), reason);
        // Another attempt's return, a link forced from elsewhere: the state
        // comes back with a refusal too (RFC 6749, 4.1.2.1).
        if !back
            .state
            .is_some_and(|state| same_state(&state, &attempt.state))
        {
            return Err(at("Retour d’une autre tentative"));
        }
        if let Some(error) = back.error {
            let code = oidc::error_code(&error);
            return Err(at(&format!("Refusée par Microsoft ({code})")));
        }
        let code = back.code.ok_or_else(|| at("Retour sans code"))?;
        // Its rights may have changed since the start.
        if !access.has(CONNECTOR_USE) {
            return Err(at("Connecteurs non autorisés"));
        }
        let (config, secret) = self.application(access, connector_id).await?;
        let metadata = oidc::discover(&self.http, &config.issuer)
            .await
            .map_err(|failure| at(&failure))?;
        let application = Application {
            client_id: &config.client_id,
            client_secret: &secret,
            redirect_uri: &self.redirect_uri,
        };
        let granted = redeem(
            &self.http,
            metadata.clone(),
            &application,
            code,
            attempt.pkce_verifier,
        )
        .await
        .map_err(|failure| at(&failure))?;
        let claims = oidc::verify(metadata, &application, &granted.id_token, &attempt.nonce)
            .map_err(|unverified| at(&unverified.failure()))?;
        let identity = linked_identity(&oidc::token_claims(&claims), &access.profile.email)
            .map_err(|refusal| match refusal {
                LinkRefusal::Unidentified => at("Jeton sans compte vérifié de l’annuaire"),
                LinkRefusal::NotTheirs => LinkFailure {
                    refused: LinkRefused::OtherAccount,
                    ..at("Compte Microsoft d’une autre adresse")
                },
            })?;
        self.keep(
            access,
            connector_id,
            &identity.subject,
            &identity.address,
            &granted,
        )
        .await
    }

    /// The attempt the browser kept, if it is the signed-in person's own.
    fn own_attempt(&self, access: &AccessContext, cookie: Option<&str>) -> Outcome<LinkAttempt> {
        let unknown = || LinkFailure::new(None, "Tentative inconnue ou expirée");
        let cipher = self.cipher.as_deref().ok_or_else(unknown)?;
        let (tenant_id, attempt) = cookie
            .and_then(|value| attempt::open(cipher, value, Utc::now()))
            .ok_or_else(unknown)?;
        if tenant_id != access.subject.tenant_id || attempt.user_id != access.subject.user_id {
            return Err(LinkFailure::new(None, "Tentative d’une autre personne"));
        }
        Ok(attempt)
    }

    /// The connector's application, still linkable, and its secret.
    async fn application(
        &self,
        access: &AccessContext,
        connector_id: Uuid,
    ) -> Outcome<(Microsoft365Config, String)> {
        let internal = |error: anyhow::Error| LinkFailure::internal(connector_id, error);
        let gone = || LinkFailure::new(Some(connector_id), "Connecteur supprimé ou désactivé");
        let tenant_id = access.subject.tenant_id;
        let mut tx = self
            .db
            .begin(tenant_id)
            .await
            .map_err(|e| internal(e.into()))?;
        let instance = get_connector_instance(&mut tx, connector_id)
            .await
            .map_err(|e| internal(e.into()))?;
        let instance = linkable(instance).ok_or_else(gone)?;
        let secret = self
            .secrets
            .read(&mut tx, tenant_id, &token_secret_name(connector_id))
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(|e| internal(e.into()))?;
        let SecretRead::Value(secret) = secret else {
            return Err(LinkFailure::new(
                Some(connector_id),
                "Secret de l’application absent ou illisible",
            ));
        };
        let config = Microsoft365Config::read(&instance.config)
            .map_err(|error| LinkFailure::new(Some(connector_id), error.to_string()))?;
        Ok((config, secret))
    }

    /// The link kept, its refresh token sealed for it, and audited.
    async fn keep(
        &self,
        access: &AccessContext,
        connector_id: Uuid,
        subject: &str,
        address: &str,
        granted: &Granted,
    ) -> Outcome<()> {
        let internal = |error: anyhow::Error| LinkFailure::internal(connector_id, error);
        let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
        let cipher = self
            .cipher
            .as_deref()
            .ok_or_else(|| internal(anyhow::anyhow!("no master key")))?;
        let sealed = cipher
            .seal(
                tenant_id,
                &sealed_as(connector_id, user_id),
                &granted.refresh_token,
            )
            .map_err(internal)?;
        let mut tx = self
            .db
            .begin(tenant_id)
            .await
            .map_err(|e| internal(e.into()))?;
        // Deleted or disabled while the person was at Microsoft.
        let instance = get_connector_instance(&mut tx, connector_id)
            .await
            .map_err(|e| internal(e.into()))?;
        if linkable(instance).is_none() {
            return Err(LinkFailure::new(
                Some(connector_id),
                "Connecteur supprimé ou désactivé",
            ));
        }
        let replaced = put_account_link(
            &mut tx,
            NewAccountLink {
                tenant_id,
                connector_id,
                user_id,
                subject,
                address,
                scopes: &granted.scopes,
                refresh_token: &sealed,
            },
        )
        .await
        .map_err(|e| internal(e.into()))?;
        append_audit(
            &mut tx,
            AuditEntry::new(tenant_id, "user", "account.link.created", "success")
                .by_user(user_id)
                .resource("connector", connector_id)
                .metadata(link_metadata(replaced)),
        )
        .await
        .map_err(|e| internal(e.into()))?;
        tx.commit().await.map_err(|e| internal(e.into()))?;
        Ok(())
    }
}
