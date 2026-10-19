//! Access policies (ADR-022): administrators write deny policies on business
//! permissions, which apply at once; allow policies, administration
//! permissions and malformed conditions are refused; another company's
//! policies do not exist from here.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::{AccessContext, AuthIdentity};
use mimorya_core::authz::{Actor, ResourceRef};
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::{Client, TestApi};
use uuid::Uuid;

/// The demo policy: administrative staff only reads completed interventions.
const DEMO_POLICY: Uuid = mimorya_core::demo::demo_id(400);

async fn context_of(api: &TestApi, user_id: Uuid) -> AccessContext {
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    api.app.services.access.context(identity).await.unwrap()
}

/// Whether the user may read an intervention they own, in this status.
async fn reads(api: &TestApi, user_id: Uuid, status: &str) -> bool {
    let access = context_of(api, user_id).await;
    let intervention = ResourceRef::new("intervention", tenant::ID)
        .owner(Some(user_id))
        .attribute("status", status);
    access
        .engine
        .check(
            &access.subject,
            "intervention:read",
            &intervention,
            &Actor::user(),
        )
        .allowed
}

fn cancelled_only() -> Value {
    json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "cancelled" } })
}

async fn create(paul: &Client<'_>, body: Value) -> String {
    let created = paul.post("/admin/policies", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn deny_policies_apply_at_once_and_can_be_switched_off() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    assert!(reads(&api, user::LUCAS, "cancelled").await);

    let id = create(
        &paul,
        json!({
            "description": "Les interventions annulées ne se consultent plus",
            "permissions": ["intervention:read"],
            "condition": cancelled_only(),
        }),
    )
    .await;
    assert!(!reads(&api, user::LUCAS, "cancelled").await);
    assert!(reads(&api, user::LUCAS, "planned").await);

    let url = format!("/admin/policies/{id}");
    let off = paul.patch(&url, json!({ "enabled": false })).await;
    assert_eq!(off.status, StatusCode::OK);
    assert!(reads(&api, user::LUCAS, "cancelled").await);
    let widened = paul
        .patch(
            &url,
            json!({ "enabled": true, "permissions": ["intervention:*"] }),
        )
        .await;
    assert_eq!(widened.status, StatusCode::OK, "{}", widened.text());
    assert!(!reads(&api, user::LUCAS, "cancelled").await);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where action = 'policy.updated' and resource_id = $1 \
         and metadata->'permissions' <> 'null'::jsonb",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        metadata["previousPermissions"],
        json!(["intervention:read"])
    );
    assert_eq!(metadata["previousEnabled"], false);

    let renamed = paul
        .patch(
            &url,
            json!({ "description": "Annulées : plus de consultation", "condition": { "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "canceled" } } }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where action = 'policy.updated' and resource_id = $1 \
         and metadata->'condition' <> 'null'::jsonb",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(metadata["previousCondition"], cancelled_only());
    assert_eq!(
        metadata["previousDescription"],
        "Les interventions annulées ne se consultent plus"
    );

    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    assert!(reads(&api, user::LUCAS, "cancelled").await);
    assert_eq!(paul.delete(&url).await.status, StatusCode::NOT_FOUND);
    let deleted: Value = sqlx::query_scalar(
        "select metadata from audit_log where action = 'policy.deleted' and resource_id = $1",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(deleted["description"], "Annulées : plus de consultation");
    assert_eq!(deleted["permissions"], json!(["intervention:*"]));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_demo_policy_is_listed_and_editable() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let listing = paul.get("/admin/policies").await.json();
    let demo = listing["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(DEMO_POLICY))
        .unwrap()
        .clone();
    assert_eq!(demo["effect"], "deny");
    assert_eq!(demo["editable"], true);
    // What a policy may restrict: business permissions only.
    let restrictable: Vec<&str> = listing["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["permission"].as_str().unwrap())
        .collect();
    assert!(restrictable.contains(&"intervention:read"));
    assert!(!restrictable.contains(&"user:manage"));
    assert!(
        !listing["domains"]
            .as_array()
            .unwrap()
            .contains(&json!("memory"))
    );

    // Isabelle (administrative staff) reads interventions in progress once it is off.
    assert!(!reads(&api, user::ISABELLE, "planned").await);
    let url = format!("/admin/policies/{DEMO_POLICY}");
    assert_eq!(
        paul.patch(&url, json!({ "enabled": false })).await.status,
        StatusCode::OK
    );
    assert!(reads(&api, user::ISABELLE, "planned").await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refuses_policies_outside_the_rules() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let valid = json!({
        "description": "Politique de test",
        "permissions": ["intervention:read"],
        "condition": cancelled_only(),
    });
    let with = |key: &str, value: Value| {
        let mut body = valid.clone();
        body[key] = value;
        body
    };
    let refused = [
        with("effect", json!("allow")),
        with("permissions", json!(["user:manage"])),
        with("permissions", json!(["memory:*"])),
        with("permissions", json!(["*"])),
        with("permissions", json!(["facture:read"])),
        with("permissions", json!([])),
        with("description", json!("x")),
        with("condition", json!({ "all": [] })),
        with(
            "condition",
            json!({ "op": "eq", "left": { "attr": "user.department" }, "right": { "value": "x" } }),
        ),
        with(
            "condition",
            json!({ "op": "eq", "left": { "attr": "resource.type" } }),
        ),
        with(
            "condition",
            json!({ "op": "matches", "left": { "attr": "resource.type" }, "right": { "value": "x" } }),
        ),
        with("condition", json!("resource.type == 'intervention'")),
        with("description", json!("Caractère nul \u{0}")),
    ];
    for body in refused {
        let response = paul.post("/admin/policies", body.clone()).await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            response.text()
        );
    }

    // The same rules hold when an existing policy is changed.
    let id = create(&paul, valid.clone()).await;
    let url = format!("/admin/policies/{id}");
    let changes = [
        json!({ "permissions": ["user:manage"] }),
        json!({ "permissions": ["memory:*"] }),
        json!({ "permissions": ["*"] }),
        json!({ "permissions": [] }),
        json!({ "permissions": ["org:read"] }),
        json!({ "permissions": ["mission:read"] }),
        json!({ "permissions": ["planning:apply"] }),
        json!({ "condition": { "all": [] } }),
        json!({ "condition": { "op": "eq", "left": { "attr": "user.x" }, "right": { "value": "y" } } }),
        json!({ "effect": "allow" }),
        json!({ "description": "Avec un caractère nul \u{0}" }),
    ];
    for change in changes {
        let response = paul.patch(&url, change.clone()).await;
        assert_eq!(
            response.status,
            StatusCode::BAD_REQUEST,
            "{change}: {}",
            response.text()
        );
    }
    let stored = paul.get("/admin/policies").await.json()["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(id))
        .unwrap()
        .clone();
    assert_eq!(stored["permissions"], json!(["intervention:read"]));
    assert_eq!(stored["condition"], cancelled_only());
    assert_eq!(stored["description"], "Politique de test");
    // The tests of this file share a database.
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    assert_eq!(
        marc.get("/admin/policies").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        marc.post("/admin/policies", valid).await.status,
        StatusCode::FORBIDDEN
    );
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
    // Written by hand, disabled: the engine never loads it.
    let id = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition, enabled) values ($1, $2, 'Écrite à la main', 'deny', '{intervention:read}', \
         $3, false)",
    )
    .bind(id)
    .bind(tenant::ID)
    .bind(json!({ "all": [] }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let url = format!("/admin/policies/{id}");
    let enabled = paul.patch(&url, json!({ "enabled": true })).await;
    assert_eq!(
        enabled.status,
        StatusCode::BAD_REQUEST,
        "{}",
        enabled.text()
    );
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
