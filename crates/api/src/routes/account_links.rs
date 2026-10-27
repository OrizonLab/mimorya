//! The accounts the signed-in person links at their company's providers
//! (ADR-116): where they may link one and what is linked; the start towards
//! the provider; its return, which sends the browser back to their account
//! page, saying only whether the link was made, refused, or made with
//! another account than theirs (the reason is audited); and the removal.

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mimorya_db::repositories::audit::{AuditEntry, append_audit};
use serde_json::json;

use crate::app::AppState;
use crate::auth::cookies::cookie_value;
use crate::contract::{AccountLinkStart, AccountLinkView, AccountLinks, Done, LinkedAccount};
use crate::http::{ApiError, ApiResult, PathId, iso};
use crate::plugins::{Auth, RequestContext};
use crate::services::AccessContext;
use crate::services::account_links::{
    LINK_COOKIE, LinkFailure, LinkRefused, cleared_link_cookie, link_cookie,
};
use crate::services::sso::Return;

/// Where the browser lands after the provider, the outcome in the query.
const LINKED: &str = "/account?liaison=ok";
const OTHER_ACCOUNT: &str = "/account?liaison=autre-compte";
const FAILED: &str = "/account?liaison=echec";
/// Without a session, nothing can be linked: the sign-in page.
const SIGNED_OUT: &str = "/login";

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/links", get(list))
        .route("/account/links/callback", get(callback))
        .route("/account/links/{id}", post(start).delete(remove))
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<AccountLinks>> {
    let access = state.access.context(identity).await?;
    let linkables = state.account_links.list(&access).await?;
    let connectors = linkables
        .into_iter()
        .map(|linkable| AccountLinkView {
            connector_id: linkable.connector_id,
            name: linkable.name,
            connector_type: linkable.connector_type,
            available: linkable.available,
            write_tools: linkable.write_tools,
            linked: linkable.link.map(|link| LinkedAccount {
                address: link.address,
                scopes: link.scopes.split_whitespace().map(Into::into).collect(),
                to_grant: linkable.to_grant,
                linked_at: iso(&link.created_at),
            }),
        })
        .collect();
    Ok(Json(AccountLinks { connectors }))
}

/// Counted with the sign-ins through a provider: each start reads the
/// provider's declaration.
async fn start(
    State(state): State<AppState>,
    context: RequestContext,
    Auth(identity): Auth,
    PathId(connector_id): PathId,
) -> ApiResult<impl IntoResponse> {
    if !state
        .limits
        .sso_sign_in
        .allow(&context.client_key())
        .await?
    {
        return Err(ApiError::RateLimited);
    }
    let access = state.access.context(identity).await?;
    let started = state.account_links.start(&access, connector_id).await?;
    Ok((
        AppendHeaders([(
            SET_COOKIE,
            link_cookie(&started.attempt, state.config.secure_cookies),
        )]),
        Json(AccountLinkStart {
            redirect_url: started.redirect_url,
        }),
    ))
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(connector_id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    state.account_links.unlink(&access, connector_id).await?;
    Ok(Json(Done::OK))
}

/// A top-level navigation from the provider: answered by a redirection,
/// the attempt's cookie cleared either way. Counted with the start: each
/// return with a sealed attempt calls the provider.
async fn callback(
    State(state): State<AppState>,
    context: RequestContext,
    signed_in: Result<Auth, ApiError>,
    headers: HeaderMap,
    back: Result<Query<Return>, QueryRejection>,
) -> Response {
    let cleared = (SET_COOKIE, cleared_link_cookie(state.config.secure_cookies));
    let away = |to: &str| {
        (
            StatusCode::SEE_OTHER,
            AppendHeaders([(LOCATION, to.to_string()), cleared.clone()]),
        )
            .into_response()
    };
    let Ok(Auth(identity)) = signed_in else {
        return away(SIGNED_OUT);
    };
    let access = match state.access.context(identity).await {
        Ok(access) => access,
        Err(_) => return away(SIGNED_OUT),
    };
    let allowed = state
        .limits
        .sso_sign_in
        .allow(&context.client_key())
        .await
        .unwrap_or(false);
    let outcome = if allowed {
        // An address that does not read as a return reads as an empty one.
        let back = back.map(|Query(back)| back).unwrap_or_default();
        let cookie = cookie_value(&headers, LINK_COOKIE);
        state
            .account_links
            .finish(&access, back, cookie.as_deref())
            .await
    } else {
        Err(LinkFailure {
            connector_id: None,
            reason: "Trop de tentatives".into(),
            refused: LinkRefused::Failed,
        })
    };
    match outcome {
        Ok(()) => away(LINKED),
        Err(failure) => {
            let to = match failure.refused {
                LinkRefused::OtherAccount => OTHER_ACCOUNT,
                LinkRefused::Failed => FAILED,
            };
            record_refusal(&state, &context, &access, failure).await;
            away(to)
        }
    }
}

/// Never a token nor a code.
async fn record_refusal(
    state: &AppState,
    context: &RequestContext,
    access: &AccessContext,
    failure: LinkFailure,
) {
    let tenant_id = access.subject.tenant_id;
    let mut entry = AuditEntry::new(tenant_id, "user", "account.link.created", "denied")
        .by_user(access.subject.user_id)
        .reason(failure.reason.chars().take(300).collect::<String>())
        .ip(context.ip_string());
    if let Some(connector_id) = failure.connector_id {
        entry = entry.resource("connector", connector_id);
    }
    entry = entry.metadata(json!({ "otherAccount": failure.refused == LinkRefused::OtherAccount }));
    entry.request_id = Some(context.id.to_string());
    let written = async {
        let mut tx = state.db.begin(tenant_id).await?;
        append_audit(&mut tx, entry).await?;
        tx.commit().await
    };
    if let Err(error) = written.await {
        tracing::warn!(error = %error, "Failed to audit a refused account link");
    }
}
