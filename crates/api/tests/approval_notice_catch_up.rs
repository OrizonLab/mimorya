//! Notices a stop of the service lost (ADR-070, ADR-086): ten minutes after
//! its run paused on a request still awaiting its manager, never told of it,
//! the manager is told once, in their own company, by whichever process
//! catches up first; sooner, its notice may be under way. A request due
//! within a day is left to the reminder, a minute later at most, rather
//! than e-mailed twice.

mod support;

use axum::http::StatusCode;
use chrono::Duration;
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::{Value, json};
use support::manager_approvals::{
    catch_up, due_in, end_foreign_request, foreign_request, paused_ago, route_to_managers,
};
use support::planning::{apply_proposal_times, propose};
use support::recovery::restart;
use support::runs::start_run;
use support::{TestApi, email};
use uuid::Uuid;

/// The notices audited for the request, as `tenant_id` sees them: the
/// platform's, for whom the agent acts, with outcome and metadata.
async fn notices(api: &TestApi, tenant_id: Uuid, approval: Uuid) -> Vec<(Uuid, String, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let rows: Vec<(Uuid, String, Value)> = sqlx::query_as(
        "select on_behalf_of_user_id, outcome, metadata from audit_log \
         where action = 'approval.notified' and resource_id = $1 \
         and actor_type = 'system' and actor_user_id is null order by id",
    )
    .bind(approval.to_string())
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    rows
}

/// `count` requests of one pause of Nadia's, awaiting Marc, made by a
/// process that could not e-mail: the database holds what a stop between
/// the pause and its notice leaves. Marc decides them through `stopped`,
/// whose model then ends the run.
async fn lost_notices(stopped: &TestApi, start_time: &str, count: usize) -> Vec<Uuid> {
    let paul = stopped.sign_in(user::PAUL).await;
    let nadia = stopped.sign_in(user::NADIA).await;
    route_to_managers(&paul, &["write"]).await;
    stopped.script(vec![
        propose("INT-2049", start_time),
        apply_proposal_times("", count),
        ScriptedStep::text("Inchangé."),
    ]);
    let run = start_run(&nadia).await;
    stopped.idle().await;
    route_to_managers(&paul, &[]).await;
    let listed = nadia.get("/approvals?status=all").await.json();
    let ids: Vec<Uuid> = listed["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["runId"] == json!(run) && a["approver"]["id"] == json!(user::MARC))
        .map(|a| a["id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids.len(), count);
    for id in &ids {
        assert!(notices(stopped, tenant::ID, *id).await.is_empty());
    }
    ids
}

async fn reject(stopped: &TestApi, approvals: &[Uuid]) {
    let marc = stopped.sign_in(user::MARC).await;
    for approval in approvals {
        let decision = format!("/approvals/{approval}/decision");
        let rejected = marc.post(&decision, json!({ "approved": false })).await;
        assert_eq!(rejected.status, StatusCode::OK, "{}", rejected.text());
    }
    stopped.idle().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_notice_a_stop_lost_is_sent_ten_minutes_on_once() {
    let stopped = TestApi::start().await;
    let lost = lost_notices(&stopped, "09:00", 1).await;
    let id = lost[0];

    // Paused just now: its notice may be on its way, from another process.
    let started = TestApi::start_with_mailer().await;
    assert_eq!(restart(&started).await, 0, "no run was running");
    assert_eq!(catch_up(&started).await, 0);
    paused_ago(&started, tenant::ID, id, 9).await;
    assert_eq!(catch_up(&started).await, 0);
    assert!(started.mail.sent().is_empty());

    paused_ago(&started, tenant::ID, id, 11).await;
    assert_eq!(catch_up(&started).await, 1);
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, email(user::MARC));
    assert_eq!(
        sent[0].subject,
        "Une action de Nadia Benali attend votre validation"
    );
    assert!(!sent[0].text.contains("INT-2049"));
    assert_eq!(
        notices(&started, tenant::ID, id).await,
        [(
            user::NADIA,
            "success".to_string(),
            json!({ "approverId": user::MARC, "grouped": 1 })
        )]
    );

    // Told of it, Marc is not told again, nor at the next start.
    assert_eq!(catch_up(&started).await, 0);
    restart(&started).await;
    assert_eq!(started.mail.sent().len(), 1);
    reject(&stopped, &lost).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_workers_catching_up_at_once_tell_the_manager_once() {
    let stopped = TestApi::start().await;
    let lost = lost_notices(&stopped, "10:00", 1).await;
    paused_ago(&stopped, tenant::ID, lost[0], 11).await;

    let (one, two) = (
        TestApi::start_with_mailer().await,
        TestApi::start_with_mailer().await,
    );
    // Under way elsewhere: skipped, nobody told.
    let held = one
        .app
        .services
        .db
        .try_sweep_lock("approval-notice-catch-up")
        .await;
    let held = held.unwrap().expect("free");
    assert_eq!(catch_up(&two).await, 0);
    assert!(two.mail.sent().is_empty());
    held.release().await;

    let (told, also) = tokio::join!(catch_up(&one), catch_up(&two));
    assert_eq!(told + also, 1);
    assert_eq!(one.mail.sent().len() + two.mail.sent().len(), 1);
    assert_eq!(notices(&one, tenant::ID, lost[0]).await.len(), 1);
    reject(&stopped, &lost).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_request_due_within_a_day_is_left_to_the_reminder() {
    let stopped = TestApi::start().await;
    let lost = lost_notices(&stopped, "11:00", 2).await;
    let (soon, later) = (lost[0], lost[1]);
    due_in(&stopped, tenant::ID, soon, 120).await;
    for id in &lost {
        paused_ago(&stopped, tenant::ID, *id, 11).await;
    }

    // Told of the later one only...
    let started = TestApi::start_with_mailer().await;
    assert_eq!(catch_up(&started).await, 1);
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        sent[0].subject,
        "Une action de Nadia Benali attend votre validation"
    );
    let alone = json!({ "approverId": user::MARC, "grouped": 1 });
    assert_eq!(
        notices(&started, tenant::ID, later).await,
        [(user::NADIA, "success".to_string(), alone)]
    );
    assert!(notices(&started, tenant::ID, soon).await.is_empty());
    // ...reminded of the sooner one, with its deadline.
    assert_eq!(started.app.services.runs.remind_due().await.unwrap(), 1);
    let sent = started.mail.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(
        sent[1].subject,
        "Rappel : Une action de Nadia Benali attend votre validation"
    );
    reject(&stopped, &lost).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_lost_notices_are_sent_in_their_own_company() {
    let api = TestApi::start_with_mailer().await;
    let (run, foreign) = foreign_request(&api, Duration::days(3)).await;
    paused_ago(&api, OTHER_TENANT_ID, foreign, 11).await;

    assert_eq!(catch_up(&api).await, 1);
    let sent = api.mail.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, "contact@autre-entreprise.example");
    assert!(sent[0].text.contains("(Autre entreprise)"));
    // Audited in its company, nowhere in the demo one.
    assert_eq!(notices(&api, OTHER_TENANT_ID, foreign).await.len(), 1);
    assert!(notices(&api, tenant::ID, foreign).await.is_empty());
    end_foreign_request(&api, run, foreign).await;
}
