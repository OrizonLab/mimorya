//! Absences (ADR-071): declared and withdrawn as their author would, and the
//! right a stand-in needs, given by the administrator.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{role, unit, user};
use serde_json::json;
use uuid::Uuid;

use super::Client;

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
