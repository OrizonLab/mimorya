//! Where the company's SIEM reads its audit log (ADR-181), before any
//! session: its token, sent as `Authorization: Bearer <token>`, is its only
//! proof; a session cookie proves nothing here. `after` names the last place
//! the SIEM holds (0 to start), `limit` how many entries it takes at most.
//! A read changes nothing but when the token was last used: the request
//! header the browser's changes carry is not asked of it.

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use validator::Validate;

use crate::app::AppState;
use crate::contract::AuditExport;
use crate::http::{ApiResult, Query, bearer};
use crate::services::audit_export::{DEFAULT_PAGE, ExportPage, MAX_PAGE, export_audit_log};

pub fn routes() -> Router<AppState> {
    Router::new().route("/audit/export", get(export))
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct ExportQuery {
    #[serde(default)]
    #[validate(range(min = 0))]
    after: i64,
    #[serde(default = "default_page")]
    #[validate(range(min = 1, max = MAX_PAGE))]
    limit: i64,
}

fn default_page() -> i64 {
    DEFAULT_PAGE
}

async fn export(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Json<AuditExport>> {
    let page = ExportPage {
        after: query.after,
        limit: query.limit,
    };
    let export =
        export_audit_log(&state.db, &state.config, bearer(&headers), page, Utc::now()).await?;
    Ok(Json(export))
}
