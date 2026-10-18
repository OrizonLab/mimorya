//! What happened on the platform: the audit trail and model usage with its cost.

use axum::Json;
use axum::extract::State;
use chrono::{Duration, Utc};
use mimorya_db::repositories::audit::{AuditQuery, list_audit};
use mimorya_db::repositories::usage::usage_summary;
use serde::Deserialize;
use serde_json::{Value, json};
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, Query, iso};
use crate::plugins::Auth;

#[derive(Deserialize, Validate)]
pub struct AuditParams {
    #[validate(range(min = 1, max = 500))]
    limit: Option<i64>,
    #[validate(range(min = 1))]
    before: Option<i64>,
}

pub async fn audit(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Query(params): Query<AuditParams>,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide("audit:read")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = list_audit(
        &mut tx,
        AuditQuery {
            limit: Some(params.limit.unwrap_or(100)),
            before_id: params.before,
            run_id: None,
        },
    )
    .await?;
    tx.commit().await?;
    let entries: Vec<Value> = rows
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "occurredAt": iso(&e.occurred_at),
                "actorType": e.actor_type,
                "actorUserId": e.actor_user_id,
                "onBehalfOfUserId": e.on_behalf_of_user_id,
                "runId": e.run_id,
                "action": e.action,
                "resourceType": e.resource_type,
                "resourceId": e.resource_id,
                "outcome": e.outcome,
                "reason": e.reason,
                "metadata": e.metadata,
            })
        })
        .collect();
    Ok(Json(json!({ "entries": entries })))
}

#[derive(Deserialize, Validate)]
pub struct UsageParams {
    #[validate(range(min = 1, max = 366))]
    days: Option<i64>,
}

pub async fn usage(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Query(params): Query<UsageParams>,
) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide("usage:read")?;
    let since = Utc::now() - Duration::days(params.days.unwrap_or(30));
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let rows = usage_summary(&mut tx, since).await?;
    tx.commit().await?;
    let rows: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "provider": r.provider,
                "model": r.model,
                "kind": r.kind,
                "calls": r.calls,
                "inputTokens": r.input_tokens,
                "outputTokens": r.output_tokens,
                "costMicros": r.cost_micros,
            })
        })
        .collect();
    Ok(Json(json!({ "since": iso(&since), "rows": rows })))
}
