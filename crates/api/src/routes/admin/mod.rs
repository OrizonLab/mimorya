//! Read-only administration endpoints, each reserved to its permission
//! (management screens come next, see docs/ROADMAP.md).

mod activity;
mod catalog;
mod directory;

use axum::Router;
use axum::routing::get;

use crate::app::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/organization", get(directory::organization))
        .route("/admin/users", get(directory::users))
        .route("/admin/roles", get(catalog::roles))
        .route("/admin/connectors", get(catalog::connectors))
        .route("/admin/skills", get(catalog::skills))
        .route("/admin/audit", get(activity::audit))
        .route("/admin/usage", get(activity::usage))
}
