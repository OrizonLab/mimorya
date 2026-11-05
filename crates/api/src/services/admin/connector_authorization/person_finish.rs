//! The return of a person's own authorization of an MCP server (ADR-174), at
//! the same address as the company's: the attempt the browser kept must be
//! the signed-in person's own, sealed as a person's (an administrator's is
//! never finished here, nor a person's there), and match the return, its
//! issuer the declared one (RFC 9207); the person must still be allowed to
//! use connectors, the server's address and declaration be those the attempt
//! was started under (each person authorizing it), and the server enabled.
//! The code is then exchanged, the tokens sealed for the connector and the
//! person, and kept under the administration lock with the person's account
//! held active: a change of the declaration, which takes every person's
//! authorization, or a deactivation, which takes the person's, either waits
//! for it or is seen by it. Tokens granted and not kept are revoked
//! (ADR-172). Every refusal says why, for the audit.

use mimorya_connectors::mcp::oauth::{CodeExchange, Granted, OAuthDeclaration, exchange_code};
use mimorya_db::repositories::account_links::lock_active_account;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, put_person_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use serde_json::json;
use uuid::Uuid;

use super::attempt::{self, AuthorizationAttempt, declared_print};
use super::finish::{
    AuthorizationFailure, OAuthReturn, Outcome, declaration, internal, issuer_refused,
};
use super::redirect_uri;
use crate::http::iso;
use crate::services::AccessContext;
use crate::services::admin::audit_entry;
use crate::services::admin::connector_oauth::McpServer;
use crate::services::admin::connectors::ConnectorAdmin;
use crate::services::connectors::{CONNECTOR_USE, Unkept, seal_person_grant};
use crate::services::sso::{error_code, same_state};

/// What the audit calls a person's own authorization.
pub const PERSON_AUTHORIZED: &str = "connector.person_oauth_authorized";

/// Whether the browser's attempt is a person's own: its return is finished
/// here, not as an administrator's.
pub fn is_person_attempt(admin: &ConnectorAdmin<'_>, cookie: Option<&str>) -> bool {
    let Ok(cipher) = admin.secrets.cipher() else {
        return false;
    };
    cookie.is_some_and(|value| attempt::open_person(cipher, value, chrono::Utc::now()).is_some())
}

pub async fn finish_person_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    back: OAuthReturn,
    cookie: Option<&str>,
    web_origin: &str,
) -> Outcome<Uuid> {
    let attempt = own_attempt(admin, access, cookie)?;
    let id = attempt.connector_id;
    let at = |reason: &str| AuthorizationFailure::new(Some(id), reason);
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
    // Their rights may have changed since the start.
    if !access.has(CONNECTOR_USE) {
        return Err(at("Connecteurs non autorisés"));
    }
    let (declared, secret) = declaration(admin, access, &attempt).await?;
    if let Some(refused) = issuer_refused(back.iss.as_deref(), &declared) {
        return Err(at(refused));
    }
    let back_to = redirect_uri(web_origin);
    let exchange = CodeExchange {
        code: &code,
        verifier: &attempt.pkce_verifier,
        redirect_uri: &back_to,
        client_secret: secret.as_deref(),
    };
    let granted = {
        // Counted with the company's starts: one company's exchanges stay
        // bounded however many people come back at once.
        let _start = admin
            .connectors
            .claim_admin_start(access.subject.tenant_id)
            .map_err(|refusal| at(&refusal.to_string()))?;
        exchange_code(&declared, exchange, admin.connectors.network())
            .await
            .map_err(|failure| at(&failure))?
    };
    let kept = keep(admin, access, &attempt, &declared, &granted).await;
    if kept.is_err() {
        // Granted, kept nowhere (ADR-172).
        admin.connectors.revoke_unkept(Unkept {
            tenant_id: access.subject.tenant_id,
            connector_id: id,
            user_id: Some(access.subject.user_id),
            declared,
            granted,
            client_secret: secret,
        });
    }
    kept
}

/// The attempt the browser kept, if it is the signed-in person's own.
fn own_attempt(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    cookie: Option<&str>,
) -> Outcome<AuthorizationAttempt> {
    let unknown = || AuthorizationFailure::new(None, "Tentative inconnue ou expirée");
    let cipher = admin.secrets.cipher().map_err(|_| unknown())?;
    let (tenant_id, attempt) = cookie
        .and_then(|value| attempt::open_person(cipher, value, chrono::Utc::now()))
        .ok_or_else(unknown)?;
    if tenant_id != access.subject.tenant_id || attempt.user_id != access.subject.user_id {
        return Err(AuthorizationFailure::new(
            None,
            "Tentative d’une autre personne",
        ));
    }
    Ok(attempt)
}

/// The tokens sealed for the connector and the person and kept, if the
/// declaration is still the one they were issued under, the server enabled
/// and the account active; audited.
async fn keep(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
    declared: &OAuthDeclaration,
    granted: &Granted,
) -> Outcome<Uuid> {
    let id = attempt.connector_id;
    let refused = |reason: &str| AuthorizationFailure::new(Some(id), reason);
    let internal = |error: anyhow::Error| internal(id, error);
    let (tenant_id, user_id) = (access.subject.tenant_id, access.subject.user_id);
    let cipher = admin.secrets.cipher().map_err(|e| internal(e.into()))?;
    let sealed = seal_person_grant(cipher, tenant_id, id, user_id, granted).map_err(internal)?;
    let scopes = sealed
        .scopes
        .clone()
        .unwrap_or_else(|| declared.scopes.join(" "));
    let mut tx = admin
        .db
        .begin(tenant_id)
        .await
        .map_err(|e| internal(e.into()))?;
    tx.lock_administration()
        .await
        .map_err(|e| internal(e.into()))?;
    let active = lock_active_account(&mut tx, user_id)
        .await
        .map_err(|e| internal(e.into()))?;
    if !active {
        return Err(refused("Compte désactivé pendant l’autorisation"));
    }
    let instance = get_connector_instance(&mut tx, id)
        .await
        .map_err(|e| internal(e.into()))?;
    let enabled = instance.as_ref().is_some_and(|instance| instance.enabled);
    let unchanged = McpServer::of(instance).ok().is_some_and(|server| {
        server
            .declared
            .is_some_and(|now| declared_print(&server.url, &now) == attempt.declared)
    });
    if !unchanged {
        return Err(refused(
            "Adresse ou déclaration OAuth changée pendant l’autorisation",
        ));
    }
    if !enabled {
        return Err(refused("Serveur désactivé pendant l’autorisation"));
    }
    let replaced = put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id,
            connector_id: id,
            user_id,
            issuer: &declared.issuer,
            client_id: &declared.client_id,
            resource: &declared.resource,
            scopes: &scopes,
            access_token: &sealed.access,
            access_expires_at: sealed.expires_at,
            refresh_token: sealed.refresh.as_ref(),
        },
    )
    .await
    .map_err(|e| internal(e.into()))?;
    append_audit(
        &mut tx,
        audit_entry(access, PERSON_AUTHORIZED, "connector", id).metadata(json!({
            "issuer": declared.issuer,
            "scopes": scopes,
            "renewable": sealed.refresh.is_some(),
            "expiresAt": sealed.expires_at.as_ref().map(iso),
            "replaced": replaced,
        })),
    )
    .await
    .map_err(|e| internal(e.into()))?;
    tx.commit().await.map_err(|e| internal(e.into()))?;
    Ok(id)
}
