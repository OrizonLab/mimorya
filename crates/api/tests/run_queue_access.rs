//! A queued run as a worker starts it (ADR-085): a turn is given its
//! conversation's earlier messages then its request, named on the run, and
//! ends when that request is gone. The rights of whom it acts for are read
//! as they stand, before anything starts for it: an account disabled, a
//! company suspended or the right to use the agent withdrawn meanwhile ends
//! it, the model never called. A run of another company is claimed in its
//! own company alone.

mod support;

use mimorya_agent::runtime::ACCOUNT_INACTIVE;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::{
    AGENT_USE_WITHDRAWN, COMPANY_SUSPENDED, QUEUED_REQUEST_NOT_FOUND,
};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_queue::claim_queued_run;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::json;
use support::TestApi;
use support::queue::{API, as_owner, ask, status, take_queued, worker};
use support::recovery::{events, status_and_error, stopped_run};
use uuid::Uuid;

/// The run's end, read in the demo company.
async fn ended_with(worker: &TestApi, run: Uuid) -> Option<String> {
    let (status, error) = status_and_error(worker, tenant::ID, run).await;
    assert_eq!(status, "failed");
    error
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_turn_is_given_its_conversation_then_its_request() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let worker = worker().await;
    worker.script(vec![
        ScriptedStep::text("Première réponse."),
        ScriptedStep::text("Seconde réponse."),
    ]);
    let (conversation, _) = ask(&marc, "Première question").await;
    assert_eq!(take_queued(&worker).await, 1);
    let sent = marc
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": "Seconde question" }),
        )
        .await;
    assert_eq!(
        sent.status,
        axum::http::StatusCode::ACCEPTED,
        "{}",
        sent.text()
    );
    assert_eq!(take_queued(&worker).await, 1);

    let asked = &worker.model.as_ref().unwrap().requests()[1];
    let messages: Vec<String> = asked.messages.iter().map(|m| format!("{m:?}")).collect();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert!(messages[0].contains("Première question"), "{messages:?}");
    assert!(messages[1].contains("Première réponse."), "{messages:?}");
    assert!(messages[2].contains("Seconde question"), "{messages:?}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_turn_whose_request_is_gone_ends_instead() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (_, run) = ask(&marc, "Fais le point").await;
    as_owner(
        "update agent_runs set request_message_id = null where id = $1",
        run,
    )
    .await;

    let worker = worker().await;
    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(
        ended_with(&worker, run).await.as_deref(),
        Some(QUEUED_REQUEST_NOT_FOUND)
    );
    assert!(worker.model.as_ref().unwrap().requests().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rights_lost_while_queued_end_the_run_before_anything_starts() {
    let api = TestApi::start_with(API).await;
    let worker = worker().await;
    let cases: [(Uuid, &str, &str, &str); 3] = [
        (
            user::MARC,
            "update users set status = 'disabled' where id = $1",
            "update users set status = 'active' where id = $1",
            ACCOUNT_INACTIVE,
        ),
        (
            tenant::ID,
            "update tenants set status = 'suspended' where id = $1",
            "update tenants set status = 'active' where id = $1",
            COMPANY_SUSPENDED,
        ),
        (
            user::ISABELLE,
            // Only the base role carries agent:use.
            "delete from role_assignments where user_id = $1 \
             and role_id in (select id from roles where key = 'member')",
            "select $1",
            AGENT_USE_WITHDRAWN,
        ),
    ];
    for (changed, change, undo, why) in cases {
        let account = if changed == user::ISABELLE {
            user::ISABELLE
        } else {
            user::MARC
        };
        let client = api.sign_in(account).await;
        let (_, run) = ask(&client, "Fais le point").await;
        as_owner(change, changed).await;
        // Read afresh, as the next step would.
        worker.app.services.access.invalidate(tenant::ID);
        let taken = take_queued(&worker).await;
        as_owner(undo, changed).await;
        worker.app.services.access.invalidate(tenant::ID);
        assert_eq!(taken, 1, "{why}");
        assert_eq!(ended_with(&worker, run).await.as_deref(), Some(why));
        // Nothing started for it: queued, then its end, in its journal.
        let journal: Vec<String> = events(&worker, tenant::ID, run)
            .await
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        assert_eq!(journal, ["run_queued", "run_finished"], "{why}");
    }
    assert!(worker.model.as_ref().unwrap().requests().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_queued_run_is_claimed_in_its_own_company_alone() {
    let worker = worker().await;
    let foreign = stopped_run(&worker, OTHER_TENANT_ID, OTHER_USER_ID, None, None, &[]).await;
    as_owner(
        "update agent_runs set status = 'queued' where id = $1",
        foreign,
    )
    .await;

    // From the demo company's transactions: nothing to claim.
    let mut demo = worker.app.services.db.begin(tenant::ID).await.unwrap();
    assert!(
        claim_queued_run(&mut demo, foreign)
            .await
            .unwrap()
            .is_none()
    );
    drop(demo);
    assert_eq!(status(foreign).await, "queued");

    // Taken in its company: no request, it ends there.
    assert_eq!(take_queued(&worker).await, 1);
    let (status, error) = status_and_error(&worker, OTHER_TENANT_ID, foreign).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(QUEUED_REQUEST_NOT_FOUND));
    assert!(events(&worker, tenant::ID, foreign).await.is_empty());
    let ended = events(&worker, OTHER_TENANT_ID, foreign).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
}
