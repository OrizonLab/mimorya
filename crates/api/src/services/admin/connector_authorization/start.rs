//! The start of an MCP server's authorization (ADR-171): a company-wide
//! connector administrator for the company, or a person who may use
//! connectors for themself on a server each person authorizes (ADR-174); a
//! server declared with OAuth, its client's secret kept if it proves itself
//! with one, an authorization endpoint the network rules still allow; the
//! browser is sent there with PKCE S256, a fresh `state`, the resource and
//! the scopes declared, and an attempt sealed for it, under a name that
//! says whose it is.

use chrono::Utc;
use mimorya_connectors::mcp::oauth::{
    ClientAuth, Departure, OAuthHolder, authorization_url, endpoint_problem,
};
use mimorya_core::DomainError;
use mimorya_db::repositories::connectors::get_connector_instance;
use openidconnect::{CsrfToken, PkceCodeChallenge};
use uuid::Uuid;

use super::attempt::{self, AUTHORIZATION_LIFETIME, AuthorizationAttempt, declared_print};
use super::redirect_uri;
use crate::services::AccessContext;
use crate::services::admin::connector_oauth::McpServer;
use crate::services::admin::connectors::{ConnectorAdmin, PERMISSION};
use crate::services::connectors::{CONNECTOR_USE, client_secret_name};

/// Where the browser goes, and the attempt it keeps.
pub struct AuthorizationStart {
    pub redirect_url: String,
    pub attempt: String,
}

/// The company's authorization, started by a company-wide administrator.
pub async fn start_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    web_origin: &str,
) -> anyhow::Result<AuthorizationStart> {
    access.require_tenant_wide(PERMISSION)?;
    started(admin, access, id, web_origin, OAuthHolder::Company).await
}

/// The signed-in person's own authorization of a server each person
/// authorizes (ADR-174).
pub async fn start_person_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    web_origin: &str,
) -> anyhow::Result<AuthorizationStart> {
    access.require(CONNECTOR_USE)?;
    started(admin, access, id, web_origin, OAuthHolder::Person).await
}

async fn started(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    id: Uuid,
    web_origin: &str,
    holder: OAuthHolder,
) -> anyhow::Result<AuthorizationStart> {
    let tenant_id = access.subject.tenant_id;
    let mut tx = admin.db.begin(tenant_id).await?;
    let instance = get_connector_instance(&mut tx, id).await?;
    let enabled = instance.as_ref().is_some_and(|instance| instance.enabled);
    // An absent connector is answered as such first (ADR-047).
    let server = McpServer::of(instance)?;
    let secret_kept = admin
        .secrets
        .exists(&mut tx, &client_secret_name(id))
        .await?;
    tx.commit().await?;
    // The tokens will be sealed: nothing starts on a server that cannot.
    let cipher = admin.secrets.cipher()?;
    let declared = match (server.declared, server.unreadable) {
        (Some(declared), _) => declared,
        (None, true) => {
            return Err(DomainError::validation(
                "Déclaration OAuth de ce serveur illisible : déclarez-la de nouveau",
            )
            .into());
        }
        (None, false) => {
            return Err(DomainError::validation("Déclarez d’abord OAuth sur ce serveur").into());
        }
    };
    // Each person authorizes it for themself, or the company does: never
    // the other (ADR-174).
    match (holder, declared.holder) {
        (OAuthHolder::Company, OAuthHolder::Person) => {
            return Err(DomainError::conflict(
                "Chaque personne autorise ce serveur pour elle-même : l’entreprise ne l’autorise \
                 pas",
            )
            .into());
        }
        (OAuthHolder::Person, OAuthHolder::Company) => {
            return Err(DomainError::conflict(
                "L’entreprise autorise ce serveur : personne ne l’autorise pour soi",
            )
            .into());
        }
        _ => {}
    }
    // A person's authorization serves runs only: none for a server they
    // cannot use.
    if holder == OAuthHolder::Person && !enabled {
        return Err(DomainError::conflict("Ce serveur est désactivé").into());
    }
    if declared.client_auth != ClientAuth::None && !secret_kept {
        return Err(DomainError::validation(
            "Secret du client absent : déclarez OAuth de nouveau avec son secret",
        )
        .into());
    }
    // The operator may have narrowed the network rules since.
    let network = admin.connectors.network();
    let endpoint = &declared.authorization_endpoint;
    if let Some(problem) = endpoint_problem(endpoint, "Point d’autorisation", network) {
        return Err(DomainError::validation(problem).into());
    }
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let state = CsrfToken::new_random();
    let back = redirect_uri(web_origin);
    let departure = Departure {
        redirect_uri: &back,
        state: state.secret(),
        code_challenge: challenge.as_str(),
    };
    let url = authorization_url(&declared, departure).map_err(DomainError::validation)?;
    let attempt = AuthorizationAttempt {
        connector_id: id,
        user_id: access.subject.user_id,
        state: state.secret().clone(),
        pkce_verifier: verifier.secret().clone(),
        declared: declared_print(&server.url, &declared),
        expires_at: Utc::now() + AUTHORIZATION_LIFETIME,
    };
    let sealed = match holder {
        OAuthHolder::Company => attempt::seal(cipher, tenant_id, &attempt)?,
        OAuthHolder::Person => attempt::seal_person(cipher, tenant_id, &attempt)?,
    };
    Ok(AuthorizationStart {
        redirect_url: url,
        attempt: sealed,
    })
}
