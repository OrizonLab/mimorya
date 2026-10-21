//! Reminding managers the day before requests expire (ADR-062): once per
//! request, in one message per manager and run, by the process that
//! executes runs, only with a mail relay; never a request its requester
//! decides, nor one decided or past its deadline; each company's in its
//! own company.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approvals::{ApprovalRouting, NewApproval, create_approval};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::manager_approvals::{applying, route_to_managers};
use support::recovery::{saved, stopped_run};
use support::{TestApi, email};
use uuid::Uuid;

fn id_of(approval: &Value) -> Uuid {
    approval["id"].as_str().unwrap().parse().unwrap()
}

/// Moves the request's deadline to `minutes` from now.
async fn due_in(api: &TestApi, tenant_id: Uuid, approval: Uuid, minutes: i64) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query("update approvals set expires_at = $2 where id = $1")
        .bind(approval)
        .bind(Utc::now() + Duration::minutes(minutes))
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

/// The reminders audited for the request, as `tenant_id` sees them: the
/// platform's, for whom the agent acts, with outcome and metadata.
async fn reminders(api: &TestApi, tenant_id: Uuid, approval: Uuid) -> Vec<(Uuid, String, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let rows: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where action = 'approval.reminded' and resource_id = $1 \
         and actor_type = 'system' and actor_user_id is null order by id",
    )
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_manager_is_reminded_once_the_day_before() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = applying(&api, &nadia, ("INT-2049", "09:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let approval = approval.expect("a request awaiting her manager");
    let id = id_of(&approval);
    let runs = &api.app.services.runs;
    assert_eq!(api.mail.sent().len(), 1, "the notice");

    // Due in two hours: only the process that executes runs reminds...
    due_in(&api, tenant::ID, id, 120).await;
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    runs.claim_execution().await.unwrap();
    // ...not a week ahead, but the day before, once.
    due_in(&api, tenant::ID, id, 7 * 24 * 60).await;
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    due_in(&api, tenant::ID, id, 120).await;
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    assert_eq!(runs.remind_due().await.unwrap(), 0);

    let sent = api.mail.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    let reminder = &sent[1];
    assert_eq!(reminder.to, email(user::MARC));
    assert_eq!(
        reminder.subject,
        "Rappel : Une action de Nadia Benali attend votre validation"
    );
    assert!(reminder.text.contains("attend toujours votre validation"));
    assert!(!reminder.text.contains("INT-2049"));
    assert_eq!(
        reminders(&api, tenant::ID, id).await,
        [(
            user::NADIA,
            "success".to_string(),
            json!({ "approverId": user::MARC, "grouped": 1 })
        )]
    );

    let decision = format!("/approvals/{id}/decision");
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_requests_still_awaiting_a_manager_before_their_deadline_are_reminded() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();

    // Past its deadline: it expires instead.
    route_to_managers(&paul, &["write"]).await;
    let (_, past) = applying(&api, &nadia, ("INT-2052", "11:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let past = id_of(&past.expect("a request awaiting her manager"));
    due_in(&api, tenant::ID, past, -1).await;
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert!(reminders(&api, tenant::ID, past).await.is_empty());
    assert_eq!(runs.expire_overdue().await.unwrap(), 1);
    api.idle().await;

    // Hers to decide: nobody to remind.
    let (_, own) = applying(&api, &nadia, ("INT-2052", "09:00"), "Inchangé.").await;
    let own = id_of(&own.expect("a request she decides"));
    due_in(&api, tenant::ID, own, 120).await;
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert!(reminders(&api, tenant::ID, own).await.is_empty());
    let subjects: Vec<String> = api.mail.sent().into_iter().map(|m| m.subject).collect();
    assert!(
        subjects.iter().all(|s| !s.starts_with("Rappel")),
        "{subjects:?}"
    );

    let decision = format!("/approvals/{own}/decision");
    let refused = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(refused.status, StatusCode::OK, "{}", refused.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_mail_relay_nobody_is_reminded() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let marc = api.sign_in(user::MARC).await;
    route_to_managers(&paul, &["write"]).await;
    let (_, approval) = applying(&api, &nadia, ("INT-2049", "13:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let id = id_of(&approval.expect("a request awaiting her manager"));
    due_in(&api, tenant::ID, id, 120).await;

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert!(reminders(&api, tenant::ID, id).await.is_empty());

    let decision = format!("/approvals/{id}/decision");
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_managers_are_reminded_in_their_own_company() {
    let api = TestApi::start_with_mailer().await;
    // A request of the other company, due in two hours, that its contact
    // decides as manager.
    let run = stopped_run(
        &api,
        OTHER_TENANT_ID,
        OTHER_USER_ID,
        None,
        Some(saved()),
        &[],
    )
    .await;
    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    let foreign = create_approval(
        &mut tx,
        NewApproval {
            tenant_id: OTHER_TENANT_ID,
            run_id: run,
            requested_for_user_id: OTHER_USER_ID,
            tool_name: "planning_apply".into(),
            tool_call_id: "call-foreign".into(),
            action_summary: "Appliquer un planning".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: "Action modifiant des données".into(),
            expires_at: Some(Utc::now() + Duration::hours(2)),
            routing: ApprovalRouting::Manager(OTHER_USER_ID),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let runs = &api.app.services.runs;
    runs.claim_execution().await.unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, "contact@autre-entreprise.example");
    assert!(sent[0].text.contains("(Autre entreprise)"));
    // Audited in its company, nowhere in the demo one.
    assert_eq!(reminders(&api, OTHER_TENANT_ID, foreign.id).await.len(), 1);
    assert!(reminders(&api, tenant::ID, foreign.id).await.is_empty());

    let mut tx = api.app.services.db.begin(OTHER_TENANT_ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'failed' where id = $1")
        .bind(run)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("update approvals set status = 'cancelled' where id = $1")
        .bind(foreign.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
