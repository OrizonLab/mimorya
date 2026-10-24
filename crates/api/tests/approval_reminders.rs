//! Reminding managers the day before requests expire (ADR-062): once per
//! request, even refused by the relay, in one message per manager and run,
//! by one process that executes runs at a time (ADR-086), only with a mail relay, only while the
//! run is paused and the manager's account active; never a request its
//! requester decides, nor one decided or past its deadline, nor one of a
//! suspended company; each company's in its own company.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::manager_approvals::{
    applying, due_in, end_foreign_request, foreign_request, route_to_managers,
};
use support::planning::{apply_proposal_times, apply_reply, propose};
use support::runs::start_run;
use support::sweeps::sweep_from_elsewhere;
use support::{TestApi, email, owner_url};
use uuid::Uuid;

fn id_of(approval: &Value) -> Uuid {
    approval["id"].as_str().unwrap().parse().unwrap()
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
    runs.start_executing().unwrap();
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
    runs.start_executing().unwrap();

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
    runs.start_executing().unwrap();
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
    // A paused run of the other company, its request due in two hours, that
    // its contact decides as manager.
    let (run, foreign) = foreign_request(&api, Duration::hours(2)).await;

    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    // Not while its company is suspended: its people cannot sign in.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let set_status = |status: &'static str| {
        sqlx::query("update tenants set status = $2 where id = $1")
            .bind(OTHER_TENANT_ID)
            .bind(status)
            .execute(&owner)
    };
    set_status("suspended").await.unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert!(api.mail.sent().is_empty());
    set_status("active").await.unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, "contact@autre-entreprise.example");
    assert!(sent[0].text.contains("(Autre entreprise)"));
    // Audited in its company, nowhere in the demo one.
    assert_eq!(reminders(&api, OTHER_TENANT_ID, foreign).await.len(), 1);
    assert!(reminders(&api, tenant::ID, foreign).await.is_empty());

    end_foreign_request(&api, run, foreign).await;
}

/// Two requests of one pause, both for Marc and due in two hours: their ids.
async fn two_requests_due(api: &TestApi, nadia: &support::Client<'_>) -> Vec<Uuid> {
    let paul = api.sign_in(user::PAUL).await;
    route_to_managers(&paul, &["write"]).await;
    api.script(vec![
        propose("INT-2049", "10:00"),
        apply_proposal_times("", 2),
        ScriptedStep::text("Inchangé."),
    ]);
    let run = start_run(nadia).await;
    api.idle().await;
    route_to_managers(&paul, &[]).await;
    let all = nadia.get("/approvals?status=all").await.json();
    let ids: Vec<Uuid> = all["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["runId"] == json!(run))
        .map(id_of)
        .collect();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        due_in(api, tenant::ID, *id, 120).await;
    }
    ids
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_s_requests_are_reminded_together_while_they_wait_once_even_refused() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let ids = two_requests_due(&api, &nadia).await;
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();

    // Both still waiting: one reminder for the two.
    assert_eq!(runs.remind_due().await.unwrap(), 2);
    let sent = api.mail.sent();
    assert_eq!(
        sent.last().unwrap().subject,
        "Rappel : 2 actions de Nadia Benali attendent votre validation"
    );
    for id in &ids {
        let grouped = json!({ "approverId": user::MARC, "grouped": 2 });
        assert_eq!(
            reminders(&api, tenant::ID, *id).await,
            [(user::NADIA, "success".to_string(), grouped)]
        );
    }

    // A third request of a later pause: not while Marc is disabled; refused
    // by the relay, it is traced so and never sent again.
    let marc = api.sign_in(user::MARC).await;
    for id in &ids {
        let decision = format!("/approvals/{id}/decision");
        let rejected = marc.post(&decision, json!({ "approved": false })).await;
        assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    }
    api.idle().await;
    route_to_managers(&paul, &["write"]).await;
    let (_, third) = applying(&api, &nadia, ("INT-2049", "15:00"), "Inchangé.").await;
    route_to_managers(&paul, &[]).await;
    let third = id_of(&third.expect("a request awaiting her manager"));
    due_in(&api, tenant::ID, third, 120).await;
    let account = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    api.mail.fail();
    assert_eq!(runs.remind_due().await.unwrap(), 1);
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    let failed = json!({ "approverId": user::MARC, "grouped": 1 });
    assert_eq!(
        reminders(&api, tenant::ID, third).await,
        [(user::NADIA, "failure".to_string(), failed)]
    );

    let marc = api.sign_in(user::MARC).await;
    let decision = format!("/approvals/{third}/decision");
    let rejected = marc.post(&decision, json!({ "approved": false })).await;
    assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    api.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_ended_elsewhere_before_its_pause_makes_no_request_to_remind() {
    let api = TestApi::start_with_mailer().await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    // Failed by another process while the model answers: no longer running
    // under its lease, the step makes no request (ADR-086).
    api.script(vec![
        propose("INT-2052", "14:00"),
        ScriptedStep::dynamic(|request| {
            sweep_from_elsewhere(user::NADIA);
            apply_reply(request, "", 2)
        }),
    ]);
    let run = start_run(&nadia).await;
    api.idle().await;
    route_to_managers(&paul, &[]).await;
    let listed = nadia.get("/approvals?status=all").await.json();
    let made = listed["approvals"].as_array().unwrap();
    assert!(!made.iter().any(|a| a["runId"] == json!(run)), "{made:?}");

    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    assert_eq!(runs.remind_due().await.unwrap(), 0);
    assert!(api.mail.sent().is_empty(), "{:?}", api.mail.sent());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_workers_reminding_at_once_remind_the_manager_once() {
    let one = TestApi::start_with_mailer().await;
    let two = TestApi::start_with_mailer().await;
    let (run, foreign) = foreign_request(&one, Duration::hours(2)).await;
    for worker in [&one, &two] {
        worker.app.services.runs.start_executing().unwrap();
    }

    // Under way elsewhere: skipped, nobody reminded.
    let held = one
        .app
        .services
        .db
        .try_sweep_lock("approval-reminders")
        .await;
    let held = held.unwrap().expect("free");
    assert_eq!(two.app.services.runs.remind_due().await.unwrap(), 0);
    assert!(two.mail.sent().is_empty());
    held.release().await;

    let (first, second) = tokio::join!(
        one.app.services.runs.remind_due(),
        two.app.services.runs.remind_due()
    );
    assert_eq!(first.unwrap() + second.unwrap(), 1);
    assert_eq!(one.mail.sent().len() + two.mail.sent().len(), 1);
    assert_eq!(reminders(&one, OTHER_TENANT_ID, foreign).await.len(), 1);
    end_foreign_request(&one, run, foreign).await;
}
