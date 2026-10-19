//! The company's skills (managed elsewhere: this screen lists them).

use crate::app::AppState;
use crate::contract::admin::{SkillList, SkillView};
use crate::http::ApiResult;
use crate::plugins::Auth;
use axum::Json;
use axum::extract::State;
use mimorya_db::repositories::skills::list_skills;

/// Every member reads skills through the agent; this screen is for their managers.
pub async fn skills(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<SkillList>> {
    let access = state.access.context(identity).await?;
    access.require_tenant_wide("skill:manage")?;
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let skills = list_skills(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(SkillList {
        skills: skills
            .iter()
            .map(SkillView::from_row)
            .collect::<anyhow::Result<_>>()?,
    }))
}
