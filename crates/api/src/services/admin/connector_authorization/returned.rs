//! What the company's return and a person's share (ADR-171, ADR-174): the
//! attempt the browser kept, the signed-in account's own and sealed for the
//! holder it is finished for; a return that matches it and carries a code;
//! the declaration it was started under, still the server's, with the
//! client's secret and whether the server is enabled; the issuer the
//! declared one (RFC 9207); the code exchanged, counted with the company's
//! starts. Every refusal says why, for the audit; never a token, a code
//! nor a secret.

use mimorya_connectors::mcp::oauth::{
    ClientAuth, CodeExchange, Granted, OAuthDeclaration, OAuthHolder, exchange_code,
};
use mimorya_db::repositories::connectors::{ConnectorInstanceRow, get_connector_instance};
use serde::Deserialize;
use uuid::Uuid;

use super::attempt::{self, AuthorizationAttempt, declared_print};
use super::redirect_uri;
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::admin::connector_oauth::McpServer;
use crate::services::admin::connectors::ConnectorAdmin;
use crate::services::connectors::client_secret_name;
use crate::services::sso::{error_code, same_state};

/// What the authorization server sends back (RFC 6749 §4.1.2, RFC 9207).
#[derive(Debug, Default, Deserialize)]
pub struct OAuthReturn {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub iss: Option<String>,
}

/// A refused authorization: why (in French, never a token, a code nor a
/// secret), for the audit.
#[derive(Debug)]
pub struct AuthorizationFailure {
    pub connector_id: Option<Uuid>,
    pub reason: String,
    /// The account authorized is proven another address's than the
    /// person's (ADR-175): their account says so, for them to authorize
    /// again with their own.
    pub other_account: bool,
}

impl AuthorizationFailure {
    pub(super) fn new(connector_id: Option<Uuid>, reason: impl Into<String>) -> Self {
        Self {
            connector_id,
            reason: reason.into(),
            other_account: false,
        }
    }
}

pub(super) type Outcome<T> = Result<T, AuthorizationFailure>;

/// What a return keeps changing under it: refused at once when changed,
/// and read again under the lock before the tokens are kept.
pub(super) const CHANGED: &str = "Adresse ou déclaration OAuth changée pendant l’autorisation";

/// The attempt the browser kept, if it is the signed-in account's own and
/// sealed for `holder`: an administrator's is never finished as a person's,
/// nor a person's as the company's.
pub(super) fn own_attempt(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    cookie: Option<&str>,
    holder: OAuthHolder,
) -> Outcome<AuthorizationAttempt> {
    let unknown = || AuthorizationFailure::new(None, "Tentative inconnue ou expirée");
    let cipher = admin.secrets.cipher().map_err(|_| unknown())?;
    let now = chrono::Utc::now();
    let opened = cookie.and_then(|value| match holder {
        OAuthHolder::Company => attempt::open(cipher, value, now),
        OAuthHolder::Person => attempt::open_person(cipher, value, now),
    });
    let (tenant_id, attempt) = opened.ok_or_else(unknown)?;
    if tenant_id != access.subject.tenant_id || attempt.user_id != access.subject.user_id {
        let other = match holder {
            OAuthHolder::Company => "Tentative d’un autre administrateur",
            OAuthHolder::Person => "Tentative d’une autre personne",
        };
        return Err(AuthorizationFailure::new(None, other));
    }
    Ok(attempt)
}

/// The code the return carries, if it is this attempt's and not a refusal:
/// another attempt's return, or an authorization forced from elsewhere,
/// does not match its state, refused or not (RFC 6749 §4.1.2.1).
pub(super) fn returned_code(back: &OAuthReturn, attempt: &AuthorizationAttempt) -> Outcome<String> {
    let at = |reason: &str| AuthorizationFailure::new(Some(attempt.connector_id), reason);
    if !back
        .state
        .as_deref()
        .is_some_and(|state| same_state(state, &attempt.state))
    {
        return Err(at("Retour d’une autre tentative"));
    }
    if let Some(error) = &back.error {
        let code = error_code(error);
        return Err(at(&format!(
            "Refusée par le serveur d’autorisation ({code})"
        )));
    }
    back.code.clone().ok_or_else(|| at("Retour sans code"))
}

