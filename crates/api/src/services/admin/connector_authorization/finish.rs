//! The authorization server's return (ADR-171): the attempt the browser kept
//! must be the signed-in administrator's own and match the return, its
//! issuer the declared one (RFC 9207: checked if given, required if the
//! server says it names it); the administrator must still administer the
//! company's connectors, and the server's address and declaration be those
//! the attempt was started under (`returned`). The code is then exchanged,
//! the tokens sealed for the connector and kept, the previous authorization
//! replaced; tokens granted and not kept are revoked (ADR-172). Every
//! refusal says why, for the audit; the browser learns only whether it was
//! authorized.

use mimorya_connectors::mcp::oauth::{Granted, OAuthDeclaration, OAuthHolder};
use mimorya_db::repositories::audit::append_audit;
use mimorya_db::repositories::connector_authorizations::{
    NewConnectorAuthorization, put_connector_authorization,
};
use mimorya_db::repositories::connectors::get_connector_instance;
use serde_json::json;
use uuid::Uuid;

use super::attempt::AuthorizationAttempt;
use super::returned::{
    AuthorizationFailure, CHANGED, OAuthReturn, Outcome, declaration, exchanged, internal,
    issuer_refused, own_attempt, returned_code, unchanged,
};
use crate::http::iso;
use crate::services::AccessContext;
use crate::services::admin::connectors::{ConnectorAdmin, PERMISSION};
use crate::services::admin::{audit_entry, begin_change};
use crate::services::connectors::{Unkept, seal_grant};

pub async fn finish_authorization(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    back: OAuthReturn,
    cookie: Option<&str>,
    web_origin: &str,
) -> Outcome<Uuid> {
    let attempt = own_attempt(admin, access, cookie, OAuthHolder::Company)?;
    let id = attempt.connector_id;
    let at = |reason: &str| AuthorizationFailure::new(Some(id), reason);
    let code = returned_code(&back, &attempt)?;
    // Its rights may have changed since the start.
    access
        .require_tenant_wide(PERMISSION)
        .map_err(|_| at("Droits d’administration des connecteurs retirés"))?;
    let server = declaration(admin, access, &attempt).await?;
    if let Some(refused) = issuer_refused(back.iss.as_deref(), &server.declared) {
        return Err(at(refused));
    }
    let (granted, start) = exchanged(admin, access, &attempt, &server, &code, web_origin).await?;
    drop(start);
    let kept = keep(admin, access, &attempt, &server.declared, &granted).await;
    if kept.is_err() {
        // Granted, kept nowhere (ADR-172).
        admin.connectors.revoke_unkept(Unkept {
            tenant_id: access.subject.tenant_id,
            connector_id: id,
            user_id: None,
            declared: server.declared,
            granted,
            client_secret: server.secret,
        });
    }
    kept
}

/// The tokens sealed for the connector and kept, if the declaration is
/// still the one they were issued under; audited.
async fn keep(
    admin: &ConnectorAdmin<'_>,
    access: &AccessContext,
    attempt: &AuthorizationAttempt,
    declared: &OAuthDeclaration,
    granted: &Granted,
) -> Outcome<Uuid> {
    let id = attempt.connector_id;
    let internal = |error: anyhow::Error| internal(id, error);
    let tenant_id = access.subject.tenant_id;
    let cipher = admin.secrets.cipher().map_err(|e| internal(e.into()))?;
    let sealed = seal_grant(cipher, tenant_id, id, granted).map_err(internal)?;
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
    if !unchanged(instance, attempt) {
        return Err(AuthorizationFailure::new(Some(id), CHANGED));
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
