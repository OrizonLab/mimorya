//! Scheduled tasks fired when due (ADR-080): the instruction is sent as its
//! author's, in a new conversation of theirs, once per occurrence even when
//! two passes race, its run's start audited as the platform's on their
//! behalf, and the task runs next at its next time. The server fires none
//! before the runs a stop interrupted are taken up, goes on batch after
//! batch, and fires no more once stopping; a change that leaves when a task
//! runs keeps its occurrence due.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::scheduled_tasks::fire_task;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::scheduled::{
    INSTRUCTION, TASKS, due, every_day, fire, listed, occurrences_audited, opened, schedule,
};
use uuid::Uuid;

/// How the start of the run in `conversation` was audited: actor type,
/// actor, on whose behalf, metadata.
async fn run_start(
    api: &TestApi,
    conversation: &str,
) -> (String, Option<Uuid>, Option<Uuid>, Value) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select actor_type, actor_user_id, on_behalf_of_user_id, metadata from audit_log \
         where action = 'agent.run.started' and resource_id = $1",
    )
    .bind(conversation)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_due_task_runs_once_as_its_author_in_a_new_conversation() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = schedule(&marc, every_day("Point du matin")).await;
    let before = opened(user::MARC).await;
    api.script(vec![ScriptedStep::text(
        "Deux interventions restent en cours.",
    )]);
    due(id, Duration::minutes(1)).await;

    // Two passes at once: the occurrence fires once.
    let (one, other) = tokio::join!(fire(&api), fire(&api));
    assert_eq!(one + other, 1);
    assert_eq!(fire(&api).await, 0);
    // Listed due by a pass that comes too late: it fired already.
    let late = fire_task(&api.app.services, tenant::ID, id, Utc::now()).await;
    assert!(!late.unwrap());

    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"]["outcome"], "started");
    assert_eq!(task["lastOccurrence"]["problem"], Value::Null);
    let next: DateTime<Utc> = task["nextRunAt"].as_str().unwrap().parse().unwrap();
    assert!(next > Utc::now());
    assert_eq!(opened(user::MARC).await, before + 1);
    let conversation = task["lastConversationId"].as_str().unwrap();
    let detail = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(detail["conversation"]["title"], "Point du matin");
    let messages = detail["messages"].as_array().unwrap();
    assert_eq!(messages[0]["content"], INSTRUCTION);
    assert_eq!(
        messages[1]["content"],
        "Deux interventions restent en cours."
    );
    // Marc's alone.
    let foreign = lucas.get(&format!("/conversations/{conversation}")).await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);

    let audit = occurrences_audited(&api, id).await;
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(
        (audit[0].0.as_str(), audit[0].1.as_str(), audit[0].2),
        ("scheduled_task.fired", "system", Some(user::MARC))
    );
    // Its run started by the platform, not by Marc, for him.
    let (actor_type, actor, behalf, metadata) = run_start(&api, conversation).await;
    assert_eq!(
        (actor_type.as_str(), actor, behalf),
        ("system", None, Some(user::MARC))
    );
    assert_eq!(metadata["scheduledTaskId"], json!(id));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_server_fires_tasks_once_interrupted_runs_are_taken_up() {
    let api = TestApi::start_scheduling().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Au démarrage")).await;
    api.script(vec![ScriptedStep::text("Point fait.")]);
    due(id, Duration::minutes(1)).await;

    // Up, the runs a stop interrupted not yet taken up: a run started now
    // would be taken for one of them. Nothing fires.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(listed(&marc, id).await["lastOccurrence"], Value::Null);

    // Taken up: its first pass, at once.
    api.app.recover_interrupted_runs().await.unwrap();
    let mut outcome = Value::Null;
    for _ in 0..50 {
        outcome = listed(&marc, id).await["lastOccurrence"]["outcome"].clone();
        if !outcome.is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(outcome, "started");
    api.idle().await;

    // Stopping, it ends between two passes: the server stops.
    let TestApi { app, .. } = api;
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(10), app.shutdown()).await;
    assert!(
        stopped.is_ok(),
        "the scheduler kept the server from stopping"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stopping_server_leaves_due_tasks_to_its_next_start() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("À l’arrêt")).await;
    due(id, Duration::minutes(1)).await;
    api.app.begin_shutdown();
    assert_eq!(fire(&api).await, 0);
    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"], Value::Null);
    let deleted = marc.delete(&format!("{TASKS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_change_that_leaves_when_it_runs_keeps_its_occurrence() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Relevé")).await;
    let url = format!("{TASKS}/{id}");

    // Due, not fired yet, when its author rewrites what it asks.
    due(id, Duration::minutes(1)).await;
    let rewritten = marc
        .patch(
            &url,
            json!({ "title": "Relevé du jour", "instruction": "Relève les retards." }),
        )
        .await;
    assert_eq!(rewritten.status, StatusCode::OK, "{}", rewritten.text());
    api.script(vec![ScriptedStep::text("Aucun retard.")]);
    assert_eq!(fire(&api).await, 1);
    let conversation = listed(&marc, id).await["lastConversationId"].clone();
    let detail = marc
        .get(&format!(
            "/conversations/{}",
            conversation.as_str().unwrap()
        ))
        .await
        .json();
    assert_eq!(detail["messages"][0]["content"], "Relève les retards.");

    // Moved to another time: it runs next then, not now.
    due(id, Duration::minutes(1)).await;
    let moved = marc.patch(&url, json!({ "time": "09:15" })).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    assert_eq!(fire(&api).await, 0);
    let next: DateTime<Utc> = listed(&marc, id).await["nextRunAt"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(next > Utc::now());

    let deleted = marc.delete(&url).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pass_goes_on_while_tasks_remain_due() {
    let api = TestApi::start().await;
    // More than a batch, all due two hours ago: none runs, each is
    // recorded missed in the one pass.
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    sqlx::query(
        "insert into scheduled_tasks \
         (tenant_id, user_id, title, instruction, weekdays, time_of_day, time_zone, next_run_at) \
         select $1, $2, 'Lot ' || n, 'Demande', '{1,2,3,4,5,6,7}', '08:00', 'Europe/Paris', \
         now() - interval '2 hours' from generate_series(1, 120) n",
    )
    .bind(tenant::ID)
    .bind(user::NADIA)
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(fire(&api).await, 0);
    let (missed, due): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where last_outcome = 'missed'), \
         count(*) filter (where next_run_at <= now()) \
         from scheduled_tasks where user_id = $1",
    )
    .bind(user::NADIA)
    .fetch_one(&owner)
    .await
    .unwrap();
    sqlx::query("delete from scheduled_tasks where user_id = $1")
        .bind(user::NADIA)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert_eq!((missed, due), (120, 0));
}
