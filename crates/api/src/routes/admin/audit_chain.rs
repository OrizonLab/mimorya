//! The verification of its company's audit chain an administrator asks for
//! (`audit:read`, company-wide, ADR-179), against the anchors they kept.

use axum::Json;
use axum::extract::State;
use chrono::Utc;

use crate::app::AppState;
use crate::contract::admin::{AuditChainCheck, AuditChainVerification};
use crate::http::{ApiResult, Body};
use crate::plugins::Auth;
use crate::services::audit_chain::verify_own_chain;

pub async fn verify(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<AuditChainCheck>,
) -> ApiResult<Json<AuditChainVerification>> {
    let access = state.access.context(identity).await?;
    let check = verify_own_chain(
        &state.db,
        &state.config,
        &state.audit_checks,
        &access,
        &body.anchors,
    )
    .await?;
    Ok(Json(AuditChainVerification::of(&check, Utc::now())))
}
