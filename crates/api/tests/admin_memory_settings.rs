//! Whether a lesson is drawn from the company's runs (ADR-076): chosen by
//! its company-wide settings administrators only, off by default, audited.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::lessons::capture_lessons;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_settings_administrators_choose_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let on = json!({ "captureLessons": true });
    assert_eq!(
        marc.get("/admin/memory-settings").await.status,
        StatusCode::FORBIDDEN
    );
    let refused = marc.put("/admin/memory-settings", on.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.lecons").await;
    let partial = activate(&api, &mut partial).await;
    let refused = partial.put("/admin/memory-settings", on.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let unknown = paul
        .put(
            "/admin/memory-settings",
            json!({ "captureLessons": true, "scope": "tenant" }),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        paul.get("/admin/memory-settings").await.json()["captureLessons"],
        false
    );

    capture_lessons(&paul, true).await;
    capture_lessons(&paul, false).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<Value> = sqlx::query_scalar(
        "select metadata from (select id, metadata from audit_log \
         where action = 'memory.lessons_configured' order by id desc limit 2) last \
         order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        audited,
        [
            json!({ "captureLessons": true }),
            json!({ "captureLessons": false })
        ]
    );
}
