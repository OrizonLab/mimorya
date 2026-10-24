//! The run queue (ADR-085): a run an `api` process starts waits queued,
//! executed by no process of that role, until a worker claims it, and
//! executes it as it would have: the request, its conversation, the answer
//! in it. A queued run blocks another turn in its conversation. A decision
//! taken through an `api` process queues the run's resumption, with how the
//! request ended. A queued run is claimed by one worker alone; one queued
//! for more than an hour ends instead. A worker woken by the queue's notice
//! claims a run at once. A worker, which listens to no one, sends the
//! model's text to an api process all the same.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::{NOT_AN_EXECUTOR, QUEUED_TOO_LONG};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::run_queue::claim_queued_run;
use serde_json::json;
use support::TestApi;
use support::leases::lease_of;
use support::manager_approvals::listed;
use support::planning::{apply_proposal, propose};
use support::queue::{API, ask, queued_ago, status, take_queued, worker};
use support::recovery::{events, status_and_error};
use support::relay::{next_event, relaying};

const REQUEST: &str = "Fais le point sur la semaine";

/// The kinds of the run's events, in order.
async fn kinds(api: &TestApi, run: uuid::Uuid) -> Vec<String> {
    let journal = events(api, tenant::ID, run).await;
    journal.into_iter().map(|(kind, _)| kind).collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_an_api_process_starts_waits_queued_until_a_worker_executes_it() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (conversation, run) = ask(&marc, REQUEST).await;
    api.idle().await;
    assert_eq!(status(run).await, "queued");
    assert_eq!(lease_of(run).await, (None, None));
    // Its stream says so.
    assert_eq!(kinds(&api, run).await, ["run_queued"]);
    // An api process takes nothing up, nor any queued run.
    let refused = api.app.services.runs.start_executing().unwrap_err();
    assert_eq!(refused.to_string(), NOT_AN_EXECUTOR);
    assert_eq!(take_queued(&api).await, 0);
    assert_eq!(status(run).await, "queued");

    let worker = worker().await;
    worker.script(vec![ScriptedStep::text(
        "Deux interventions restent en cours.",
    )]);
    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(status(run).await, "completed");
    assert_eq!(kinds(&api, run).await[..2], ["run_queued", "run_started"]);
    let asked = &worker.model.as_ref().unwrap().requests()[0];
    let last = format!("{:?}", asked.messages.last().unwrap());
    assert!(last.contains(REQUEST), "{last}");
    // The answer, read through the api process.
    let detail = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    let messages = detail["messages"].as_array().unwrap();
    assert_eq!(messages[0]["content"], REQUEST);
    assert_eq!(
        messages[1]["content"],
        "Deux interventions restent en cours."
    );
    assert_eq!(take_queued(&worker).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_run_blocks_another_turn_in_its_conversation() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (conversation, run) = ask(&marc, REQUEST).await;
    let again = marc
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": "Et la suivante ?" }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());

    let worker = worker().await;
    worker.script(vec![ScriptedStep::text("Rien à signaler.")]);
    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(status(run).await, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_decision_taken_through_an_api_process_queues_the_run_s_resumption() {
    let api = TestApi::start_with(API).await;
    let nadia = api.sign_in(user::NADIA).await;
    let worker = worker().await;
    worker.script(vec![
        propose("INT-2049", "09:00"),
        apply_proposal(""),
        ScriptedStep::text("Le planning reste inchangé."),
    ]);
    let (_, run) = ask(&nadia, "Planifie INT-2049").await;
    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(status(run).await, "awaiting_approval");

    let approval = listed(&nadia, &run.to_string()).await.expect("her request");
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let decided = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    assert_eq!(decided.json()["resumed"], true);
    assert_eq!(status(run).await, "queued");
    let journal = kinds(&api, run).await;
    assert_eq!(journal.last().map(String::as_str), Some("run_queued"));

    assert_eq!(take_queued(&worker).await, 1);
    assert_eq!(status(run).await, "completed");
    // Resumed knowing the request was refused.
    let resumed = worker.model.as_ref().unwrap().requests();
    let last = format!("{:?}", resumed.last().unwrap().messages.last().unwrap());
    assert!(last.to_lowercase().contains("refus"), "{last}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_queued_run_is_claimed_by_one_worker_alone() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (_, run) = ask(&marc, REQUEST).await;
    let db = &api.app.services.db;

    let mut first = db.begin(tenant::ID).await.unwrap();
    assert!(claim_queued_run(&mut first, run).await.unwrap().is_some());
    // Being claimed: skipped by another, without waiting.
    let mut second = db.begin(tenant::ID).await.unwrap();
    assert!(claim_queued_run(&mut second, run).await.unwrap().is_none());
    drop(second);
    // The first one gave up: queued again, and claimed.
    drop(first);
    let mut third = db.begin(tenant::ID).await.unwrap();
    assert!(claim_queued_run(&mut third, run).await.unwrap().is_some());
    drop(third);

    let worker = worker().await;
    worker.script(vec![ScriptedStep::text("Fait.")]);
    assert_eq!(take_queued(&worker).await, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_queued_for_more_than_an_hour_ends_instead() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (_, run) = ask(&marc, REQUEST).await;
    queued_ago(run, 61).await;

    let worker = worker().await;
    assert_eq!(take_queued(&worker).await, 1);
    let (status, error) = status_and_error(&worker, tenant::ID, run).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(QUEUED_TOO_LONG));
    let ended = events(&worker, tenant::ID, run).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
    assert!(worker.model.as_ref().unwrap().requests().is_empty());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_worker_woken_by_the_queue_s_notice_claims_a_run_at_once() {
    let worker = TestApi::start_with(support::queue::WORKER).await;
    // Taken up: its queue's first pass, then it waits.
    worker.app.recover_interrupted_runs().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    worker.script(vec![ScriptedStep::text("Fait sans attendre.")]);

    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (_, run) = ask(&marc, REQUEST).await;
    // Well before the queue is read again by itself (every five seconds).
    let mut ended = String::new();
    for _ in 0..30 {
        worker.idle().await;
        ended = status(run).await;
        if ended == "completed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(ended, "completed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_worker_which_listens_to_no_one_sends_its_text_to_an_api_process() {
    let api = TestApi::start_with(API).await;
    let marc = api.sign_in(user::MARC).await;
    let (_, run) = ask(&marc, REQUEST).await;
    relaying(&api).await;
    let mut followed = api.app.services.bus.subscribe(tenant::ID, run);
    let worker = worker().await;

    // Written until the api process listens: sent while nobody did, it is lost.
    let mut relayed = None;
    for _ in 0..50 {
        worker.app.services.bus.publish_delta(run, None, "Bonjour");
        relayed = next_event(&mut followed, 0.2).await;
        if relayed.is_some() {
            break;
        }
    }
    let delta = relayed.expect("text relayed");
    assert_eq!(delta.data, json!({ "text": "Bonjour" }));
    worker.script(vec![ScriptedStep::text("Fait.")]);
    assert_eq!(take_queued(&worker).await, 1);
}
