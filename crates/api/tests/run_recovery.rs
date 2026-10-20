//! Runs a stop of the service left running are taken up at startup, before
//! the API serves (ADR-053), each in its own company. A run goes on from its
//! last saved step when only reading was under way since, its lost step's
//! validation requests cancelled; one that was running an action that may
//! have changed something ends, saying which, without the model being called
//! again. So does a run stopped before its first saved step, or whose user
//! may no longer act. Every end is saved and announced.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::runtime::{ACCOUNT_INACTIVE, DEFAULT_BUDGET, RunCheckpoint};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::STOPPED_BEFORE_ITS_FIRST_STEP;
use mimorya_core::autonomy::AutonomyLevel;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approvals::{NewApproval, create_approval, get_approval};
use mimorya_db::repositories::run_events::{append_run_event, list_run_events};
use mimorya_db::repositories::runs::{NewRun, RunPatch, create_run, get_run, update_run};
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use serde_json::{Value, json};
use support::{Client, TestApi};
use uuid::Uuid;

/// A run of `user` in a new conversation of theirs (`conversation`), left
/// running by a stop of the service at `state`, after it started `started`.
async fn stopped_run(
    api: &TestApi,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation: Option<Uuid>,
    state: Option<RunCheckpoint>,
    started: &[&str],
) -> Uuid {
    let db = &api.app.services.db;
    let mut tx = db.begin(tenant_id).await.unwrap();
    let run = create_run(
        &mut tx,
        NewRun {
            tenant_id,
            conversation_id: conversation,
            initiated_by_user_id: user_id,
            on_behalf_of_user_id: user_id,
            objective: "Fais le point sur la semaine".into(),
            success_criteria: Vec::new(),
            autonomy_level: AutonomyLevel::ExecuteWithApproval,
            budget: serde_json::to_value(DEFAULT_BUDGET).unwrap(),
        },
    )
    .await
    .unwrap();
    if let Some(state) = state {
        let patch = RunPatch {
            state: Some(serde_json::to_value(state).unwrap()),
            ..RunPatch::default()
        };
        update_run(&mut tx, run.id, patch).await.unwrap();
    }
    tx.commit().await.unwrap();
    // Calls started after the step was saved.
    for (n, tool) in started.iter().enumerate() {
        let mut tx = db.begin(tenant_id).await.unwrap();
        let call = json!({ "toolCallId": format!("c{n}"), "toolName": tool, "title": tool, "decision": "allow" });
        append_run_event(&mut tx, tenant_id, run.id, "tool_call", &call)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    run.id
}

fn saved() -> RunCheckpoint {
    RunCheckpoint::new(
        "CONSIGNES ENREGISTRÉES".into(),
        vec![LlmMessage::user("Fais le point sur la semaine")],
    )
}

async fn conversation_of(client: &Client<'_>) -> Uuid {
    let created = client.post("/conversations", json!({})).await.json();
    created["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Takes up the stopped runs, as at startup, and waits for them.
async fn restart(api: &TestApi) -> usize {
    let taken = api.app.recover_interrupted_runs().await.unwrap();
    api.idle().await;
    taken
}

async fn status_and_error(
    api: &TestApi,
    tenant_id: Uuid,
    run_id: Uuid,
) -> (String, Option<String>) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    let run = get_run(&mut tx, run_id).await.unwrap().unwrap();
    (run.status, run.error)
}

async fn events(api: &TestApi, tenant_id: Uuid, run_id: Uuid) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    list_run_events(&mut tx, run_id, 0)
        .await
        .unwrap()
        .into_iter()
        .map(|e| (e.event_type, e.data))
        .collect()
}

async fn last_answer(client: &Client<'_>, conversation: Uuid) -> String {
    let thread = client
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    let messages = thread["messages"].as_array().unwrap();
    messages.last().unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_goes_on_from_its_last_step_when_only_reading_was_under_way() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let conversation = conversation_of(&marc).await;
    let run_id = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        Some(conversation),
        Some(saved()),
        &["interventions_search"],
    )
    .await;
    // A validation its lost step had asked for.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let approval = create_approval(
        &mut tx,
        NewApproval {
            tenant_id: tenant::ID,
            run_id,
            requested_for_user_id: user::MARC,
            tool_name: "planning_apply".into(),
            tool_call_id: "c9".into(),
            action_summary: "Appliquer le planning".into(),
            payload: json!({}),
            risk: "write".into(),
            reason: "Validation requise".into(),
            expires_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    api.script(vec![ScriptedStep::text("Semaine calme : rien à signaler.")]);

    assert_eq!(restart(&api).await, 1);

    assert_eq!(
        status_and_error(&api, tenant::ID, run_id).await.0,
        "completed"
    );
    assert_eq!(
        last_answer(&marc, conversation).await,
        "Semaine calme : rien à signaler."
    );
    // From the step saved, its instructions included.
    let requests = api.model.as_ref().unwrap().requests();
    assert_eq!(requests.last().unwrap().system, "CONSIGNES ENREGISTRÉES");
    let kinds: Vec<(String, Value)> = events(&api, tenant::ID, run_id).await;
    assert!(kinds.contains(&(
        "run_resumed".into(),
        json!({ "interrupted": ["interventions_search"] })
    )));
    assert_eq!(kinds.last().unwrap().1["status"], "completed");
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let approval = get_approval(&mut tx, approval.id).await.unwrap().unwrap();
    assert_eq!(approval.status, "cancelled");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_action_that_may_have_changed_something_is_never_repeated() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let conversation = conversation_of(&marc).await;
    let run_id = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        Some(conversation),
        Some(saved()),
        &["interventions_search", "planning_apply"],
    )
    .await;
    let calls_before = api.model.as_ref().unwrap().requests().len();

    restart(&api).await;

    let (status, error) = status_and_error(&api, tenant::ID, run_id).await;
    assert_eq!(status, "failed");
    let error = error.unwrap();
    assert!(error.contains("« Application d’un planning »"), "{error}");
    assert!(error.contains("son effet n’est pas connu"), "{error}");
    assert_eq!(api.model.as_ref().unwrap().requests().len(), calls_before);
    assert!(last_answer(&marc, conversation).await.contains(&error));
    let ended = events(&api, tenant::ID, run_id).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
    assert_eq!(ended.last().unwrap().1["status"], "failed");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_stopped_before_its_first_step_or_whose_user_left_ends() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let conversation = conversation_of(&marc).await;
    let unsaved = stopped_run(&api, tenant::ID, user::MARC, Some(conversation), None, &[]).await;
    let other = conversation_of(&marc).await;
    let left = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        Some(other),
        Some(saved()),
        &[],
    )
    .await;
    let account = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());

    assert_eq!(restart(&api).await, 2);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());

    for (run_id, expected) in [
        (unsaved, STOPPED_BEFORE_ITS_FIRST_STEP),
        (left, ACCOUNT_INACTIVE),
    ] {
        let (status, error) = status_and_error(&api, tenant::ID, run_id).await;
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some(expected));
        let ended = events(&api, tenant::ID, run_id).await;
        assert_eq!(ended.last().unwrap().1["error"], expected);
    }
    // The saved step is kept.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let run = get_run(&mut tx, left).await.unwrap().unwrap();
    assert_eq!(run.state.unwrap()["system"], "CONSIGNES ENREGISTRÉES");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_company_s_runs_are_taken_up_in_their_own_company_and_only_running_ones() {
    let api = TestApi::start().await;
    let foreign = stopped_run(&api, OTHER_TENANT_ID, OTHER_USER_ID, None, None, &[]).await;
    let paused = stopped_run(&api, tenant::ID, user::MARC, None, Some(saved()), &[]).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'awaiting_approval' where id = $1")
        .bind(paused)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(restart(&api).await, 1);

    let (status, error) = status_and_error(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(status, "failed");
    assert_eq!(error.as_deref(), Some(STOPPED_BEFORE_ITS_FIRST_STEP));
    let ended = events(&api, OTHER_TENANT_ID, foreign).await;
    assert_eq!(ended.last().unwrap().0, "run_finished");
    // Its end is nowhere in the demo company.
    assert!(events(&api, tenant::ID, foreign).await.is_empty());
    assert_eq!(
        status_and_error(&api, tenant::ID, paused).await.0,
        "awaiting_approval"
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update agent_runs set status = 'failed' where id = $1")
        .bind(paused)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}
