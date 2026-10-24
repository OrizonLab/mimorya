//! A mission's stage paused on a request its author decides, and what its
//! author is told of by e-mail (ADR-091).

use mimorya_agent::testing::{ScriptedStep, call};
use mimorya_api::mail::Mail;
use mimorya_api::services::missions::tell_ended_missions;
use serde_json::{Value, json};
use uuid::Uuid;

use super::missions::{entrust, read, stages};
use super::planning::{apply_proposal, propose};
use super::queue::take_queued;
use super::{Client, TestApi, owner_url, week};

/// Entrusts `client`'s mission `title`, its first stage asking to apply a
/// planning, and takes it from the queue: the mission, its stage and the
/// request it awaits.
pub async fn awaiting(api: &TestApi, client: &Client<'_>, title: &str) -> (Uuid, Uuid, Value) {
    let id = entrust(client, title).await;
    let mut search = week();
    search["status"] = json!(["in_progress"]);
    api.script(vec![
        ScriptedStep::calls("", vec![call("interventions_search", search)]),
        propose("INT-2049", "13:00"),
        apply_proposal("Je demande la validation."),
    ]);
    assert_eq!(take_queued(api).await, 1);
    api.idle().await;
    assert_eq!(read(client, id).await["status"], "awaiting_approval");
    let run = stages(id).await[0].0;
    let approvals = client.get("/approvals?status=pending").await.json();
    let approval = approvals["approvals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["runId"] == run.to_string())
        .cloned()
        .unwrap_or_else(|| panic!("a request of {run}: {approvals}"));
    (id, run, approval)
}

/// The messages `api` sent about the mission `title`.
pub fn told(api: &TestApi, title: &str) -> Vec<Mail> {
    let about = format!("« {title} »");
    api.mail
        .sent()
        .into_iter()
        .filter(|mail| mail.subject.contains(&about))
        .collect()
}

/// Tells the authors of the missions over, as a process that executes runs
/// does every minute.
pub async fn tell_ends(api: &TestApi) {
    let runs = &api.app.services.runs;
    runs.start_executing().unwrap();
    tell_ended_missions(&api.app.services).await.unwrap();
}

/// The end notices audited for the mission: outcome and metadata.
pub async fn end_notices(mission: Uuid) -> Vec<(String, Value)> {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let notices = sqlx::query_as(
        "select outcome, metadata from audit_log where action = 'mission.notified' \
         and resource_type = 'mission' and resource_id = $1 and actor_type = 'system' \
         order by id",
    )
    .bind(mission.to_string())
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    notices
}

/// Moves the mission's end `hours` back.
pub async fn ended_ago(mission: Uuid, hours: i64) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(
        "update missions set finished_at = now() - make_interval(hours => $2::int) \
         where id = $1",
    )
    .bind(mission)
    .bind(hours)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
