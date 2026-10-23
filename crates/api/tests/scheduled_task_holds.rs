//! Occurrences of a scheduled task that do not run (ADR-080): while the last
//! run one started is under way, however many come meanwhile; more than an
//! hour late; for an account that cannot act (a disabled one's tasks pause)
//! or has lost the agent; for a suspended company; without a model. None
//! opens a conversation; why is recorded for its author, and audited.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::manager_approvals::without;
use support::scheduled::{
    due, every_day, fire, last, listed, occurrences_audited, opened, schedule, set_last_run,
};
use support::{TestApi, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_occurrence_waits_for_the_last_run_started_and_an_hour_late_at_most() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Synthèse")).await;
    api.script(vec![ScriptedStep::text("Synthèse faite.")]);
    due(id, Duration::minutes(59)).await;
    assert_eq!(fire(&api).await, 1);
    let first = listed(&marc, id).await["lastConversationId"].clone();
    assert!(first.is_string());

    // Its run awaiting a validation: no occurrence runs, day after day, and
    // the conversation awaiting stays at hand.
    set_last_run(id, "awaiting_approval").await;
    let before = opened(user::MARC).await;
    for _ in 0..2 {
        due(id, Duration::minutes(1)).await;
        assert_eq!(fire(&api).await, 0);
        let task = listed(&marc, id).await;
        assert_eq!(task["lastOccurrence"]["outcome"], "still_running");
        let problem = task["lastOccurrence"]["problem"].as_str().unwrap();
        assert!(problem.contains("précédente occurrence"), "{problem}");
        assert_eq!(task["lastConversationId"], first);
    }
    assert_eq!(opened(user::MARC).await, before);
    set_last_run(id, "completed").await;

    // More than an hour late: not caught up, and runs at its next time.
    due(id, Duration::minutes(61)).await;
    assert_eq!(fire(&api).await, 0);
    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"]["outcome"], "missed");
    let next: DateTime<Utc> = task["nextRunAt"].as_str().unwrap().parse().unwrap();
    assert!(next > Utc::now());
    assert_eq!(opened(user::MARC).await, before);

    // Its run over, the next one runs.
    api.script(vec![ScriptedStep::text("Nouvelle synthèse.")]);
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 1);
    let task = listed(&marc, id).await;
    assert_eq!(task["lastOccurrence"]["outcome"], "started");
    assert_ne!(task["lastConversationId"], first);

    let outcomes: Vec<Value> = occurrences_audited(&api, id)
        .await
        .into_iter()
        .map(|a| a.3["outcome"].clone())
        .collect();
    assert_eq!(
        outcomes,
        [
            "started",
            "still_running",
            "still_running",
            "missed",
            "started"
        ]
        .map(|o| json!(o))
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_runs_for_an_account_that_cannot_act_or_lost_the_agent() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let id = schedule(&lucas, every_day("Relevé")).await;
    let before = opened(user::LUCAS).await;

    // Disabled: refused, and paused until its author resumes it.
    let account = format!("/admin/users/{}", user::LUCAS);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let enabled = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let lucas = api.sign_in(user::LUCAS).await;
    let task = listed(&lucas, id).await;
    assert_eq!(task["lastOccurrence"]["outcome"], "refused");
    let problem = task["lastOccurrence"]["problem"].as_str().unwrap();
    assert!(problem.contains("désactivé"), "{problem}");
    assert_eq!(
        (task["enabled"].clone(), task["nextRunAt"].clone()),
        (json!(false), Value::Null)
    );
    let url = format!("/scheduled-tasks/{id}");
    let resumed = lucas.patch(&url, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK, "{}", resumed.text());

    // Holding a temporary password, as after a reset: refused, not paused.
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
    let (outcome, problem) = last(&lucas, id).await;
    assert_eq!(outcome, "refused");
    assert!(
        problem
            .as_str()
            .unwrap()
            .contains("mot de passe provisoire"),
        "{problem}"
    );
    assert_eq!(listed(&lucas, id).await["enabled"], true);

    // No longer allowed to use the agent: refused before anything opens.
    let policy = without(&paul, user::LUCAS, "agent:use").await;
    due(id, Duration::minutes(1)).await;
    assert_eq!(fire(&api).await, 0);
    let restored = paul.delete(&policy).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    let (outcome, problem) = last(&lucas, id).await;
    assert_eq!(outcome, "refused");
    assert!(problem.as_str().unwrap().contains("agent:use"), "{problem}");
    assert_eq!(opened(user::LUCAS).await, before);
    assert_eq!(lucas.delete(&url).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_runs_for_a_suspended_company() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Pendant la suspension")).await;
    let before = opened(user::MARC).await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let set_status = |status: &'static str| {
        sqlx::query("update tenants set status = $2 where id = $1")
            .bind(tenant::ID)
            .bind(status)
            .execute(&owner)
    };
    set_status("suspended").await.unwrap();
    // As when the company's cached rules expire.
    api.app.services.access.invalidate(tenant::ID);
    due(id, Duration::minutes(1)).await;
    let fired = fire(&api).await;
    set_status("active").await.unwrap();
    api.app.services.access.invalidate(tenant::ID);
    owner.close().await;

    assert_eq!(fired, 0);
    let (outcome, problem) = last(&marc, id).await;
    assert_eq!(outcome, "refused");
    assert_eq!(
        problem,
        "Votre entreprise est suspendue : l’agent n’agit plus pour elle."
    );
    assert_eq!(opened(user::MARC).await, before);
    assert_eq!(
        marc.delete(&format!("/scheduled-tasks/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_model_the_occurrence_fails_and_leaves_no_conversation() {
    let api = TestApi::without_llm().await;
    let marc = api.sign_in(user::MARC).await;
    let id = schedule(&marc, every_day("Sans modèle")).await;
    let before = opened(user::MARC).await;
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
    assert_eq!(opened(user::MARC).await, before);
    // It runs again at its next time.
    assert!(listed(&marc, id).await["nextRunAt"].is_string());
}
