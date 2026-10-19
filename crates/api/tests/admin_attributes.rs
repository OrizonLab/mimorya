//! Collaborator attributes (ADR-023): set by a company-wide account
//! administrator, never on their own account; checked before storage; read
//! at once by access policies (`subject.attributes.*`) and audited with the
//! previous state.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::access::reads;
use support::accounts::{activate, create_partial_admin, directory_entry};
use uuid::Uuid;

fn url(user_id: Uuid) -> String {
    format!("/admin/users/{user_id}")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn attributes_change_who_a_policy_applies_to_at_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    // The demo policy: the administrative department only reads completed interventions.
    assert!(reads(&api, user::LUCAS, "planned").await);
    let moved = paul
        .patch(
            &url(user::LUCAS),
            json!({ "attributes": { "department": " administratif ", "sites": ["Lille"] } }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    assert!(!reads(&api, user::LUCAS, "planned").await);
    assert!(reads(&api, user::LUCAS, "completed").await);
    let entry = directory_entry(&paul, user::LUCAS).await;
    assert_eq!(
        entry["attributes"],
        json!({ "department": "administratif", "sites": ["Lille"] })
    );

    // Policy conditions can name the attribute.
    let listing = paul.get("/admin/policies").await.json();
    let paths: Vec<&str> = listing["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["path"].as_str().unwrap())
        .collect();
    assert!(paths.contains(&"subject.attributes.department"));
    assert!(paths.contains(&"subject.attributes.sites"));

    let cleared = paul
        .patch(&url(user::LUCAS), json!({ "attributes": {} }))
        .await;
    assert_eq!(cleared.status, StatusCode::OK);
    assert!(reads(&api, user::LUCAS, "planned").await);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<Value> = sqlx::query_scalar(
        "select metadata from audit_log where action = 'user.updated' and resource_id = $1 \
         order by id",
    )
    .bind(user::LUCAS.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let last = audited.last().unwrap();
    assert_eq!(last["attributes"], json!({}));
    assert_eq!(
        last["previousAttributes"],
        json!({ "department": "administratif", "sites": ["Lille"] })
    );
    // Other changes leave the attributes as they are.
    let renamed = paul
        .patch(
            &url(user::ISABELLE),
            json!({ "jobTitle": "Assistante de gestion" }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK);
    assert_eq!(
        directory_entry(&paul, user::ISABELLE).await["attributes"],
        json!({ "department": "administratif" })
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn attributes_follow_the_rules_and_the_administration_rights() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    for attributes in [
        json!({ "Department": "x" }),
        json!({ "site.name": "x" }),
        json!({ "department": "" }),
        json!({ "department": null }),
        json!({ "department": { "name": "x" } }),
        json!({ "sites": [1, 2] }),
        json!({ "department": "a\u{0}b" }),
        json!({ "department": "x".repeat(201) }),
        json!(["department"]),
    ] {
        let refused = paul
            .patch(&url(user::LUCAS), json!({ "attributes": attributes }))
            .await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{attributes}");
    }
    assert_eq!(
        directory_entry(&paul, user::LUCAS).await["attributes"],
        json!({})
    );

    // Never on one's own account: it could lift a policy aimed at oneself.
    let own = paul
        .patch(
            &url(user::PAUL),
            json!({ "attributes": { "department": "direction" } }),
        )
        .await;
    assert_eq!(own.status, StatusCode::FORBIDDEN);
    // Company-wide account administration only.
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.attributs").await;
    let partial = activate(&api, &mut partial).await;
    let narrowed = partial
        .patch(
            &url(user::LUCAS),
            json!({ "attributes": { "department": "nord" } }),
        )
        .await;
    assert_eq!(narrowed.status, StatusCode::FORBIDDEN);
    let marc = api.sign_in(user::MARC).await;
    assert!(
        marc.get("/admin/users").await.json()["users"]
            .as_array()
            .unwrap()
            .iter()
            .all(|u| u.get("attributes").is_none())
    );
    assert_eq!(
        paul.patch(&url(Uuid::new_v4()), json!({ "attributes": {} }))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}
