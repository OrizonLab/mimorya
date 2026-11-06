//! A run no one awaits waits for a connector still starting however long
//! its start takes (ADR-177): a hook's run, a mission's stage and a
//! scheduled task's run, through the executor, use its tools once its
//! server answers, later than the patience a person's turn has. A person's
//! turn: `connector_start_patience.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use support::TestApi;
use support::hooks::{deliver, hook};
use support::missions::{approving_review, entrust, executing, stages};
use support::queue::take_queued;
use support::scheduled::{due, every_day, fire, schedule};
use support::slow_starts::{
    PATIENT, answering_late, last_run, remove, search_status, searching, slow_connector,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_hook_s_run_waits_for_its_connector_however_long_it_starts() {
    let api = TestApi::start_with(&PATIENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, hold) = slow_connector(&paul).await;
    let (hook_id, token) = hook(&marc, "Panne du circulateur").await;
    api.script(searching("Voici la procédure du circulateur."));
    let delivered = async {
        let accepted = deliver(&api, hook_id, &token, "{}").await;
        api.idle().await;
        accepted
    };
    let (accepted, ()) = tokio::join!(delivered, answering_late(&hold));
    assert_eq!(accepted.status, StatusCode::ACCEPTED, "{}", accepted.text());
    let run = last_run(&api).await;
    assert_eq!(search_status(&marc, run).await, "succeeded");
    let deleted = marc.delete(&format!("/inbound-hooks/{hook_id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mission_s_stage_waits_for_its_connector_however_long_it_starts() {
    let api = TestApi::start_with(&PATIENT).await;
    executing(&api);
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, hold) = slow_connector(&paul).await;
    let mission = entrust(&marc, "Suivi du circulateur").await;
    let mut steps = searching("Suivi proposé : la procédure du circulateur.");
    steps.push(approving_review());
    api.script(steps);
    let (taken, ()) = tokio::join!(take_queued(&api), answering_late(&hold));
    assert_eq!(taken, 1);
    let [(stage, _)] = stages(mission).await[..] else {
        panic!("one stage");
    };
    assert_eq!(search_status(&marc, stage).await, "succeeded");
    remove(&paul, &id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_scheduled_task_s_run_waits_for_its_connector_however_long_it_starts() {
    let api = TestApi::start_with(&PATIENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, hold) = slow_connector(&paul).await;
    let task = schedule(&marc, every_day("Point du circulateur")).await;
    due(task, chrono::Duration::minutes(1)).await;
    api.script(searching("Voici la procédure du circulateur."));
    let (fired, ()) = tokio::join!(fire(&api), answering_late(&hold));
    assert_eq!(fired, 1);
    let run = last_run(&api).await;
    assert_eq!(search_status(&marc, run).await, "succeeded");
    let deleted = marc.delete(&format!("/scheduled-tasks/{task}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    remove(&paul, &id).await;
}
