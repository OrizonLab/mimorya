//! Allow policies proposed and decided in the tests (ADR-168): a second
//! administrator of the access policies, made by Paul; a proposal; the
//! audit of each step; and what a technician, who reads only their own
//! interventions, reads of the south team's.

use axum::http::StatusCode;
use mimorya_core::authz::{Actor, ResourceRef};
use mimorya_core::demo::{tenant, unit, user};
use serde_json::{Value, json};
use uuid::Uuid;

use super::access::context_of;
use super::accounts::holder_of;
use super::{Client, TestApi};

/// An account holding only `policy:manage`, company-wide; returns its
/// session and its id.
pub async fn second_admin<'a>(
    api: &'a TestApi,
    paul: &Client<'_>,
    login: &str,
) -> (Client<'a>, Uuid) {
    let admin = holder_of(api, paul, login, &["policy:manage"]).await;
    let me = admin.get("/auth/me").await.json();
    let id = me["user"]["id"].as_str().unwrap().parse().unwrap();
    (admin, id)
}

/// Completed interventions only.
pub fn completed_only() -> Value {
    json!({ "op": "eq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "completed" } })
}

/// A new allow policy reading the south team's completed interventions.
pub fn completed_interventions() -> Value {
    json!({
        "description": "Les interventions terminées se consultent dans toute l’entreprise",
        "permissions": ["intervention:read"],
        "condition": completed_only(),
    })
}

/// A pending proposal written without the API's checks, by `author`, in
/// `tenant_id`; returns its id.
pub async fn insert_proposal(
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

/// Proposes `body` as `admin`; returns the proposal's id.
pub async fn propose(admin: &Client<'_>, body: Value) -> String {
    let proposed = admin.post("/admin/policy-proposals", body).await;
    assert_eq!(proposed.status, StatusCode::CREATED, "{}", proposed.text());
    proposed.json()["id"].as_str().unwrap().to_string()
}

/// Approves the proposal `id` as `admin`.
pub async fn approve(admin: &Client<'_>, id: &str) -> super::TestResponse {
    admin
        .post(&format!("/admin/policy-proposals/{id}/approval"), json!({}))
        .await
}

/// The proposal `id` as `admin` lists it.
pub async fn listed(admin: &Client<'_>, id: &str) -> Value {
    let listing = admin.get("/admin/policy-proposals").await;
    assert_eq!(listing.status, StatusCode::OK, "{}", listing.text());
    listing.json()["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == json!(id))
        .cloned()
        .unwrap_or(Value::Null)
}

/// The id of the policy an approved proposal wrote, from its audit entry.
pub async fn approved_policy(api: &TestApi, proposal: &str) -> String {
    let (_, metadata) = audited(api, "policy_proposal.approved", proposal)
        .await
        .pop()
        .expect("approval audited");
    metadata["policyId"].as_str().unwrap().to_string()
}

/// Who wrote each audit entry `action` on `resource_id`, and what it says.
pub async fn audited(api: &TestApi, action: &str, resource_id: &str) -> Vec<(Uuid, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(Option<Uuid>, Value)> = sqlx::query_as(
        "select actor_user_id, metadata from audit_log where action = $1 and resource_id = $2 \
         order by id",
    )
    .bind(action)
    .bind(resource_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(actor, metadata)| (actor.expect("an administrator's entry"), metadata))
        .collect()
}

/// Whether `user_id` reads an intervention of the south team, Hugo's, in
/// this status.
pub async fn reads_south(api: &TestApi, user_id: Uuid, status: &str) -> bool {
    let access = context_of(api, user_id).await;
    let intervention = ResourceRef::new("intervention", tenant::ID)
        .owner(Some(user::HUGO))
        .org_unit(Some(unit::TEAM_SOUTH))
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
