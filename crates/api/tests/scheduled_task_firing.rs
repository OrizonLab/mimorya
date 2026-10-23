//! Scheduled tasks fired when due (ADR-080): the instruction is sent as its
//! author's, in a new conversation of theirs, once per occurrence even when
//! two passes race, its run's start audited as the platform's on their
//! behalf, and the task runs next at its next time. A change that leaves
//! when a task runs keeps its occurrence due. The server's passes:
//! `scheduled_task_passes.rs`.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::scheduled_tasks::{Firing, fire_task};
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
    assert_eq!(late.unwrap(), Firing::Unclaimed);

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
