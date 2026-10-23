//! Scheduled tasks (ADR-080): written as their author would, made due as
//! time would make them, fired as the server does every minute.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_api::services::scheduled_tasks::fire_due_tasks;
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi, owner_url};

pub const TASKS: &str = "/scheduled-tasks";
pub const INSTRUCTION: &str = "Fais le point sur les interventions de la semaine.";

/// A task every day at 08:00 in Paris, so titled.
pub fn every_day(title: &str) -> Value {
    json!({
        "title": title,
        "instruction": INSTRUCTION,
        "weekdays": [1, 2, 3, 4, 5, 6, 7],
        "time": "08:00",
        "timeZone": "Europe/Paris",
    })
}

/// Has `client` create a task; its identifier.
pub async fn schedule(client: &Client<'_>, body: Value) -> Uuid {
    let created = client.post(TASKS, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().parse().unwrap()
}

/// The task `id` as its author lists it.
pub async fn listed(client: &Client<'_>, id: Uuid) -> Value {
    let list = client.get(TASKS).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    let tasks = list.json()["tasks"].as_array().unwrap().clone();
    tasks
        .into_iter()
        .find(|t| t["id"] == json!(id))
        .unwrap_or(Value::Null)
}

/// Makes the task `id` due `late` ago, as the clock would.
pub async fn due(id: Uuid, late: Duration) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update scheduled_tasks set next_run_at = $2 where id = $1")
        .bind(id)
        .bind(Utc::now() - late)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

/// Fires the tasks due, as the server does every minute, and waits for the
/// runs started; how many there were.
pub async fn fire(api: &TestApi) -> usize {
    let started = fire_due_tasks(&api.app.services, Utc::now()).await.unwrap();
    api.idle().await;
    started
}

/// What the last occurrence of the task `id` became, and why.
pub async fn last(client: &Client<'_>, id: Uuid) -> (Value, Value) {
    let occurrence = listed(client, id).await["lastOccurrence"].clone();
    (occurrence["outcome"].clone(), occurrence["problem"].clone())
}

/// How many conversations `user_id` has, archived ones included: what an
/// occurrence opened, whatever became of it.
pub async fn opened(user_id: Uuid) -> i64 {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let count = sqlx::query_scalar("select count(*) from conversations where owner_user_id = $1")
        .bind(user_id)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    count
}

/// Sets the status of the last run the task `id` started, which it has.
pub async fn set_last_run(id: Uuid, status: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let changed = sqlx::query(
        "update agent_runs set status = $2 \
         where id = (select last_run_id from scheduled_tasks where id = $1)",
    )
    .bind(id)
    .bind(status)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    assert_eq!(changed.rows_affected(), 1, "the task started no run");
}

/// What the occurrences of the task `id` were audited as: action, actor,
/// on whose behalf, metadata.
pub async fn occurrences_audited(
    api: &TestApi,
    id: Uuid,
) -> Vec<(String, String, Option<Uuid>, Value)> {
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
