//! Attempts around the double validation of allow policies (ADR-168): a
//! proposal stored outside the rules a grant follows today is never
//! approved; another company's allow policy is unknown to a proposal; an
//! administrator of the policies on one unit only neither sees, proposes
//! nor decides.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::{activate, create_partial_admin};
use support::policy_proposals::{
    approve, completed_interventions, completed_only, listed, propose, reads_south, second_admin,
};
use support::refusals::{latest_entry, unknown_references_since};
use uuid::Uuid;

/// A pending proposal written without the API's checks, by `author`.
async fn insert_proposal(
    api: &TestApi,
    tenant_id: Uuid,
    author: Uuid,
    permissions: &[&str],
    condition: Value,
) -> String {
    let id = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query(
        "insert into policy_proposals (id, tenant_id, description, permissions, condition, \
         enabled, proposed_by_user_id, expires_at) \
         values ($1, $2, 'Écrite à la main', $3, $4, true, $5, now() + interval '1 day')",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(permissions)
    .bind(condition)
    .bind(author)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id.to_string()
}

fn allow_policies(listing: &Value) -> usize {
    listing["policies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["effect"] == "allow")
        .count()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_proposal_the_rules_no_longer_accept_is_never_approved() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, claire_id) = second_admin(&api, &paul, "claire.regles").await;
    let before = allow_policies(&paul.get("/admin/policies").await.json());
    for (permissions, condition) in [
        // Checked on the person: a grant would never apply to it.
        (&["planning:apply"][..], completed_only()),
        // An administration permission.
        (&["user:manage"][..], completed_only()),
        // A condition the engine refuses to read.
        (&["intervention:read"][..], json!({ "all": [] })),
        (&["intervention:read"][..], json!({ "op": "eq" })),
    ] {
        let id = insert_proposal(&api, tenant::ID, claire_id, permissions, condition).await;
        let refused = approve(&paul, &id).await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
        assert_eq!(listed(&paul, &id).await["status"], "pending");
    }
    let after = paul.get("/admin/policies").await.json();
    assert_eq!(allow_policies(&after), before, "{after}");
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_allow_policy_is_unknown_to_a_proposal() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let foreign = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into access_policies (id, tenant_id, description, effect, permissions, \
         condition) values ($1, $2, 'Ailleurs', 'allow', '{intervention:read}', $3)",
    )
    .bind(foreign)
    .bind(OTHER_TENANT_ID)
    .bind(completed_only())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let since = latest_entry(&api).await;
    let mut body = completed_interventions();
    body["policyId"] = json!(foreign);
    let refused = paul.post("/admin/policy-proposals", body).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    let traced = unknown_references_since(&api, since).await;
    assert_eq!(traced.len(), 1);
    assert_eq!(traced[0].1.metadata["reference"]["id"], json!(foreign));
    // The other company's own proposals are as unknown (its author's).
    let theirs = insert_proposal(
        &api,
        OTHER_TENANT_ID,
        OTHER_USER_ID,
        &["intervention:read"],
        completed_only(),
    )
    .await;
    assert_eq!(approve(&paul, &theirs).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_of_one_unit_neither_sees_proposes_nor_decides() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let mut account = create_partial_admin(&api, &paul, "admin.equipe.nord").await;
    let partial = activate(&api, &mut account).await;
    let id = propose(&paul, completed_interventions()).await;

    assert_eq!(
        partial.get("/admin/policy-proposals").await.status,
        StatusCode::FORBIDDEN
    );
    let proposed = partial
        .post("/admin/policy-proposals", completed_interventions())
        .await;
    assert_eq!(proposed.status, StatusCode::FORBIDDEN);
    for decision in ["approval", "rejection"] {
        let url = format!("/admin/policy-proposals/{id}/{decision}");
        assert_eq!(
            partial.post(&url, json!({})).await.status,
            StatusCode::FORBIDDEN,
            "{decision}"
        );
    }
    assert_eq!(listed(&paul, &id).await["status"], "pending");
    assert!(!reads_south(&api, user::LUCAS, "completed").await);
    let withdrawn = paul
        .post(
            &format!("/admin/policy-proposals/{id}/withdrawal"),
            json!({}),
        )
        .await;
    assert_eq!(withdrawn.status, StatusCode::OK);
}
