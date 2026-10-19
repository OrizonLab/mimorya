//! Access policies the API did not write (ADR-022): allow policies and another
//! company's are out of reach; a stored deny policy that the current rules
//! refuse (hand-written, or older than a rule) is never enabled, yet can still
//! be renamed or switched off.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::TestApi;
use uuid::Uuid;

fn cancelled_only() -> Value {
    json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "cancelled" } })
}

/// A deny policy inserted without the API's checks.
async fn insert_deny(
    api: &TestApi,
    permissions: &[&str],
    condition: Value,
    enabled: bool,
) -> String {
    let id = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition, enabled) values ($1, $2, 'Écrite à la main', 'deny', $3, $4, $5)",
    )
    .bind(id)
    .bind(tenant::ID)
    .bind(permissions)
    .bind(condition)
    .bind(enabled)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    format!("/admin/policies/{id}")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn allow_policies_and_other_companies_policies_are_out_of_reach() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (allow, foreign) = (Uuid::new_v4(), Uuid::new_v4());
    let insert = "insert into access_policies (id, tenant_id, description, effect, permissions, \
                  condition) values ($1, $2, 'Insérée par le test', $3, '{intervention:read}', $4)";
    for (id, tenant_id, effect) in [
        (allow, tenant::ID, "allow"),
        (foreign, OTHER_TENANT_ID, "deny"),
    ] {
        let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
        sqlx::query(insert)
            .bind(id)
            .bind(tenant_id)
            .bind(effect)
            .bind(cancelled_only())
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    let listing = paul.get("/admin/policies").await.json();
    let policies = listing["policies"].as_array().unwrap();
    let listed_allow = policies.iter().find(|p| p["id"] == json!(allow)).unwrap();
    assert_eq!(listed_allow["editable"], false);
    assert!(policies.iter().all(|p| p["id"] != json!(foreign)));

    let narrowed = json!({ "permissions": ["planning:read"] });
    let allow_url = format!("/admin/policies/{allow}");
    assert_eq!(
        paul.patch(&allow_url, narrowed.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(paul.delete(&allow_url).await.status, StatusCode::FORBIDDEN);
    let foreign_url = format!("/admin/policies/{foreign}");
    assert_eq!(
        paul.patch(&foreign_url, narrowed).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        paul.delete(&foreign_url).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_unreadable_stored_condition_is_never_enabled() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Disabled: the engine never loads it.
    let url = insert_deny(&api, &["intervention:read"], json!({ "all": [] }), false).await;

    let enabled = paul.patch(&url, json!({ "enabled": true })).await;
    assert_eq!(
        enabled.status,
        StatusCode::BAD_REQUEST,
        "{}",
        enabled.text()
    );
    // Still renamed while it stays off.
    let renamed = paul
        .patch(
            &url,
            json!({ "description": "À corriger", "enabled": false }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    // Replacing the condition makes it usable again.
    let fixed = paul
        .patch(
            &url,
            json!({ "enabled": true, "condition": cancelled_only() }),
        )
        .await;
    assert_eq!(fixed.status, StatusCode::OK, "{}", fixed.text());
    assert_eq!(paul.get("/auth/me").await.status, StatusCode::OK);
    // The tests of this file share a database.
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn stored_permissions_the_rules_refuse_are_never_enabled() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Enabled, it would lock every administrator out of policies, accounts and roles.
    let everyone = json!({ "op": "exists", "left": { "attr": "subject.userId" } });
    let url = insert_deny(
        &api,
        &["policy:manage", "role:manage", "user:manage"],
        everyone.clone(),
        false,
    )
    .await;
    let enabled = paul.patch(&url, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::BAD_REQUEST);
    assert!(enabled.error_message().contains("permissions enregistrées"));
    assert_eq!(paul.get("/admin/policies").await.status, StatusCode::OK);
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);

    // Enabled already (a permission the platform does not check yet): it
    // cannot be changed as it stands, only switched off or corrected.
    let url = insert_deny(&api, &["mission:read"], everyone, true).await;
    let renamed = paul.patch(&url, json!({ "description": "Renommée" })).await;
    assert_eq!(renamed.status, StatusCode::BAD_REQUEST);
    let off = paul.patch(&url, json!({ "enabled": false })).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
}
