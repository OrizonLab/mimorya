//! The passes that play the evaluations (ADR-104, ADR-157): one per
//! company, under its own lock, so that another company's evaluation is
//! played while one is, and one company's evaluations one at a time; four
//! companies at once in a process, a failing pass ending its company's
//! alone. Only a process that executes runs plays them; an evaluation a
//! stopped process left running fails; a pass whose evaluation another
//! ended, or whose lock's session is lost, plays no further, what its case
//! cost counted all the same (ADR-154).

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::skill_evaluations::{
    COMPANIES_AT_ONCE, evaluation_sweep, play_company, play_pending_evaluations,
};
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use support::TestApi;
use support::evaluation_passes::{
    TAKEN_OVER, allow_endings, ask_elsewhere, forget_elsewhere, lose_lock, once_ended_elsewhere,
    refuse_endings, status_elsewhere, taken_over,
};
use support::llm::SCRIPTED_CALL_MICROS;
use support::skill_evaluations::{
    evaluations, executing, forget, launched, new_version, play, status_of, stored_evaluation,
    verdict, write_case,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_evaluation_is_played_while_one_is() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    // The company's evaluation first, the other company's since: one pass
    // for the platform would leave the latter waiting until the former ends.
    let id = launched(&paul, DEMO_SKILL_ID, 1).await;
    let elsewhere = ask_elsewhere().await;
    let seen = Arc::new(Mutex::new(String::new()));
    let step = once_ended_elsewhere("Trois retards.", elsewhere, Arc::clone(&seen));
    api.script(vec![step, verdict(true)]);
    play(&api).await;

    assert_eq!(
        *seen.lock().unwrap(),
        "failed",
        "ended while the case played"
    );
    assert_eq!(status_of(&api, &id).await.0, "completed");
    assert_eq!(forget_elsewhere(elsewhere).await, "failed");
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_left_running_by_a_stopped_process_fails_under_its_company_s_lock() {
    let api = TestApi::start().await;
    let left = stored_evaluation(&api, "running", 0).await.to_string();
    let elsewhere = ask_elsewhere().await;
    // A process that executes no runs plays nothing.
    let services = &api.app.services;
    assert_eq!(play_pending_evaluations(services).await.unwrap(), 0);
    assert_eq!(status_of(&api, &left).await.0, "running");
    // Nor in a company whose lock another holds: the other company's is.
    services.runs.start_executing().unwrap();
    let lock = evaluation_sweep(tenant::ID);
    let held = services.db.try_sweep_lock(&lock).await.unwrap();
    let held = held.expect("free");
    assert_eq!(play_pending_evaluations(services).await.unwrap(), 1);
    assert_eq!(status_of(&api, &left).await.0, "running");
    assert_eq!(forget_elsewhere(elsewhere).await, "failed");
    held.release().await;

    play(&api).await;
    let (status, failure, _) = status_of(&api, &left).await;
    assert_eq!(status, "failed");
    assert!(
        failure
            .unwrap()
            .contains("interrompue par un arrêt du service")
    );
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_whose_evaluation_another_ended_writes_nothing_more_and_stops() {
    let api = TestApi::start_priced().await;
    let paul = api.sign_in(user::PAUL).await;
    let version = new_version(&paul).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    api.script(vec![taken_over("Trois retards."), verdict(true)]);
    let id = launched(&paul, DEMO_SKILL_ID, version).await;
    executing(&api).await;
    // It played one, and left the one asked for since to the other pass.
    let services = &api.app.services;
    assert_eq!(play_pending_evaluations(services).await.unwrap(), 1);

    let (status, failure, results) = status_of(&api, &id).await;
    assert_eq!(status, "failed");
    assert_eq!(failure.as_deref(), Some(TAKEN_OVER));
    assert_eq!(results, 0, "no result written");
    let listed = evaluations(&paul, DEMO_SKILL_ID).await;
    assert_eq!(listed[0]["status"], "queued", "{}", listed[0]);
    assert_eq!(listed[1]["id"], id.as_str());
    // What the case it played cost is spent all the same (ADR-154): the
    // agent's call and the judge's.
    let spent = paul.get("/admin/evaluation-settings").await.json()["spentMicros"].clone();
    assert_eq!(spent, json!(2 * SCRIPTED_CALL_MICROS));
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_whose_lock_s_session_is_lost_plays_no_further() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    write_case(&paul, DEMO_SKILL_ID, "Retards", "Quels retards ?").await;
    let id = launched(&paul, DEMO_SKILL_ID, 1).await;
    // Its lock's session lost before it plays one: another may play them.
    let services = &api.app.services;
    let name = evaluation_sweep(tenant::ID);
    let lock = services.db.try_sweep_lock(&name).await.unwrap();
    let lock = lock.expect("free");
    lose_lock(tenant::ID).await;
    assert_eq!(play_company(services, tenant::ID, lock).await, 0);
    assert_eq!(status_of(&api, &id).await.0, "queued");

    api.script(vec![ScriptedStep::text("Trois retards."), verdict(true)]);
    play(&api).await;
    assert_eq!(status_of(&api, &id).await.0, "completed");
    forget(&api, DEMO_SKILL_ID).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_process_plays_four_companies_at_once_at_most() {
    let api = TestApi::start().await;
    let elsewhere = ask_elsewhere().await;
    executing(&api).await;
    // Four companies played meanwhile in the process: the fifth waits.
    let services = &api.app.services;
    let slots = &services.evaluation_slots;
    let mut taken: Vec<_> = (0..COMPANIES_AT_ONCE).map(|_| slots.take()).collect();
    assert!(taken.iter().all(Option::is_some));
    assert!(slots.take().is_none(), "no fifth place");
    assert_eq!(play_pending_evaluations(services).await.unwrap(), 0);
    assert_eq!(status_elsewhere(elsewhere).await, "queued");

    // One ended, the next pass plays it.
    taken.pop();
    play_pending_evaluations(services).await.unwrap();
    let name = evaluation_sweep(OTHER_TENANT_ID);
    let done = services.db.sweep_lock(&name, Duration::from_secs(60)).await;
    done.unwrap().release().await;
    assert_eq!(forget_elsewhere(elsewhere).await, "failed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_s_failing_pass_leaves_the_others_played() {
    let api = TestApi::start().await;
    let left = stored_evaluation(&api, "running", 0).await.to_string();
    let elsewhere = ask_elsewhere().await;
    refuse_endings().await;
    executing(&api).await;
    // The company's pass fails as it ends the one left running; the other
    // company's plays its own.
    let services = &api.app.services;
    play_pending_evaluations(services).await.unwrap();
    assert_eq!(status_of(&api, &left).await.0, "running");
    assert_eq!(forget_elsewhere(elsewhere).await, "failed");

    allow_endings().await;
    play(&api).await;
    assert_eq!(status_of(&api, &left).await.0, "failed");
    forget(&api, DEMO_SKILL_ID).await;
}
