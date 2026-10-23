//! Scheduled tasks (ADR-080): written as their author would, made due as
//! time would make them, fired as the server does every minute.

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_api::services::scheduled_tasks::fire_due_tasks;
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
