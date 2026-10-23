//! Scheduled tasks fired when due (ADR-080): the instruction is sent as its
//! author's, in a new conversation of theirs, once per occurrence even when
//! two passes race, and the task runs next at its next time. An occurrence
//! does not run while the last one's run is under way, more than an hour
//! late, for an account that cannot act or has lost the agent, nor without a
//! model; why is recorded for its author, and audited.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::scheduled_tasks::fire_task;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::manager_approvals::without;
use support::scheduled::{INSTRUCTION, due, every_day, fire, listed, schedule};
use support::{Client, TestApi, owner_url};
use uuid::Uuid;

/// How many conversations `client` has.
async fn conversations(client: &Client<'_>) -> usize {
    let list = client.get("/conversations").await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    list.json()["conversations"].as_array().unwrap().len()
}

/// What the last occurrence of the task `id` became, and why.
async fn last(client: &Client<'_>, id: Uuid) -> (Value, Value) {
    let occurrence = listed(client, id).await["lastOccurrence"].clone();
    (occurrence["outcome"].clone(), occurrence["problem"].clone())
}

async fn audited(api: &TestApi, id: Uuid) -> Vec<(String, String, Option<Uuid>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, actor_type, on_behalf_of_user_id, metadata from audit_log \
         where resource_id = $1::text \
         and action in ('scheduled_task.fired', 'scheduled_task.not_run') order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
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
    let before = conversations(&marc).await;
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
    let occurrence = &task["lastOccurrence"];
    assert_eq!(occurrence["outcome"], "started");
    assert_eq!(occurrence["problem"], Value::Null);
    let next: DateTime<Utc> = task["nextRunAt"].as_str().unwrap().parse().unwrap();
    assert!(next > Utc::now());
    assert_eq!(conversations(&marc).await, before + 1);
    let conversation = occurrence["conversationId"].as_str().unwrap();
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

    let audit = audited(&api, id).await;
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(
        (audit[0].0.as_str(), audit[0].1.as_str(), audit[0].2),
        ("scheduled_task.fired", "system", Some(user::MARC))
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_occurrence_waits_for_none_and_runs_late_by_an_hour_at_most() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Synthèse")).await;
    api.script(vec![ScriptedStep::text("Synthèse faite.")]);
    due(id, Duration::minutes(59)).await;
    assert_eq!(fire(&api).await, 1);
    let first_run = listed(&marc, id).await["lastOccurrence"]["conversationId"].clone();

    // Its run still awaiting a validation: the next occurrence does not run.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let awaiting = "update agent_runs set status = $2 \
         where id = (select last_run_id from scheduled_tasks where id = $1)";
    sqlx::query(awaiting)
        .bind(id)
        .bind("awaiting_approval")
        .execute(&owner)
        .await
        .unwrap();
    let conversations_before = conversations(&marc).await;
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let (outcome, problem) = last(&marc, id).await;
    assert_eq!(outcome, "still_running");
    assert!(problem.as_str().unwrap().contains("précédente occurrence"));
    assert_eq!(conversations(&marc).await, conversations_before);
    sqlx::query(awaiting)
        .bind(id)
        .bind("completed")
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;

    // More than an hour late: not caught up, and runs at its next time.
    due(id, Duration::minutes(61)).await;
    assert_eq!(fire(&api).await, 0);
    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"]["outcome"], "missed");
    assert!(
        task["nextRunAt"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap()
            > Utc::now()
    );
    assert_eq!(conversations(&marc).await, conversations_before);
    assert_ne!(first_run, Value::Null);

    let outcomes: Vec<Value> = audited(&api, id)
        .await
        .into_iter()
        .map(|a| a.3["outcome"].clone())
        .collect();
    assert_eq!(
        outcomes,
        [json!("started"), json!("still_running"), json!("missed")]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_runs_for_an_account_that_cannot_act_or_lost_the_agent() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = schedule(&lucas, every_day("Relevé")).await;
    let before = conversations(&lucas).await;

    // Disabled.
    let account = format!("/admin/users/{}", user::LUCAS);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let lucas = api.sign_in(user::LUCAS).await;
    let (outcome, problem) = last(&lucas, id).await;
    assert_eq!(outcome, "refused");
    assert!(problem.as_str().unwrap().contains("ne pouvait pas agir"));

    // Holding a temporary password, as after a reset.
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let temporary = "update users set must_change_password = $2 where id = $1";
    sqlx::query(temporary)
        .bind(user::LUCAS)
        .bind(true)
        .execute(&owner)
        .await
        .unwrap();
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    sqlx::query(temporary)
        .bind(user::LUCAS)
        .bind(false)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    assert_eq!(last(&lucas, id).await.0, "refused");

    // No longer allowed to use the agent: the conversation opened goes.
    let policy = without(&paul, user::LUCAS, "agent:use").await;
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let restored = paul.delete(&policy).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    let (outcome, problem) = last(&lucas, id).await;
    assert_eq!(outcome, "refused");
    assert!(problem.as_str().unwrap().contains("agent:use"), "{problem}");
    assert_eq!(conversations(&lucas).await, before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_model_the_occurrence_fails_and_leaves_no_conversation() {
    let api = TestApi::without_llm().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Sans modèle")).await;
    let before = conversations(&marc).await;
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let (outcome, problem) = last(&marc, id).await;
    assert_eq!(outcome, "failed");
    assert!(
        problem
            .as_str()
            .unwrap()
            .starts_with("Aucun modèle n’est disponible"),
        "{problem}"
    );
    assert_eq!(conversations(&marc).await, before);
    // It runs again at its next time.
    assert!(listed(&marc, id).await["nextRunAt"].is_string());
}
