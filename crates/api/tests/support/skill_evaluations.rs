//! Evaluations of a skill's versions (ADR-104): a version and cases to try,
//! the model's script for one case, an evaluation asked for, played and
//! read back, and what is left of them once a test is done.

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_api::services::skill_evaluations::{EVALUATION_SWEEP, play_pending_evaluations};
use mimorya_core::demo::tenant;
use mimorya_db::seed::DEMO_SKILL_ID;
use serde_json::{Value, json};
use uuid::Uuid;

use super::skill_proposals::{SKILL, SKILLS};
use super::{Client, TestApi, TestResponse};

/// What only the version written by `new_version` says.
pub const SECOND_VERSION: &str = "Relire chaque retard avec son technicien.";
/// The issue the judge finds in a case it fails.
pub const ISSUE: &str = "Aucun retard n’est cité.";

/// A new version of the weekly report, a draft; its number.
pub async fn new_version(paul: &Client<'_>) -> i32 {
    let content = format!("# Synthèse\n1. Lister les interventions.\n2. {SECOND_VERSION}");
    let added = paul
        .post(
            &format!("{SKILLS}/{DEMO_SKILL_ID}/versions"),
            json!({ "content": content }),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    added.json()["version"].as_i64().unwrap() as i32
}

/// A case of skill `skill`, asking `request`; its id.
pub async fn write_case(paul: &Client<'_>, skill: Uuid, name: &str, request: &str) -> String {
    let created = paul
        .post(
            &format!("{SKILLS}/{skill}/cases"),
            json!({ "name": name, "request": request, "criteria": ["Cite chaque retard."] }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_owned()
}

/// The model's script for one case played for one version: the agent loads
/// the weekly report or not, answers `answer`, and the judge passes it or
/// not.
pub fn played(loads: bool, answer: &str, passes: bool) -> Vec<ScriptedStep> {
    let mut steps = Vec::new();
    if loads {
        steps.push(ScriptedStep::calls(
            "",
            vec![call("skill_read", json!({ "name": SKILL }))],
        ));
    }
    steps.extend([ScriptedStep::text(answer), verdict(passes)]);
    steps
}

/// The judge passes an answer or not.
pub fn verdict(passes: bool) -> ScriptedStep {
    let issues: Vec<&str> = if passes { Vec::new() } else { vec![ISSUE] };
    ScriptedStep::calls(
        "",
        vec![call(
            "submit_verdict",
            json!({ "pass": passes, "issues": issues }),
        )],
    )
}

pub async fn launch(client: &Client<'_>, skill: Uuid, version: i32) -> TestResponse {
    client
        .post(
            &format!("{SKILLS}/{skill}/evaluations"),
            json!({ "version": version }),
        )
        .await
}

/// Launches the evaluation of `version`; its id.
pub async fn launched(client: &Client<'_>, skill: Uuid, version: i32) -> String {
    let launched = launch(client, skill, version).await;
    assert_eq!(launched.status, StatusCode::CREATED, "{}", launched.text());
    launched.json()["id"].as_str().unwrap().to_owned()
}

/// Makes the process execute runs, once the pass a launch woke found it
/// did not: the test's own pass then plays alone.
pub async fn executing(api: &TestApi) {
    // Spawned by the launch, the woken pass runs first.
    tokio::task::yield_now().await;
    api.app.services.runs.start_executing().unwrap();
}

/// Plays the evaluations waiting, as a process that executes runs does.
pub async fn play(api: &TestApi) {
    executing(api).await;
    let services = &api.app.services;
    play_pending_evaluations(services).await.unwrap();
    // A maintenance pass may have played them meanwhile: once it is done.
    let wait = Duration::from_secs(60);
    let done = services.db.sweep_lock(EVALUATION_SWEEP, wait).await;
    done.unwrap().release().await;
}

pub async fn evaluations(client: &Client<'_>, skill: Uuid) -> Vec<Value> {
    let listed = client.get(&format!("{SKILLS}/{skill}/evaluations")).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["evaluations"].as_array().unwrap().clone()
}

pub async fn detail(client: &Client<'_>, id: &str) -> TestResponse {
    client.get(&format!("/admin/skill-evaluations/{id}")).await
}

/// The status, failure and number of results of the company's evaluation
/// `id`.
pub async fn status_of(api: &TestApi, id: &str) -> (String, Option<String>, i64) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id: Uuid = id.parse().unwrap();
    let row: (String, Option<String>) =
        sqlx::query_as("select status, failure from skill_evaluations where id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let results: i64 = sqlx::query_scalar(
        "select count(*) from skill_evaluation_results where evaluation_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    (row.0, row.1, results)
}

/// An evaluation of the demo skill written as `status`, `days` old.
pub async fn stored_evaluation(api: &TestApi, status: &str, days: i32) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into skill_evaluations (tenant_id, skill_id, version, status, cases, created_at) \
         values ($1, $2, 1, $3, '[]', now() - make_interval(days => $4)) returning id",
    )
    .bind(tenant::ID)
    .bind(DEMO_SKILL_ID)
    .bind(status)
    .bind(days)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// Deletes the company's evaluations, what they cost (ADR-154) and the
/// cases of `skill`.
pub async fn forget(api: &TestApi, skill: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from skill_evaluation_spending")
        .execute(&mut *tx)
        .await
        .unwrap();
    for statement in [
        "delete from skill_evaluations",
        "delete from skill_eval_cases where skill_id = $1",
    ] {
        sqlx::query(statement)
            .bind(skill)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

/// How many rows of `table` the company holds.
pub async fn count(api: &TestApi, table: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let count = sqlx::query_scalar(&format!("select count(*) from {table}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    count
}
