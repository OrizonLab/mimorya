//! A person's inbound hooks (ADR-109): created, listed, changed, paused,
//! given a new token and deleted by their author only, audited without what
//! they ask; another's answer as absent. The token is shown once and only
//! its digest is kept. At most ten each; creating or resuming one needs the
//! right to use the agent, pausing or deleting one does not.

mod support;

use axum::http::StatusCode;
use mimorya_api::auth::credentials::hash_token;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::hooks::{HOOKS, INSTRUCTION, deliver, hook, listed};
use support::manager_approvals::without;
use support::refusals::{entries_since, latest_entry};
use uuid::Uuid;

fn hook_url(id: Uuid) -> String {
    format!("{HOOKS}/{id}")
}

/// The digest kept for the hook `id`.
async fn kept_digest(api: &TestApi, id: Uuid) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let digest = sqlx::query_scalar("select token_hash from inbound_hooks where id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    digest
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_writes_their_hooks_and_sees_each_token_once() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let since = latest_entry(&api).await;
    let made = marc
        .post(
            HOOKS,
            json!({ "title": "  Panne\n signalée\u{200B} ", "instruction": INSTRUCTION }),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.text());
    let made = made.json();
    let id: Uuid = made["id"].as_str().unwrap().parse().unwrap();
    let token = made["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("mhk_"), "{token}");
    assert_eq!(made["path"], format!("/hooks/{id}"));
    assert_eq!(kept_digest(&api, id).await, hash_token(&token));

    let shown = listed(&marc, id).await;
    assert_eq!(shown["title"], "Panne signalée");
    assert_eq!(shown["instruction"], INSTRUCTION);
    assert_eq!(shown["enabled"], true);
    assert_eq!(shown["path"], format!("/hooks/{id}"));
    assert_eq!(shown["lastDelivery"], Value::Null);
    let list = marc.get(HOOKS).await;
    assert!(!list.text().contains(&token), "the token is shown once");
    assert_eq!(list.json()["maxHooks"], 10);
    assert_eq!(list.json()["maxPerHour"], 30);
    assert_eq!(list.json()["maxBodyBytes"], 16384);

    let changed = marc
        .patch(&hook_url(id), json!({ "title": "Panne", "enabled": false }))
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let shown = listed(&marc, id).await;
    assert_eq!(
        (&shown["title"], &shown["enabled"]),
        (&json!("Panne"), &json!(false))
    );
    assert_eq!(shown["instruction"], INSTRUCTION);
    let refused = marc
        .patch(&hook_url(id), json!({ "instruction": " " }))
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);

    // A new token: the former no longer delivers.
    marc.patch(&hook_url(id), json!({ "enabled": true })).await;
    let renewed = marc
        .post(&format!("{}/token", hook_url(id)), json!({}))
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    let new_token = renewed.json()["token"].as_str().unwrap().to_owned();
    assert_ne!(new_token, token);
    assert_eq!(kept_digest(&api, id).await, hash_token(&new_token));
    assert_eq!(
        deliver(&api, id, &token, "{}").await.status,
        StatusCode::NOT_FOUND
    );

    assert_eq!(marc.delete(&hook_url(id)).await.status, StatusCode::OK);
    assert_eq!(listed(&marc, id).await, Value::Null);
    assert_eq!(
        marc.delete(&hook_url(id)).await.status,
        StatusCode::NOT_FOUND
    );
    let audit = entries_since(&api, since).await;
    for action in [
        "inbound_hook.created",
        "inbound_hook.updated",
        "inbound_hook.token_replaced",
        "inbound_hook.deleted",
    ] {
        assert!(audit.contains(action), "{action}: {audit}");
    }
    for secret in [INSTRUCTION, &token, &new_token] {
        assert!(!audit.contains(secret), "{audit}");
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nobody_reads_or_changes_another_s_hook() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let (id, _) = hook(&marc, "Panne de Marc").await;
    assert_eq!(listed(&lucas, id).await, Value::Null);
    let attempts = [
        lucas
            .patch(&hook_url(id), json!({ "enabled": false }))
            .await,
        lucas
            .post(&format!("{}/token", hook_url(id)), json!({}))
            .await,
        lucas.delete(&hook_url(id)).await,
    ];
    for attempt in attempts {
        assert_eq!(attempt.status, StatusCode::NOT_FOUND, "{}", attempt.text());
    }
    assert_eq!(listed(&marc, id).await["enabled"], true);
    marc.delete(&hook_url(id)).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ten_at_most_and_running_one_needs_the_agent() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let mut ids = Vec::new();
    for n in 1..=10 {
        ids.push(hook(&marc, &format!("Déclencheur {n}")).await.0);
    }
    let eleventh = marc
        .post(
            HOOKS,
            json!({ "title": "Onzième", "instruction": INSTRUCTION }),
        )
        .await;
    assert_eq!(eleventh.status, StatusCode::CONFLICT, "{}", eleventh.text());

    let policy = without(&paul, user::MARC, "agent:use").await;
    marc.delete(&hook_url(ids[9])).await;
    let created = marc
        .post(
            HOOKS,
            json!({ "title": "Après le retrait", "instruction": INSTRUCTION }),
        )
        .await;
    assert_eq!(created.status, StatusCode::FORBIDDEN, "{}", created.text());
    let paused = marc
        .patch(&hook_url(ids[0]), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let resumed = marc
        .patch(&hook_url(ids[0]), json!({ "enabled": true }))
        .await;
    assert_eq!(resumed.status, StatusCode::FORBIDDEN, "{}", resumed.text());
    let renewed = marc
        .post(&format!("{}/token", hook_url(ids[0])), json!({}))
        .await;
    assert_eq!(renewed.status, StatusCode::FORBIDDEN, "{}", renewed.text());
    for id in &ids[..9] {
        assert_eq!(marc.delete(&hook_url(*id)).await.status, StatusCode::OK);
    }
    let restored = paul.delete(&policy).await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.text());
}
