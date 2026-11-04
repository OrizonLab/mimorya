//! The authorization server's return (ADR-171): the attempt the browser kept
//! must be the signed-in administrator's own and match the return, its
//! issuer the declared one (RFC 9207: checked if given, required if the
//! server says it names it); the administrator must still administer the
//! company's connectors, and the server's address and declaration be those
//! the attempt was started under. The code is then exchanged, the tokens
//! sealed for the connector and kept, the previous authorization replaced.
//! Every refusal says why, for the audit; the browser learns only whether
//! it was authorized.

use mimorya_connectors::mcp::oauth::{ClientAuth, CodeExchange, OAuthDeclaration, exchange_code};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_authorizations::{
    NewConnectorAuthorization, put_connector_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::attempt::{self, AuthorizationAttempt, declared_print};
use super::redirect_uri;
use crate::http::iso;
use crate::secrets::SecretRead;
use crate::services::AccessContext;
use crate::services::admin::connector_oauth::McpServer;
use crate::services::admin::connectors::{ConnectorAdmin, PERMISSION};
use crate::services::admin::{audit_entry, begin_change};
use crate::services::connectors::{client_secret_name, seal_grant};
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
}

impl AuthorizationFailure {
    fn new(connector_id: Option<Uuid>, reason: impl Into<String>) -> Self {
        Self {
            connector_id,
            reason: reason.into(),
        }
    }
}

type Outcome<T> = Result<T, AuthorizationFailure>;

pub async fn finish_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    back: OAuthReturn,
    cookie: Option<&str>,
    web_origin: &str,
) -> Outcome<Uuid> {
    let attempt = own_attempt(admin, access, cookie)?;
    let id = attempt.connector_id;
    let at = |reason: &str| AuthorizationFailure::new(Some(id), reason);
    // Another attempt's return, an authorization forced from elsewhere:
    // the state comes back with a refusal too (RFC 6749 §4.1.2.1).
    if !back
        .state
        .as_deref()
        .is_some_and(|state| same_state(state, &attempt.state))
    {
        return Err(at("Retour d’une autre tentative"));
    }
    if let Some(error) = back.error {
        let code = error_code(&error);
        return Err(at(&format!(
            "Refusée par le serveur d’autorisation ({code})"
        )));
    }
    let code = back.code.ok_or_else(|| at("Retour sans code"))?;
    // Its rights may have changed since the start.
    access
        .require_tenant_wide(PERMISSION)
        .map_err(|_| at("Droits d’administration des connecteurs retirés"))?;
    let (declared, secret) = declaration(admin, access, &attempt).await?;
    match back.iss {
        Some(issuer) if issuer != declared.issuer => {
            return Err(at(
                "Retour d’un autre émetteur que le serveur d’autorisation déclaré",
            ));
        }
        None if declared.names_issuer_on_return => {
            return Err(at(
                "Retour sans l’émetteur que ce serveur d’autorisation annonce",
            ));
        }
        _ => {}
    }
    let back_to = redirect_uri(web_origin);
    let exchange = CodeExchange {
        code: &code,
        verifier: &attempt.pkce_verifier,
        redirect_uri: &back_to,
        client_secret: secret.as_deref(),
    };
    let granted = {
        let _start = admin
            .connectors
            .claim_admin_start(access.subject.tenant_id)
            .map_err(|refusal| at(&refusal.to_string()))?;
        exchange_code(&declared, exchange, admin.connectors.network())
            .await
            .map_err(|failure| at(&failure))?
    };
    keep(admin, access, &attempt, &declared, granted).await
}

/// The attempt the browser kept, if it is the signed-in administrator's own.
fn own_attempt(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    cookie: Option<&str>,
) -> Outcome<AuthorizationAttempt> {
    let unknown = || AuthorizationFailure::new(None, "Tentative inconnue ou expirée");
    let cipher = admin.secrets.cipher().map_err(|_| unknown())?;
    let (tenant_id, attempt) = cookie
        .and_then(|value| attempt::open(cipher, value, chrono::Utc::now()))
        .ok_or_else(unknown)?;
    if tenant_id != access.subject.tenant_id || attempt.user_id != access.subject.user_id {
        return Err(AuthorizationFailure::new(
            None,
            "Tentative d’un autre administrateur",
        ));
    }
    Ok(attempt)
}

/// The declaration the attempt was started under, still the server's, and
/// the client's secret if it proves itself with one.
async fn declaration(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
) -> Outcome<(OAuthDeclaration, Option<String>)> {
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
        .ok_or_else(|| {
            AuthorizationFailure::new(
                Some(id),
                "Adresse ou déclaration OAuth changée pendant l’autorisation",
            )
        })?;
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
    Ok((declared, secret))
}

/// The tokens sealed for the connector and kept, if the declaration is
/// still the one they were issued under; audited.
async fn keep(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
    declared: &OAuthDeclaration,
    granted: mimorya_connectors::mcp::oauth::Granted,
) -> Outcome<Uuid> {
    let id = attempt.connector_id;
    let internal = |error: anyhow::Error| internal(id, error);
    let tenant_id = access.subject.tenant_id;
    let cipher = admin.secrets.cipher().map_err(|e| internal(e.into()))?;
    let sealed = seal_grant(cipher, tenant_id, id, &granted).map_err(internal)?;
    let (access_token, refresh_token, expires_at) =
        (sealed.access, sealed.refresh, sealed.expires_at);
    // Not said by the server: those asked (RFC 6749 §5.1).
    let scopes = sealed.scopes.unwrap_or_else(|| declared.scopes.join(" "));
    // Under the administration lock, the administrator's rights read again.
    let mut tx = begin_change(admin.db, access, PERMISSION)
        .await
        .map_err(|error| AuthorizationFailure::new(Some(id), error.to_string()))?;
    let instance = get_connector_instance(&mut tx, id)
        .await
        .map_err(|e| internal(e.into()))?;
    let unchanged = McpServer::of(instance).ok().is_some_and(|server| {
        server
            .declared
            .is_some_and(|now| declared_print(&server.url, &now) == attempt.declared)
    });
    if !unchanged {
        return Err(AuthorizationFailure::new(
            Some(id),
            "Adresse ou déclaration OAuth changée pendant l’autorisation",
        ));
    }
    let replaced = put_connector_authorization(
        &mut tx,
        NewConnectorAuthorization {
            tenant_id,
            connector_id: id,
            issuer: &declared.issuer,
            client_id: &declared.client_id,
            resource: &declared.resource,
            scopes: &scopes,
            access_token: &access_token,
            access_expires_at: expires_at,
            refresh_token: refresh_token.as_ref(),
            authorized_by_user_id: access.subject.user_id,
        },
    )
    .await
    .map_err(|e| internal(e.into()))?;
    append_audit(
        &mut tx,
        audit_entry(access, "connector.oauth_authorized", "connector", id).metadata(json!({
            "issuer": declared.issuer,
            "scopes": scopes,
            "renewable": refresh_token.is_some(),
            "expiresAt": expires_at.as_ref().map(iso),
            "replaced": replaced,
        })),
    )
    .await
    .map_err(|e| internal(e.into()))?;
    tx.commit().await.map_err(|e| internal(e.into()))?;
    admin.connectors.invalidate(id).await;
    Ok(id)
}

/// The database failing: still a refusal for the browser.
fn internal(connector_id: Uuid, error: anyhow::Error) -> AuthorizationFailure {
    tracing::error!(error = %error, "Connector authorization failed");
    AuthorizationFailure::new(Some(connector_id), "Erreur interne")
}
