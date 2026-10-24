//! Access policies (ADR-022): administrators write deny policies on business
//! permissions, which apply at once; allow policies, administration
//! permissions and malformed conditions are refused. Policies written outside
//! the API: `admin_policy_storage.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::access::reads;
use support::{Client, TestApi};
use uuid::Uuid;

/// The demo policy: administrative staff only reads completed interventions.
const DEMO_POLICY: Uuid = mimorya_core::demo::demo_id(400);

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
        json!({ "permissions": ["intervention:write"] }),
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

    // A new condition is checked against the stored permissions too.
    let for_agents = json!({ "op": "eq", "left": { "attr": "context.actorType" }, "right": { "value": "agent" } });
    let id = create(
        &paul,
        json!({ "description": "Pas d’application par l’agent", "permissions": ["planning:apply"], "condition": for_agents }),
    )
    .await;
    let url = format!("/admin/policies/{id}");
    let on_resource = paul
        .patch(&url, json!({ "condition": cancelled_only() }))
        .await;
    assert_eq!(on_resource.status, StatusCode::BAD_REQUEST);
    assert!(
        on_resource
            .error_message()
            .contains("ressource par ressource")
    );
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
async fn text_values_are_stored_as_attributes_are() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // Pasted decomposed (e + combining acute), stored composed like attributes.
    let id = create(
        &paul,
        json!({
            "description": "Pas de planning pour la Zélande",
            "permissions": ["planning:apply"],
            "condition": { "op": "eq", "left": { "attr": "subject.attributes.site" }, "right": { "value": "Ze\u{301}lande" } },
        }),
    )
    .await;
    let stored = paul.get("/admin/policies").await.json()["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(id))
        .unwrap()["condition"]["right"]["value"]
        .clone();
    assert_eq!(stored, json!("Z\u{e9}lande"));
    let hidden = paul
        .post(
            "/admin/policies",
            json!({
                "description": "Valeur piégée",
                "permissions": ["planning:apply"],
                "condition": { "op": "eq", "left": { "attr": "subject.attributes.site" }, "right": { "value": "Lil\u{200B}le" } },
            }),
        )
        .await;
    assert_eq!(hidden.status, StatusCode::BAD_REQUEST);
    // The tests of this file share a database.
    assert_eq!(
        paul.delete(&format!("/admin/policies/{id}")).await.status,
        StatusCode::OK
    );
}
