//! OAuth on an MCP server (`connector:manage`, company-wide, ADR-171):
//! declared, where the discovery finds it is authorized, and withdrawn. The
//! client's secret is write-only.

use axum::Json;
use axum::extract::State;
use mimorya_connectors::mcp::oauth::OAuthDeclaration;

use super::connectors::{admin, secrets};
use crate::app::AppState;
use crate::contract::Done;
use crate::contract::admin::{McpOAuth, McpOAuthDeclaration};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{OAuthInput, declare_oauth, remove_oauth};
use crate::services::permission_catalog::permission_catalog;

pub async fn declare(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    Body(body): Body<McpOAuthDeclaration>,
) -> ApiResult<Json<McpOAuth>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    let input = OAuthInput {
        issuer: body.issuer.filter(|issuer| !issuer.is_empty()),
        client_id: body.client_id,
        client_secret: body.client_secret.filter(|secret| !secret.is_empty()),
        scopes: body.scopes,
    };
    let has_secret = input.client_secret.is_some();
    let declared: OAuthDeclaration =
        declare_oauth(&admin(&state, &store, &catalog), &access, id, input).await?;
    Ok(Json(McpOAuth::new(declared, has_secret)))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    let (store, catalog) = (secrets(&state), permission_catalog());
    remove_oauth(&admin(&state, &store, &catalog), &access, id).await?;
    Ok(Json(Done::OK))
}
