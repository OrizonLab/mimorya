//! Requests nobody decided in time (ADR-061): the process that executes runs
//! expires them, audited as the platform's, and lets their run go on once
//! nothing else waits, with the rights of whom the agent acts for, or ends
//! it if that account is no longer active.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, PendingApproval, PendingCalls};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::AGENT_USE_WITHDRAWN;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approvals::{ApprovalRouting, NewApproval, create_approval};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::manager_approvals::{applying, listed};
use support::recovery::{events, saved, status_and_error, stopped_run};
use support::{Client, TestApi};
use uuid::Uuid;

fn id_of(approval: &Value) -> Uuid {
    approval["id"].as_str().unwrap().parse().unwrap()
}

/// A request of `run`, made for `user`, due at `expires_at`.
fn request(tenant_id: Uuid, run_id: Uuid, user_id: Uuid, expires_at: DateTime<Utc>) -> NewApproval {
    NewApproval {
        tenant_id,
        run_id,
        requested_for_user_id: user_id,
        tool_name: "planning_apply".into(),
        tool_call_id: format!("call-{}", Uuid::new_v4()),
        action_summary: "Appliquer un planning".into(),
        payload: json!({}),
        risk: "write".into(),
        reason: "Action modifiant des données".into(),
        expires_at: Some(expires_at),
        routing: ApprovalRouting::Requester,
    }
}

/// A run of `user` paused, as the engine leaves it, on a request already
/// past its deadline; returns both.
async fn paused_on_overdue(api: &TestApi, tenant_id: Uuid, user_id: Uuid) -> (Uuid, Uuid) {
    let run = stopped_run(api, tenant_id, user_id, None, None, &[]).await;
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let past = Utc::now() - Duration::minutes(1);
    let approval = create_approval(&mut tx, request(tenant_id, run, user_id, past))
        .await
        .unwrap();
    let mut checkpoint = saved();
    checkpoint.pending = Some(PendingCalls {
        completed_results: Vec::new(),
        approvals: vec![PendingApproval {
            approval_id: approval.id,
            tool_call_id: approval.tool_call_id.clone(),
            tool_name: approval.tool_name.clone(),
            input: json!({}),
        }],
    });
    sqlx::query("update agent_runs set status = 'awaiting_approval', state = $2 where id = $1")
        .bind(run)
        .bind(serde_json::to_value(checkpoint).unwrap())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (run, approval.id)
}

/// The request's status and its expiries audited, as `tenant_id` sees them.
async fn seen_from(api: &TestApi, tenant_id: Uuid, approval: Uuid) -> (Option<String>, i64) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let status: Option<String> = sqlx::query_scalar("select status from approvals where id = $1")
        .bind(approval)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    let audited: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where action = 'approval.expired' and resource_id = $1",
    )
    .bind(approval.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    (status, audited)
}

