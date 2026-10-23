//! A scheduled task's run paused on a request its author decides (ADR-082),
//! and what its notices were audited as.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_db::repositories::approvals::{ApprovalRouting, NewApproval, create_approval};
use mimorya_db::repositories::scheduled_tasks::{TaskDefinition, create_scheduled_task};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use uuid::Uuid;

use super::planning::{apply_proposal, propose};
use super::recovery::{saved, stopped_run};
use super::scheduled::{due, fire, listed};
use super::{Client, TestApi};

pub const TITLE: &str = "Point du lundi";

/// The notices audited in `tenant_id` for `approval`: for whom, outcome and
/// metadata.
pub async fn notices(api: &TestApi, tenant_id: Uuid, approval: &str) -> Vec<(Uuid, String, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where action = 'approval.notified' and resource_id = $1 \
         and actor_type = 'system' and actor_user_id is null order by id",
    )
    .bind(approval)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// Has `api` fire Nadia's task `id`, its run asking to apply a planning,
/// and returns the conversation it ran in and the request it awaits.
pub async fn paused(api: &TestApi, nadia: &Client<'_>, id: Uuid) -> (String, Value) {
    api.script(vec![
        propose("INT-2049", "09:00"),
        apply_proposal(""),
        ScriptedStep::text("Inchangé."),
    ]);
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(api).await, 1);
    let conversation = listed(nadia, id).await["lastConversationId"]
        .as_str()
        .unwrap()
        .to_string();
    let approvals = nadia.get("/approvals?status=pending").await.json();
    let approval = approvals["approvals"][0].clone();
    assert!(approval.is_object(), "{approvals}");
    (conversation, approval)
}

/// Refuses `approval` as `decider`, ending its run.
pub async fn refuse(api: &TestApi, decider: &Client<'_>, approval: &Value) {
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let refused = decider.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    api.idle().await;
}

/// A scheduled task of the other company, its last run paused on a request
/// its author decides, never notified: the task, the run and the request.
pub async fn foreign_scheduled_request(api: &TestApi) -> (Uuid, Uuid, Uuid) {
    let run = stopped_run(
        api,
        OTHER_TENANT_ID,
        OTHER_USER_ID,
        None,
        Some(saved()),
        &[],
    )
    .await;
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'awaiting_approval' where id = $1")
        .bind(run)
        .execute(&mut *tx)
        .await
        .unwrap();
    let request = create_approval(
        &mut tx,
        NewApproval {
            tenant_id: OTHER_TENANT_ID,
            run_id: run,
            requested_for_user_id: OTHER_USER_ID,
            tool_name: "planning_apply".into(),
            tool_call_id: "call-foreign-task".into(),
            action_summary: "Appliquer un planning".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: "Action modifiant des données".into(),
            expires_at: Some(Utc::now() + Duration::days(7)),
            routing: ApprovalRouting::Requester,
        },
    )
    .await
    .unwrap();
    let definition = TaskDefinition {
        title: "Relevé de l’autre entreprise",
        instruction: "Relève les retards.",
        weekdays: &[1, 2, 3, 4, 5],
        time_of_day: "08:00",
        time_zone: "Europe/Paris",
        next_run_at: None,
    };
    let task = create_scheduled_task(&mut tx, OTHER_TENANT_ID, OTHER_USER_ID, definition)
        .await
        .unwrap();
    // No conversation: the e-mail leads to « Validations ».
    sqlx::query("update scheduled_tasks set last_run_id = $2 where id = $1")
        .bind(task)
        .bind(run)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (task, run, request.id)
}

/// Ends the run of `foreign_scheduled_request`, its request cancelled and
/// its task deleted.
pub async fn end_foreign_scheduled_request(
    api: &TestApi,
    (task, run, request): (Uuid, Uuid, Uuid),
) {
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    for (sql, id) in [
        ("update agent_runs set status = 'failed' where id = $1", run),
        (
            "update approvals set status = 'cancelled' where id = $1",
            request,
        ),
        ("delete from scheduled_tasks where id = $1", task),
    ] {
        sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
}