/// The declaration an attempt was started under, as the server holds it
/// now.
pub(super) struct AttemptServer {
    pub declared: OAuthDeclaration,
    /// The client's secret, if it proves itself with one.
    pub secret: Option<String>,
    pub enabled: bool,
}

/// The declaration the attempt was started under, still the server's, and
/// the client's secret if it proves itself with one.
pub(super) async fn declaration(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
) -> Outcome<AttemptServer> {
    let id = attempt.connector_id;
    let internal = |error: anyhow::Error| internal(id, error);
    let tenant_id = access.subject.tenant_id;
    let mut tx = admin
        .db
        .begin(tenant_id)
        .await
        .map_err(|e| internal(e.into()))?;
    let instance = get_connector_instance(&mut tx, id)
        .await
        .map_err(|e| internal(e.into()))?;
    let enabled = instance.as_ref().is_some_and(|instance| instance.enabled);
    let server = McpServer::of(instance)
        .map_err(|_| AuthorizationFailure::new(Some(id), "Connecteur supprimé"))?;
    let secret = admin
        .secrets
        .read(&mut tx, tenant_id, &client_secret_name(id))
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(|e| internal(e.into()))?;
    let declared = server
        .declared
        .filter(|declared| declared_print(&server.url, declared) == attempt.declared)
        .ok_or_else(|| AuthorizationFailure::new(Some(id), CHANGED))?;
    let secret = match (declared.client_auth, secret) {
        (ClientAuth::None, _) => None,
        (_, SecretRead::Value(secret)) => Some(secret),
        _ => {
            return Err(AuthorizationFailure::new(
                Some(id),
                "Secret du client absent ou illisible",
            ));
        }
    };
    Ok(AttemptServer {
        declared,
        secret,
        enabled,
    })
}

/// Why the return's issuer is refused, if it is (RFC 9207): another than
/// the declared one, or none when the server says it names it.
pub(super) fn issuer_refused(
    issuer: Option<&str>,
    declared: &OAuthDeclaration,
) -> Option<&'static str> {
    match issuer {
        Some(issuer) if issuer != declared.issuer => {
            Some("Retour d’un autre émetteur que le serveur d’autorisation déclaré")
        }
        None if declared.names_issuer_on_return => {
            Some("Retour sans l’émetteur que ce serveur d’autorisation annonce")
        }
        _ => None,
    }
}

/// The code exchanged at the declared token endpoint. Counted with the
/// company's starts: one company's exchanges stay bounded however many
/// come back at once.
pub(super) async fn exchanged(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
    server: &AttemptServer,
    code: &str,
    web_origin: &str,
) -> Outcome<Granted> {
    let at = |reason: &str| AuthorizationFailure::new(Some(attempt.connector_id), reason);
    let back_to = redirect_uri(web_origin);
    let exchange = CodeExchange {
        code,
        verifier: &attempt.pkce_verifier,
        redirect_uri: &back_to,
        client_secret: server.secret.as_deref(),
    };
    let _start = admin
        .connectors
        .claim_admin_start(access.subject.tenant_id)
        .map_err(|refusal| at(&refusal.to_string()))?;
    exchange_code(&server.declared, exchange, admin.connectors.network())
        .await
        .map_err(|failure| at(&failure))
}

/// Whether the instance read under the lock still has the address and the
/// declaration the attempt was started under.
pub(super) fn unchanged(
    instance: Option<ConnectorInstanceRow>,
    attempt: &AuthorizationAttempt,
) -> bool {
    McpServer::of(instance).ok().is_some_and(|server| {
        server
            .declared
            .is_some_and(|now| declared_print(&server.url, &now) == attempt.declared)
    })
}

/// The database failing: still a refusal for the browser.
pub(super) fn internal(connector_id: Uuid, error: anyhow::Error) -> AuthorizationFailure {
    tracing::error!(error = %error, "Connector authorization failed");
    AuthorizationFailure::new(Some(connector_id), "Erreur interne")
}