/// Moves the request's deadline a minute into the past.
async fn overdue(api: &TestApi, approval: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update approvals set expires_at = now() - interval '1 minute' where id = $1")
        .bind(approval)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The status of the request `id` in `client`'s approvals list.
async fn status_of(client: &Client<'_>, id: Uuid) -> Value {
    client.get("/approvals?status=all").await.json()["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == json!(id))
        .map(|a| a["status"].clone())
        .unwrap_or_default()
}

async fn run_status(client: &Client<'_>, run_id: &str) -> Value {
    client.get(&format!("/runs/{run_id}")).await.json()["run"].clone()
}

/// The platform's expiries audited for the run: reason and metadata.
async fn expiries(api: &TestApi, run_id: &str) -> Vec<(Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows: Vec<(Option<String>, Value)> = sqlx::query_as(
        "select reason, metadata from audit_log where run_id = $1::uuid \
         and action = 'approval.expired' and actor_type = 'system' \
         and actor_user_id is null and on_behalf_of_user_id = $2 \
         and outcome = 'failure' order by id",
    )
    .bind(run_id)
    .bind(user::NADIA)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_overdue_request_expires_and_its_run_goes_on_without_the_action() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(
        &api,
        &nadia,
        ("INT-2049", "09:00"),
        "Le planning n'a pas été appliqué.",
    )
    .await;
    let approval = approval.expect("a request she decides");
    let runs = &api.app.services.runs;

    // Not yet due: nothing expires.
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;

    let expired = listed(&nadia, &run_id).await.unwrap();
    assert_eq!(expired["status"], "expired");
    assert_eq!(expired["decidedBy"], Value::Null);
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "completed", "{run}");
    assert_eq!(
        expiries(&api, &run_id).await,
        [(
            Some("Délai de validation dépassé".to_string()),
            json!({ "toolName": "planning_apply", "risk": "write", "routing": "requester" })
        )]
    );
    // Once only.
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_process_that_executes_runs_expires_requests() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "09:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    overdue(&api, id_of(&approval)).await;

    // Without the run executor's lock, it leaves them to whoever holds it.
    let runs = &api.app.services.runs;
    assert_eq!(runs.expire_overdue().await.unwrap(), 0);
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "pending");
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "expired");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_waits_while_another_of_its_requests_may_still_be_decided() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2049", "10:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    // A second request of the run, due tomorrow.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let tomorrow = Utc::now() + Duration::days(1);
    let run: Uuid = run_id.parse().unwrap();
    let other = create_approval(&mut tx, request(tenant::ID, run, user::NADIA, tomorrow))
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(status_of(&nadia, id_of(&approval)).await, "expired");
    assert_eq!(status_of(&nadia, other.id).await, "pending");
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "awaiting_approval", "{run}");

    // The other one due in turn, the run goes on.
    overdue(&api, other.id).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    assert_eq!(status_of(&nadia, other.id).await, "expired");
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "completed", "{run}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_run_of_an_account_no_longer_active_ends_when_its_request_expires() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "10:00"), "Inchangé.").await;
    let approval = approval.expect("a request she decides");
    let account = format!("/admin/users/{}", user::NADIA);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    overdue(&api, id_of(&approval)).await;
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    let nadia = api.sign_in(user::NADIA).await;
    let run = run_status(&nadia, &run_id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert_eq!(run["error"], ACCOUNT_INACTIVE);
    assert_eq!(listed(&nadia, &run_id).await.unwrap()["status"], "expired");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_requests_expire_in_their_own_company() {
    let api = TestApi::start().await;
    let foreign = paused_on_overdue(&api, OTHER_TENANT_ID, OTHER_USER_ID).await;
    let own = paused_on_overdue(&api, tenant::ID, user::MARC).await;
    // Only Marc's run calls the model: the other contact may not use it.
    api.script(vec![ScriptedStep::text("Fait.")]);

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.expire_overdue().await.unwrap(), 2);
    api.idle().await;

    // Each expired and audited in its own company...
    for (tenant_id, (_, approval)) in [(OTHER_TENANT_ID, foreign), (tenant::ID, own)] {
        assert_eq!(
            seen_from(&api, tenant_id, approval).await,
            (Some("expired".to_string()), 1)
        );
    }
    // ...each run gone on there, with the rights of whom it acts for: Marc's
    // to its end, the other company's contact, who may not use the agent,
    // stopped.
    let (status, error) = status_and_error(&api, tenant::ID, own.0).await;
    assert_eq!((status.as_str(), error), ("completed", None));
    let (status, error) = status_and_error(&api, OTHER_TENANT_ID, foreign.0).await;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains(AGENT_USE_WITHDRAWN));
    // ...and nothing of the other company's shows in the demo company.
    assert_eq!(seen_from(&api, tenant::ID, foreign.1).await, (None, 0));
    assert!(events(&api, tenant::ID, foreign.0).await.is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_decision_meeting_the_expiry_finds_the_request_settled() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (run_id, approval) = applying(&api, &nadia, ("INT-2052", "11:00"), "Inchangé.").await;
    let approval = id_of(&approval.expect("a request she decides"));
    overdue(&api, approval).await;
    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();

    // The sweep queues first on the run's lock, the decision after it.
    let run: Uuid = run_id.parse().unwrap();
    let hold = hold_rows(&api, "agent_runs", &[run]).await;
    let decision = format!("/approvals/{approval}/decision");
    let (expired, decided, ()) = tokio::join!(
        runs.expire_overdue(),
        async {
            wait_for_waiters(&api, 1).await;
            nadia.post(&decision, json!({ "approved": true })).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(expired.unwrap(), 1);
    assert_eq!(decided.status, StatusCode::CONFLICT, "{}", decided.text());
    assert_eq!(decided.error_message(), "Cette demande a déjà été traitée");
    api.idle().await;

    // Expired once, by the platform.
    assert_eq!(
        seen_from(&api, tenant::ID, approval).await,
        (Some("expired".to_string()), 1)
    );
    assert_eq!(expiries(&api, &run_id).await.len(), 1);
}
