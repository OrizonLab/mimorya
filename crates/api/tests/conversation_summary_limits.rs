//! When a conversation is not summarised (ADR-145): never for whom may no
//! longer use the agent, whose run stopped there (ADR-035), the model left
//! uncalled; not while a run waits for a validation, its end folding the
//! conversation instead; nor for a company without a model to call; nor
//! once the service stops (ADR-149).

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::runs::{ConversationSummaries, RunScope};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::TestApi;
use support::manager_approvals::listed;
use support::planning::{apply_proposal, propose};
use support::queue::status;
use support::summaries::{
    SUMMARY, SUMMARY_SYSTEM, audited_passes, conversation, disabling, exchanges, last_system,
    set_status, stored, turn,
};
use tokio::sync::watch;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_summarised_for_whom_may_no_longer_use_the_agent() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;

    // His account disabled while the model answers: the run stops there,
    // and the model is not called again, for the summary either.
    let steps = vec![disabling(user::MARC), ScriptedStep::text(SUMMARY)];
    let run = turn(&api, &marc, c, "Question 9", steps).await;
    set_status(user::MARC, "active").await;
    assert_eq!(status(run).await, "failed");
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
    assert!(!last_system(&api).starts_with(SUMMARY_SYSTEM));
    assert_eq!(stored(c).await, None);
    assert_eq!(audited_passes(c).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_awaiting_a_validation_is_summarised_once_it_ends() {
    let api = TestApi::start().await;
    let nadia = api.sign_in(user::NADIA).await;
    let c = conversation(&nadia).await;
    exchanges(c, 1, 8).await;

    // Paused with seventeen messages: nothing folded yet.
    let steps = vec![
        propose("INT-2049", "09:00"),
        apply_proposal(""),
        ScriptedStep::text("Le planning reste inchangé."),
        ScriptedStep::text(SUMMARY),
    ];
    let run = turn(&api, &nadia, c, "Planifie INT-2049", steps).await;
    assert_eq!(status(run).await, "awaiting_approval");
    assert_eq!(api.model.as_ref().unwrap().remaining(), 2);
    assert_eq!(stored(c).await, None);

    // Refused, the run ends with its answer: the conversation is folded.
    let approval = listed(&nadia, &run.to_string()).await.expect("her request");
    let decision = format!("/approvals/{}/decision", approval["id"].as_str().unwrap());
    let decided = nadia.post(&decision, json!({ "approved": false })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    api.idle().await;
    assert_eq!(status(run).await, "completed");
    assert!(last_system(&api).starts_with(SUMMARY_SYSTEM));
    assert_eq!(
        stored(c).await,
        Some((SUMMARY.into(), "Réponse 6".into(), 12))
    );
    assert_eq!(audited_passes(c).await, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_company_without_a_model_has_nothing_summarised() {
    let api = TestApi::without_llm().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 9).await;

    let services = &api.app.services;
    let summaries = ConversationSummaries::new(
        services.db.clone(),
        services.access.clone(),
        services.llm.clone(),
        services.background.clone(),
    );
    let scope = RunScope {
        tenant_id: tenant::ID,
        run_id: Uuid::new_v4(),
        user_id: user::MARC,
        conversation_id: Some(c),
        mission_id: None,
    };
    assert!(!summaries.fold(scope, c).await.unwrap());
    assert_eq!(stored(c).await, None);
    assert_eq!(audited_passes(c).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_ending_as_the_service_stops_is_followed_by_no_pass() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 8).await;

    // The service told to stop while the model answers: the run ends with
    // its answer, and the application chains no pass after it.
    let services = api.app.services.clone();
    let stopping = ScriptedStep::dynamic(move |_| {
        services.shutdown.send_replace(true);
        ScriptedReply::text("Réponse 9")
    });
    let steps = vec![stopping, ScriptedStep::text(SUMMARY)];
    let run = turn(&api, &marc, c, "Question 9", steps).await;
    assert_eq!(status(run).await, "completed");
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
    assert_eq!(stored(c).await, None);
    assert_eq!(audited_passes(c).await, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_pass_is_chained_once_the_service_stops() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let c = conversation(&marc).await;
    exchanges(c, 1, 9).await;

    // A run of another, short conversation, its cost recorded under it.
    let short = conversation(&marc).await;
    let reply = vec![ScriptedStep::text("Bonjour")];
    let run = turn(&api, &marc, short, "Bonjour", reply).await;

    let services = &api.app.services;
    let (stopping, stopped) = watch::channel(true);
    let summaries = ConversationSummaries::new(
        services.db.clone(),
        services.access.clone(),
        services.llm.clone(),
        services.background.clone(),
    )
    .stopping_on(stopped);
    let scope = RunScope {
        tenant_id: tenant::ID,
        run_id: run,
        user_id: user::MARC,
        conversation_id: Some(c),
        mission_id: None,
    };
    // Eighteen messages to fold, a summary scripted: none asked.
    api.script(vec![ScriptedStep::text(SUMMARY)]);
    assert_eq!(summaries.catch_up(scope, c).await.unwrap(), 0);
    assert_eq!(api.model.as_ref().unwrap().remaining(), 1);
    assert_eq!(stored(c).await, None);
    // Running again, the pass is made.
    stopping.send_replace(false);
    assert_eq!(summaries.catch_up(scope, c).await.unwrap(), 1);
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
    assert_eq!(audited_passes(c).await, 1);
}
