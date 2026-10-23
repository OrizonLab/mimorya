//! Absences (ADR-071): declared and withdrawn as their author would, or by
//! an account administrator (ADR-078), and the right a stand-in needs, given
//! by the administrator.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{role, tenant, unit, user};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi};

pub const ABSENCE: &str = "/approval-delegation";

/// Has `absent` name `delegate` from `starts_in` days from now, for a week.
pub async fn away(absent: &Client<'_>, delegate: Uuid, starts_in: i64) {
    let starts = Utc::now() + Duration::days(starts_in);
    let body = json!({
        "delegateId": delegate,
        "startsAt": starts.to_rfc3339(),
        "endsAt": (starts + Duration::days(7)).to_rfc3339(),
    });
    let declared = absent.put(ABSENCE, body).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
}

pub async fn back(absent: &Client<'_>) {
    let withdrawn = absent.delete(ABSENCE).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
}

/// Gives Julien the team lead's role over the south team: he then holds
/// `approval:decide` over Nadia. The assignment, to revoke.
pub async fn julien_decides_for_the_south(paul: &Client<'_>) -> String {
    let scope =
        json!({ "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true });
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", user::JULIEN),
            json!({ "roleId": role::TEAM_LEAD, "scope": scope }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    format!(
        "/admin/assignments/{}",
        granted.json()["id"].as_str().unwrap()
    )
}

/// Where an administrator reads and declares the absence of `user_id`.
pub fn absence_of(user_id: Uuid) -> String {
    format!("/admin/users/{user_id}/absence")
}

/// A week from now, standing in: `delegate`.
pub fn a_week(delegate: Uuid) -> Value {
    json!({ "delegateId": delegate, "endsAt": (Utc::now() + Duration::days(7)).to_rfc3339() })
}

/// Has `admin` declare the absence of `absent` for a week; their answer.
pub async fn declare_for(admin: &Client<'_>, absent: Uuid, delegate: Uuid) -> Value {
    let declared = admin.put(&absence_of(absent), a_week(delegate)).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    declared.json()
}

/// Absence changes audited on `user_id`: who made them, and the delegate.
pub async fn absence_changes(
    api: &TestApi,
    user_id: Uuid,
) -> Vec<(String, Option<Uuid>, Option<Value>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(String, Option<Uuid>, Value)> = sqlx::query_as(
        "select action, actor_user_id, metadata from audit_log \
         where action like 'approval.delegation%' and resource_id = $1::text order by id",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows.into_iter()
        .map(|(action, actor, metadata)| (action, actor, metadata.get("delegateId").cloned()))
        .collect()
}
