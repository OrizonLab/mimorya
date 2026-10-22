//! Deciding for an absent manager (ADR-071): while the absence runs, whom
//! the manager named lists and decides the pending requests routed to them,
//! but only where they hold `approval:decide` over whom each is for; the
//! audit names whom they stood in for. Before the absence, after it is
//! withdrawn, for a disabled account, through someone else's delegate (one
//! hop), or without the right, the delegate finds nothing; the manager still decides, and the
//! requester, though named, may only withdraw. What the manager decided
//! stays theirs to see.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{role, unit, user};
use serde_json::{Value, json};
use support::manager_approvals::{applying, audited, route_to_managers};
use support::{Client, TestApi};

const ABSENCE: &str = "/approval-delegation";

/// Has `absent` name `delegate` from `starts_in` days from now, for a week.
async fn away(absent: &Client<'_>, delegate: uuid::Uuid, starts_in: i64) {
    let starts = Utc::now() + Duration::days(starts_in);
    let body = json!({
        "delegateId": delegate,
        "startsAt": starts.to_rfc3339(),
        "endsAt": (starts + Duration::days(7)).to_rfc3339(),
    });
    let declared = absent.put(ABSENCE, body).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
}

async fn back(absent: &Client<'_>) {
    let withdrawn = absent.delete(ABSENCE).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
}

/// Gives Julien the team lead's role over the south team: he then holds
/// `approval:decide` over Nadia. The assignment, to revoke.
async fn julien_decides_for_the_south(paul: &Client<'_>) -> String {
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

/// A request of Nadia's, awaiting Marc: its identifier.
async fn nadia_s_request(api: &TestApi, paul: &Client<'_>, start_time: &str) -> String {
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(paul, &["write"]).await;
    let (_, approval) = applying(api, &nadia, ("INT-2049", start_time), "Inchangé.").await;
    route_to_managers(paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    assert_eq!(approval["approver"]["id"], json!(user::MARC));
    approval["id"].as_str().unwrap().to_owned()
}

/// The request as `client` finds it among theirs, if at all.
async fn found(client: &Client<'_>, id: &str) -> Option<Value> {
    let listed = client.get("/approvals?status=all").await.json();
    listed["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .cloned()
}

async fn decide(client: &Client<'_>, id: &str, approved: bool) -> StatusCode {
    let decision = format!("/approvals/{id}/decision");
    client
        .post(&decision, json!({ "approved": approved }))
        .await
        .status
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_delegate_decides_what_they_hold_the_right_for() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let id = nadia_s_request(&api, &paul, "09:00").await;
    away(&marc, user::JULIEN, 0).await;

    // Without `approval:decide` over Nadia, Julien finds nothing.
    assert!(found(&julien, &id).await.is_none());
    assert_eq!(decide(&julien, &id, true).await, StatusCode::NOT_FOUND);
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);
    // Not even whether Nadia's account is still active.
    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);
    assert_eq!(decide(&marc, &id, false).await, StatusCode::CONFLICT);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    let assignment = julien_decides_for_the_south(&paul).await;
    let shown = found(&julien, &id).await.expect("shown to the delegate");
    assert_eq!(shown["canDecide"], true);
    assert_eq!(shown["approver"]["id"], json!(user::MARC));
    assert_eq!(decide(&julien, &id, false).await, StatusCode::OK);
    api.idle().await;

    // Decided by him, for Nadia, standing in for Marc.
    let run_id = shown["runId"].as_str().unwrap();
    assert_eq!(
        audited(&api, run_id, "approval.rejected").await,
        [(Some(user::JULIEN), Some(user::NADIA))]
    );
    let decided = found(&julien, &id).await.expect("what he decided");
    assert_eq!(decided["decidedBy"]["id"], json!(user::JULIEN));
    assert_eq!(decided["canDecide"], false);
    let metadata = decision_metadata(&api, &id).await;
    assert_eq!(metadata["standingInFor"], json!(user::MARC));

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

async fn decision_metadata(api: &TestApi, approval_id: &str) -> Value {
    let mut tx = api
        .app
        .services
        .db
        .begin(mimorya_core::demo::tenant::ID)
        .await
        .unwrap();
    let metadata: Value = sqlx::query_scalar(
        "select metadata from audit_log where resource_id = $1 \
         and action in ('approval.approved', 'approval.rejected')",
    )
    .bind(approval_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    metadata
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_delegate_decides_only_while_the_absence_runs_and_only_for_whom_named_them() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;
    let id = nadia_s_request(&api, &paul, "11:00").await;

    // Not before the absence starts.
    away(&marc, user::JULIEN, 1).await;
    assert!(found(&julien, &id).await.is_none());
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);

    // Not through someone else's delegate: Marc names Paul, who names him.
    away(&marc, user::PAUL, 0).await;
    away(&paul, user::JULIEN, 0).await;
    assert!(found(&julien, &id).await.is_none());
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);
    back(&paul).await;

    // Named, then back: nothing left to him.
    away(&marc, user::JULIEN, 0).await;
    assert_eq!(found(&julien, &id).await.unwrap()["canDecide"], true);
    back(&marc).await;
    assert!(found(&julien, &id).await.is_none());
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);

    // Not for a disabled account.
    away(&marc, user::JULIEN, 0).await;
    let account = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert!(found(&julien, &id).await.is_none());
    assert_eq!(decide(&julien, &id, false).await, StatusCode::NOT_FOUND);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    // Away or not, Marc decides.
    let marc = api.sign_in(user::MARC).await;
    assert_eq!(decide(&marc, &id, false).await, StatusCode::OK);
    api.idle().await;
    let metadata = decision_metadata(&api, &id).await;
    assert!(metadata.get("standingInFor").is_none(), "{metadata}");

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_requester_named_may_only_withdraw_and_the_manager_s_decisions_stay_hidden() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let nadia = api.sign_in(user::NADIA).await;
    let julien = api.sign_in(user::JULIEN).await;
    let assignment = julien_decides_for_the_south(&paul).await;

    // Nadia, named by Marc, may not approve her own request.
    let own = nadia_s_request(&api, &paul, "13:00").await;
    away(&marc, user::NADIA, 0).await;
    let shown = found(&nadia, &own).await.unwrap();
    assert_eq!(shown["canDecide"], false);
    assert_eq!(shown["canWithdraw"], true);
    assert_eq!(decide(&nadia, &own, true).await, StatusCode::FORBIDDEN);
    // Decided by Marc before his absence: never shown to his delegate.
    assert_eq!(decide(&marc, &own, false).await, StatusCode::OK);
    api.idle().await;
    away(&marc, user::JULIEN, 0).await;
    assert!(found(&julien, &own).await.is_none());

    // What waits now is his to decide.
    let pending = nadia_s_request(&api, &paul, "15:00").await;
    assert_eq!(found(&julien, &pending).await.unwrap()["canDecide"], true);
    assert_eq!(decide(&nadia, &pending, false).await, StatusCode::OK);
    api.idle().await;

    back(&marc).await;
    let revoked = paul.delete(&assignment).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
}
