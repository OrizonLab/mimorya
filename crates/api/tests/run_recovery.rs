//! Runs a stop of the service left running are taken up at startup (ADR-053).
//! A run goes on from its last saved step when only reading was under way
//! since, its lost step's validation requests cancelled and audited; one
//! that was running an action that may have changed something (a write, a
//! draft) ends, saying which, without the model being called again. So does
//! a run stopped before its first saved step, or whose user may no longer
//! act. Every end is saved and announced. Who takes runs up, when, and in
//! which company: `run_recovery_guards.rs`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::runtime::ACCOUNT_INACTIVE;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::runs::STOPPED_BEFORE_ITS_FIRST_STEP;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::approvals::{NewApproval, create_approval, get_approval};
use mimorya_db::repositories::runs::get_run;
use serde_json::{Value, json};
use support::TestApi;
use support::recovery::{
    conversation_of, events, last_answer, restart, saved, status_and_error, stopped_run,
};
use support::refusals::{entries_since, latest_entry};

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
    let since = latest_entry(&api).await;

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
    let cancelled = get_approval(&mut tx, approval.id).await.unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("approval.cancelled"), "{audited}");
    assert!(audited.contains(&approval.id.to_string()), "{audited}");
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
    // A draft too changes something (a planning proposal).
    let drafted = stopped_run(
        &api,
        tenant::ID,
        user::MARC,
        None,
        Some(saved()),
        &["planning_propose"],
    )
    .await;
    let calls_before = api.model.as_ref().unwrap().requests().len();

    assert_eq!(restart(&api).await, 2);

    let (status, error) = status_and_error(&api, tenant::ID, run_id).await;
    assert_eq!(status, "failed");
    let error = error.unwrap();
    assert!(
        error.contains("l’action « Application d’un planning » : son effet n’est pas connu"),
        "{error}"
    );
    let (drafted_status, drafted_error) = status_and_error(&api, tenant::ID, drafted).await;
    assert_eq!(drafted_status, "failed");
    assert!(
        drafted_error
            .unwrap()
            .contains("« Proposition de planning »")
    );
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
