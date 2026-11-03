//! What the double validation of allow policies refuses (ADR-168): a
//! policy outside the rules of a grant, a deny or an unknown policy named,
//! anyone but an administrator of the access policies, another company's
//! proposal, a decision by the wrong administrator, an expired proposal or
//! one whose author lost the right, one more than a company may have waiting.
//! Decided proposals are purged later.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::user;
use mimorya_db::repositories::policy_proposals::{MAX_PENDING_PROPOSALS, purge_policy_proposals};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::TestApi;
use support::accounts::directory_entry;
use support::policy_proposals::{
    approve, audited, completed_interventions, completed_only, listed, propose, reads_south,
    second_admin,
};
use support::refusals::{latest_entry, unknown_references_since};
use uuid::Uuid;

/// The demo deny policy: administrative staff only reads completed interventions.
const DEMO_POLICY: Uuid = mimorya_core::demo::demo_id(400);

fn with(field: &str, value: Value) -> Value {
    let mut body = completed_interventions();
    body[field] = value;
    body
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn proposals_outside_the_rules_of_a_grant_are_refused() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    for body in [
        // Checked on the person: a grant would never apply.
        with("permissions", json!(["planning:apply"])),
        // Also covers intervention:write, never checked yet.
        with("permissions", json!(["intervention:*"])),
        with("permissions", json!(["user:manage"])),
        with("permissions", json!([])),
        with("condition", json!({ "all": [] })),
        with("policyId", json!(DEMO_POLICY)),
    ] {
        let refused = paul.post("/admin/policy-proposals", body.clone()).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{body}: {}",
            refused.text()
        );
    }
    // An unknown policy is answered as invalid, and traced.
    let since = latest_entry(&api).await;
    let before = proposal_count(&paul).await;
    let unknown = Uuid::new_v4();
    let refused = paul
        .post("/admin/policy-proposals", with("policyId", json!(unknown)))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    let traced = unknown_references_since(&api, since).await;
    assert_eq!(traced.len(), 1);
    assert_eq!(traced[0].1.metadata["reference"]["id"], json!(unknown));

    assert_eq!(proposal_count(&paul).await, before);
}

