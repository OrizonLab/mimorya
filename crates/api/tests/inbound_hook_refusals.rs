//! Deliveries to an inbound hook that start nothing (ADR-109): beyond thirty
//! in an hour, kept and audited once until it ends; for an author who cannot
//! act (disabled, holding a temporary password, without the agent) or whose
//! company is suspended; without a model, or when the run cannot start. The
//! system that posted learns no more than the status; why is kept for the
//! author, and nothing is opened.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::hooks::{HOOKS, deliver, hook, listed};
use support::manager_approvals::without;
use support::privileges::without_insert;
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::scheduled::opened;
use support::{Client, TestApi};
use uuid::Uuid;

const REFUSED: &str = "Ce déclencheur ne peut rien lancer pour l’instant.";
const FAILED: &str = "Le démarrage a échoué : renvoyez les données plus tard.";

/// Starts the hourly window of the hook `id` at `started`, with
/// `deliveries` counted.
async fn window(api: &TestApi, id: Uuid, started: DateTime<Utc>, deliveries: i32) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update inbound_hooks set window_started_at = $2, window_deliveries = $3 where id = $1",
    )
    .bind(id)
    .bind(started)
    .bind(deliveries)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Why the last delivery to the hook `id` did not start, as its author
/// reads it, if it ended as `outcome`.
async fn kept(client: &Client<'_>, id: Uuid, outcome: &str) -> String {
    let shown = listed(client, id).await;
    assert_eq!(shown["lastDelivery"]["outcome"], outcome, "{shown}");
    shown["lastDelivery"]["problem"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Delivers to the hook `id`, refused because its author cannot act.
async fn refused(api: &TestApi, id: Uuid, token: &str) {
    let answer = deliver(api, id, token, "{}").await;
    assert_eq!(answer.status, StatusCode::CONFLICT, "{}", answer.text());
    assert_eq!(answer.error_message(), REFUSED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn thirty_deliveries_an_hour_then_none_until_it_ends() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let (id, token) = hook(&marc, "Panne").await;
    window(&api, id, Utc::now() - Duration::minutes(50), 30).await;
    let since = latest_entry(&api).await;
    let limited = deliver(&api, id, &token, "{}").await;
    assert_eq!(
        limited.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        limited.text()
    );
    let wait: i64 = limited.headers["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=600).contains(&wait), "{wait}");
    let first = listed(&marc, id).await["lastDelivery"].clone();
    assert_eq!(first["outcome"], "limited", "{first}");
    // Those that follow in the hour write nothing.
    let again = deliver(&api, id, &token, "{}").await;
    assert_eq!(again.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(listed(&marc, id).await["lastDelivery"], first);
    let audit = entries_since(&api, since).await;
    assert_eq!(audit.matches("inbound_hook.not_run").count(), 1, "{audit}");
    assert!(api.model.as_ref().unwrap().requests().is_empty());

    // The hour over, it takes deliveries again.
    window(&api, id, Utc::now() - Duration::minutes(61), 30).await;
    api.script(vec![ScriptedStep::text("Reçu.")]);
    let accepted = deliver(&api, id, &token, "{}").await;
    assert_eq!(accepted.status, StatusCode::ACCEPTED, "{}", accepted.text());
    api.idle().await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let counted: i32 =
        sqlx::query_scalar("select window_deliveries from inbound_hooks where id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(counted, 1);
    marc.delete(&format!("{HOOKS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_author_who_cannot_act_starts_nothing() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, token) = hook(&marc, "Panne").await;
    let before = opened(user::MARC).await;
    let since = latest_entry(&api).await;

    // Disabled.
    let account = format!("/admin/users/{}", user::MARC);
    let disabled = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    refused(&api, id, &token).await;
    let restored = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    let marc = api.sign_in(user::MARC).await;
    assert!(kept(&marc, id, "refused").await.contains("désactivé"));

    // Holding a temporary password, as after a reset.
    let temporary =
        |held: bool| format!("update users set must_change_password = {held} where id = $1");
    as_owner(&temporary(true), user::MARC).await;
    refused(&api, id, &token).await;
    as_owner(&temporary(false), user::MARC).await;
    let problem = kept(&marc, id, "refused").await;
    assert!(problem.contains("mot de passe provisoire"), "{problem}");

    // No longer allowed to use the agent.
    let policy = without(&paul, user::MARC, "agent:use").await;
    refused(&api, id, &token).await;
    let restored = paul.delete(&policy).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
    let problem = kept(&marc, id, "refused").await;
    assert!(problem.contains("agent:use"), "{problem}");

    // Their company suspended.
    as_owner(
        "update tenants set status = 'suspended' where id = $1",
        tenant::ID,
    )
    .await;
    api.app.services.access.invalidate(tenant::ID);
    refused(&api, id, &token).await;
    as_owner(
        "update tenants set status = 'active' where id = $1",
        tenant::ID,
    )
    .await;
    api.app.services.access.invalidate(tenant::ID);
    assert_eq!(
        kept(&marc, id, "refused").await,
        "Votre entreprise est suspendue : l’agent n’agit plus pour elle."
    );

    assert!(api.model.as_ref().unwrap().requests().is_empty());
    assert_eq!(opened(user::MARC).await, before);
    let audit = entries_since(&api, since).await;
    assert_eq!(audit.matches("inbound_hook.not_run").count(), 4, "{audit}");
    assert_eq!(listed(&marc, id).await["lastConversationId"], json!(null));
    marc.delete(&format!("{HOOKS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_model_nothing_starts() {
    let api = TestApi::without_llm().await;
    let marc = api.sign_in(user::MARC).await;
    let (id, token) = hook(&marc, "Sans modèle").await;
    let before = opened(user::MARC).await;
    let failed = deliver(&api, id, &token, "{}").await;
    assert_eq!(
        failed.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        failed.text()
    );
    assert_eq!(failed.error_message(), FAILED);
    let problem = kept(&marc, id, "failed").await;
    assert!(
        problem.starts_with("Aucun modèle n’est disponible"),
        "{problem}"
    );
    assert_eq!(opened(user::MARC).await, before);
    marc.delete(&format!("{HOOKS}/{id}")).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_that_cannot_start_leaves_no_conversation() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let (id, token) = hook(&marc, "Panne").await;
    let before = opened(user::MARC).await;
    // The run cannot be recorded, once its conversation is.
    let failed = without_insert("agent_runs", deliver(&api, id, &token, "{}")).await;
    assert_eq!(
        failed.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        failed.text()
    );
    assert_eq!(failed.error_message(), FAILED);
    assert_eq!(
        kept(&marc, id, "failed").await,
        "Le démarrage a échoué : les données envoyées n’ont pas été traitées."
    );
    assert_eq!(opened(user::MARC).await, before);
    assert!(api.model.as_ref().unwrap().requests().is_empty());
    marc.delete(&format!("{HOOKS}/{id}")).await;
}
