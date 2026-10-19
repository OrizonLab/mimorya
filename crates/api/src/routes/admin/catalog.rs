//! The company's skills (managed elsewhere: this screen lists them).

use axum::Json;
use axum::extract::State;
use mimorya_db::repositories::skills::list_skills;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::http::{ApiResult, iso};
use crate::plugins::Auth;

/// Every member reads skills through the agent; this screen is for their managers.
pub async fn skills(State(state): State<AppState>, Auth(identity): Auth) -> ApiResult<Json<Value>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide("skill:manage")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let skills = list_skills(&mut tx).await?;
    tx.commit().await?;
    let skills: Vec<Value> = skills
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "description": s.description,
                "status": s.status,
                "activeVersion": s.active_version,
                "updatedAt": iso(&s.updated_at),
            })
        })
        .collect();
    Ok(Json(json!({ "skills": skills })))
}