async fn proposal_count(admin: &support::Client<'_>) -> usize {
    let listing = admin.get("/admin/policy-proposals").await.json();
    listing["proposals"].as_array().unwrap().len()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_administrators_of_the_policies_of_the_company_see_or_decide_proposals() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = propose(&paul, completed_interventions()).await;
    assert_eq!(
        lucas.get("/admin/policy-proposals").await.status,
        StatusCode::FORBIDDEN
    );
    let proposed = lucas
        .post("/admin/policy-proposals", completed_interventions())
        .await;
    assert_eq!(proposed.status, StatusCode::FORBIDDEN);
    assert_eq!(approve(&lucas, &id).await.status, StatusCode::FORBIDDEN);

    // Another company's proposal is unknown here.
    let foreign = Uuid::new_v4();
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query(
        "insert into policy_proposals (id, tenant_id, description, permissions, condition, \
         enabled, proposed_by_user_id, expires_at) \
         values ($1, $2, 'Ailleurs', '{intervention:read}', $3, true, $4, now() + interval '1 day')",
    )
    .bind(foreign)
    .bind(OTHER_TENANT_ID)
    .bind(completed_only())
    .bind(OTHER_USER_ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for decision in ["approval", "rejection", "withdrawal"] {
        let url = format!("/admin/policy-proposals/{foreign}/{decision}");
        assert_eq!(
            paul.post(&url, json!({})).await.status,
            StatusCode::NOT_FOUND,
            "{decision}"
        );
    }
    assert_eq!(listed(&paul, &foreign.to_string()).await, json!(null));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_administrator_rejects_and_only_the_author_withdraws() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.refus").await;
    let decide = |id: &str, decision: &str| format!("/admin/policy-proposals/{id}/{decision}");

    let rejected = propose(&paul, completed_interventions()).await;
    let own = paul.post(&decide(&rejected, "rejection"), json!({})).await;
    assert_eq!(own.status, StatusCode::FORBIDDEN, "{}", own.text());
    let not_hers = claire
        .post(&decide(&rejected, "withdrawal"), json!({}))
        .await;
    assert_eq!(not_hers.status, StatusCode::FORBIDDEN);
    let refused = claire
        .post(&decide(&rejected, "rejection"), json!({}))
        .await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    let decided = listed(&paul, &rejected).await;
    assert_eq!(decided["status"], "rejected");
    assert_eq!(decided["decidedBy"]["id"], json!(claire_id));
    assert_eq!(
        approve(&claire, &rejected).await.status,
        StatusCode::CONFLICT
    );

    let withdrawn = propose(&paul, completed_interventions()).await;
    let taken_back = paul
        .post(&decide(&withdrawn, "withdrawal"), json!({}))
        .await;
    assert_eq!(taken_back.status, StatusCode::OK, "{}", taken_back.text());
    assert_eq!(listed(&claire, &withdrawn).await["status"], "withdrawn");
    assert_eq!(
        approve(&claire, &withdrawn).await.status,
        StatusCode::CONFLICT
    );
    assert!(!reads_south(&api, user::LUCAS, "completed").await);

    let entries = audited(&api, "policy_proposal.rejected", &rejected).await;
    assert_eq!(entries[0].0, claire_id);
    let entries = audited(&api, "policy_proposal.withdrawn", &withdrawn).await;
    assert_eq!(entries[0].0, user::PAUL);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_expired_proposal_or_one_whose_author_lost_the_right_is_never_approved() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, claire_id) = second_admin(&api, &paul, "claire.expiration").await;

    let expired = propose(&paul, completed_interventions()).await;
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    sqlx::query(
        "update policy_proposals set proposed_at = now() - interval '8 days', \
         expires_at = now() - interval '1 day' where id = $1",
    )
    .bind(expired.parse::<Uuid>().unwrap())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(listed(&claire, &expired).await["status"], "expired");
    let late = approve(&claire, &expired).await;
    assert_eq!(late.status, StatusCode::CONFLICT, "{}", late.text());

    // Claire proposes, then loses the right before Paul approves.
    let orphan = propose(&claire, completed_interventions()).await;
    let entry = directory_entry(&paul, claire_id).await;
    for assignment in entry["assignments"].as_array().unwrap() {
        let url = format!("/admin/assignments/{}", assignment["id"].as_str().unwrap());
        let revoked = paul.delete(&url).await;
        assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
    }
    let unbacked = approve(&paul, &orphan).await;
    assert_eq!(unbacked.status, StatusCode::CONFLICT, "{}", unbacked.text());
    assert!(!reads_south(&api, user::LUCAS, "completed").await);

    // Settled or expired by then, a proposal is purged; one still pending
    // is kept.
    let purged = purge_policy_proposals(api.app.services.db.pool(), Utc::now() + Duration::days(1))
        .await
        .unwrap();
    assert!(purged >= 1, "{purged}");
    assert_eq!(listed(&paul, &expired).await, json!(null));
    assert_eq!(listed(&paul, &orphan).await["status"], "pending");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_has_at_most_so_many_proposals_waiting() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let listing = paul.get("/admin/policy-proposals").await.json();
    let waiting = listing["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["status"] == "pending")
        .count();
    let limit = usize::try_from(MAX_PENDING_PROPOSALS).unwrap();
    let mine: Vec<String> = {
        let mut made = Vec::new();
        for _ in waiting..limit {
            made.push(propose(&paul, completed_interventions()).await);
        }
        made
    };
    let one_more = paul
        .post("/admin/policy-proposals", completed_interventions())
        .await;
    assert_eq!(one_more.status, StatusCode::CONFLICT, "{}", one_more.text());
    // All listed, waiting first.
    let listing = paul.get("/admin/policy-proposals").await.json();
    let first: Vec<&Value> = listing["proposals"].as_array().unwrap()[..limit]
        .iter()
        .collect();
    assert!(first.iter().all(|p| p["status"] == "pending"));
    for id in &mine {
        let url = format!("/admin/policy-proposals/{id}/withdrawal");
        assert_eq!(paul.post(&url, json!({})).await.status, StatusCode::OK);
    }
    // Withdrawn, they leave room again.
    let freed = propose(&paul, completed_interventions()).await;
    let url = format!("/admin/policy-proposals/{freed}/withdrawal");
    assert_eq!(paul.post(&url, json!({})).await.status, StatusCode::OK);
}
