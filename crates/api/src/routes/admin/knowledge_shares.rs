//! A company's units' knowledge shared with other units (ADR-143,
//! `policy:manage`, company-wide): listing, making and removing the shares.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mimorya_core::knowledge_shares::KnowledgeShare;
use mimorya_db::repositories::knowledge_shares::list_knowledge_shares;

use crate::app::AppState;
use crate::contract::admin::{KnowledgeShareView, KnowledgeShares, NewKnowledgeShare};
use crate::contract::{Created, Done};
use crate::http::{ApiResult, Body, PathId};
use crate::plugins::Auth;
use crate::services::admin::{remove_knowledge_share, share_knowledge};

const PERMISSION: &str = "policy:manage";

pub async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<KnowledgeShares>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide(PERMISSION)?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_knowledge_shares(&mut tx).await?;
    tx.commit().await?;
    let shares = rows
        .iter()
        .filter_map(KnowledgeShareView::from_row)
        .collect();
    Ok(Json(KnowledgeShares { shares }))
}

pub async fn create(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Body(body): Body<NewKnowledgeShare>,
) -> ApiResult<(StatusCode, Json<Created>)> {
    let access = state.access.context(identity).await?;
    let share = KnowledgeShare {
        source_unit_id: body.source_org_unit_id,
        target_unit_id: body.target_org_unit_id,
        include_descendants: body.include_descendants.unwrap_or(false),
        memory: body.memory,
        documents: body.documents,
        ceiling: body.max_sensitivity.into(),
        expires_at: body.expires_at,
    };
    let id = share_knowledge(&state.db, &state.access, &access, share).await?;
    Ok((StatusCode::CREATED, Json(Created { id })))
}

pub async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    remove_knowledge_share(&state.db, &state.access, &access, id).await?;
    Ok(Json(Done::OK))
}
