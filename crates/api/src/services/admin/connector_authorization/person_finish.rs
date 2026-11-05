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

use mimorya_connectors::mcp::oauth::{Granted, OAuthDeclaration, OAuthHolder};
use mimorya_db::repositories::account_links::lock_active_account;
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, put_person_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use serde_json::json;
use uuid::Uuid;

use super::attempt::{self, AuthorizationAttempt};
use super::returned::{
    AuthorizationFailure, CHANGED, OAuthReturn, Outcome, declaration, exchanged, internal,
    issuer_refused, own_attempt, returned_code, unchanged,
};
use crate::http::iso;
use crate::services::AccessContext;
use crate::services::admin::audit_entry;
use crate::services::admin::connectors::{ConnectorAdmin, PERMISSION};
use crate::services::connectors::{CONNECTOR_USE, Unkept, seal_person_grant};

/// What the audit calls a person's own authorization.
pub const PERSON_AUTHORIZED: &str = "connector.person_oauth_authorized";

/// A server disabled between the start and the return: nothing is asked.
const DISABLED: &str = "Serveur désactivé pendant l’autorisation";

/// Whose return this is: the attempt's that opens, an administrator's for
/// the company or a person's own, finished here; with none, a person's if
/// the caller does not administer the company's connectors, so that it is
/// refused as theirs and they go back to their account.
pub fn attempt_holder(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    cookie: Option<&str>,
) -> OAuthHolder {
    let opened = admin.secrets.cipher().ok().and_then(|cipher| {
        let (value, now) = (cookie?, chrono::Utc::now());
        if attempt::open_person(cipher, value, now).is_some() {
            Some(OAuthHolder::Person)
        } else if attempt::open(cipher, value, now).is_some() {
            Some(OAuthHolder::Company)
        } else {
            None
        }
    });
    opened.unwrap_or_else(|| match access.require_tenant_wide(PERMISSION) {
        Ok(()) => OAuthHolder::Company,
        Err(_) => OAuthHolder::Person,
    })
}

pub async fn finish_person_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    back: OAuthReturn,
    cookie: Option<&str>,
    web_origin: &str,
) -> Outcome<Uuid> {
    let attempt = own_attempt(admin, access, cookie, OAuthHolder::Person)?;
    let id = attempt.connector_id;
    let at = |reason: &str| AuthorizationFailure::new(Some(id), reason);
    let code = returned_code(&back, &attempt)?;
    // Their rights may have changed since the start.
    if !access.has(CONNECTOR_USE) {
        return Err(at("Connecteurs non autorisés"));
    }
    let server = declaration(admin, access, &attempt).await?;
    // Nothing is asked for a server that is disabled: checked again once
    // granted, under the lock.
    if !server.enabled {
        return Err(at(DISABLED));
    }
    if let Some(refused) = issuer_refused(back.iss.as_deref(), &server.declared) {
        return Err(at(refused));
    }
    let granted = exchanged(admin, access, &attempt, &server, &code, web_origin).await?;
    let kept = keep(admin, access, &attempt, &server.declared, &granted).await;
    if kept.is_err() {
        // Granted, kept nowhere (ADR-172).
        admin.connectors.revoke_unkept(Unkept {
            tenant_id: access.subject.tenant_id,
            connector_id: id,
            user_id: Some(access.subject.user_id),
            declared: server.declared,
            granted,
            client_secret: server.secret,
        });
    }
    kept
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
    if !unchanged(instance, attempt) {
        return Err(refused(CHANGED));
    }
    if !enabled {
        return Err(refused(DISABLED));
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
