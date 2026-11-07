//! The tokens with which the company's SIEM reads its audit log
//! (`audit:export`, company-wide, ADR-181): listed without their secret,
//! made with a name, the secret shown this once, and removed.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{
    AuditExportTokenCreated, AuditExportTokenView, AuditExportTokens, NewAuditExportToken,
    export_url,
};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::audit_export::{
    audit_export_tokens, create_audit_export_token, delete_audit_export_token,
};

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<AuditExportTokens>> {
    let access = state.access.context(identity).await?;
    let tokens = audit_export_tokens(&state.db, &access).await?;
    Ok(Json(AuditExportTokens {
        tokens: tokens.iter().map(AuditExportTokenView::from).collect(),
        sealing: state.config.secrets.is_some(),
        export_url: export_url(&state.config.web_origin),
    }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewAuditExportToken>,
) -> ApiResult<(StatusCode, Json<AuditExportTokenCreated>)> {
    let access = state.access.context(identity).await?;
    let (token, secret) = create_audit_export_token(&state.db, &access, &body.name).await?;
    let made = AuditExportTokenCreated {
        token: AuditExportTokenView::from(&token),
        secret,
    };
    Ok((StatusCode::CREATED, Json(made)))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    delete_audit_export_token(&state.db, &access, id).await?;
    Ok(Json(Done::OK))
}
