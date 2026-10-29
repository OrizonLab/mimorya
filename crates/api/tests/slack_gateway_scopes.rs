//! The permissions of the company's Slack application (ADR-132), as Slack
//! says them with its token (`x-oauth-scopes`): a token lacking one the
//! application asks for is refused when it is given, naming what is
//! missing; one Slack says nothing of is taken, its permissions unknown. A
//! gateway is checked again on demand with its stored token: what is
//! missing, and what is more than asked for, kept and shown, audited, its
//! token never. Reserved to the connectors' administrators.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{BOT_TOKEN, FakeSlack, SIGNING_SECRET};
use mimorya_core::demo::{tenant, user};
use mimorya_core::gateways::slack::BOT_SCOPES;
use serde_json::{Value, json};
use support::gateways::{DECLARE, GATEWAYS, declare_slack, gateway_view, remove_gateways};
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::{Client, TestApi, TestResponse};
use uuid::Uuid;

const WITHOUT_IM_READ: [&str; 4] = ["chat:write", "im:history", "users:read", "users:read.email"];

/// The audit entries of `action` since `since`, oldest first.
async fn audited(api: &TestApi, since: i64, action: &str) -> Vec<Value> {
    let entries: Value = serde_json::from_str(&entries_since(api, since).await).unwrap();
    let mut found: Vec<Value> = (entries.as_array().unwrap().iter())
        .filter(|entry| entry["action"] == action)
        .cloned()
        .collect();
    found.reverse();
    found
}

/// Asks Slack again what the gateway `id`'s token is.
async fn check(admin: &Client<'_>, id: Uuid) -> TestResponse {
    admin
        .post(&format!("{GATEWAYS}/{id}/check"), json!({}))
        .await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_lacking_a_permission_is_refused_when_given() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    remove_gateways(&paul).await;
    let since = latest_entry(&api).await;
    let declare =
        json!({ "name": "Slack", "botToken": BOT_TOKEN, "signingSecret": SIGNING_SECRET });

    // Without im:read: refused, nothing kept.
    slack.grant_scopes(Some(&WITHOUT_IM_READ));
    let refused = paul.post(DECLARE, declare.clone()).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert_eq!(
        refused.error_message(),
        "Il manque à l’application Slack la permission im:read : ajoutez-la dans « OAuth & \
         Permissions », réinstallez l’application, puis donnez son jeton du bot"
    );
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));

    // Slack saying none: taken, its permissions unknown.
    slack.grant_scopes(None);
    let declared = paul.post(DECLARE, declare).await;
    assert_eq!(declared.status, StatusCode::CREATED, "{}", declared.text());
    let id: Uuid = declared.json()["id"].as_str().unwrap().parse().unwrap();
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["missingScopes"], Value::Null);
    assert_eq!(view["extraScopes"], Value::Null);
    assert!(view["scopesCheckedAt"].is_string(), "{view}");

    // A new token lacking two: refused, both named, nothing changed.
    let path = format!("{GATEWAYS}/{id}");
    slack.grant_scopes(Some(&["chat:write", "im:history", "users:read.email"]));
    let refused = paul.patch(&path, json!({ "botToken": BOT_TOKEN })).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("Il manque à l’application Slack les permissions im:read, users:read :"),
        "{}",
        refused.text()
    );
    assert_eq!(gateway_view(&paul, id).await["missingScopes"], Value::Null);
    // One with all and more: taken, the more shown.
    let mut more = BOT_SCOPES.to_vec();
    more.push("channels:history");
    slack.grant_scopes(Some(&more));
    let renewed = paul.patch(&path, json!({ "botToken": BOT_TOKEN })).await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["missingScopes"], json!([]));
    assert_eq!(view["extraScopes"], json!(["channels:history"]));

    // Audited: what Slack said of the permissions, never the token.
    let created = audited(&api, since, "gateway.created").await;
    assert_eq!(created[0]["metadata"]["scopes"], json!({ "said": false }));
    let updated = audited(&api, since, "gateway.updated").await;
    assert_eq!(
        updated[0]["metadata"]["scopes"],
        json!({ "said": true, "missing": [], "extra": ["channels:history"] })
    );
    let all = entries_since(&api, since).await;
    assert!(!all.contains(BOT_TOKEN), "{all}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_gateway_is_checked_again_with_its_stored_token() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    // As one declared before its permissions were read.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update gateways set granted_scopes = null, scopes_checked_at = null")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["missingScopes"], Value::Null);
    assert_eq!(view["scopesCheckedAt"], Value::Null);
    let changed_at = view["updatedAt"].clone();
    let since = latest_entry(&api).await;

    // Its application reinstalled without im:read: told, and kept.
    slack.grant_scopes(Some(&WITHOUT_IM_READ));
    let checked = check(&paul, id).await;
    assert_eq!(checked.status, StatusCode::OK, "{}", checked.text());
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["missingScopes"], json!(["im:read"]));
    assert_eq!(view["extraScopes"], json!([]));
    assert!(view["scopesCheckedAt"].is_string(), "{view}");
    // A check is no change of the gateway.
    assert_eq!(view["updatedAt"], changed_at);
    let entries = audited(&api, since, "gateway.checked").await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["resource_id"], id.to_string());
    assert_eq!(
        entries[0]["metadata"]["scopes"],
        json!({ "said": true, "missing": ["im:read"], "extra": [] })
    );
    // Given it back: nothing missing.
    slack.grant_scopes(Some(&BOT_SCOPES));
    assert_eq!(check(&paul, id).await.status, StatusCode::OK);
    assert_eq!(gateway_view(&paul, id).await["missingScopes"], json!([]));

    // Slack refusing the token: said, nothing changed.
    slack.revoke();
    let refused = check(&paul, id).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("Slack refuse le jeton du bot"),
        "{}",
        refused.text()
    );
    assert_eq!(gateway_view(&paul, id).await["missingScopes"], json!([]));
    assert_eq!(audited(&api, since, "gateway.checked").await.len(), 2);
    let all = entries_since(&api, since).await;
    assert!(!all.contains(BOT_TOKEN), "{all}");

    // None: as absent (another company's: `slack_gateway_scope_checks.rs`).
    let unknown = check(&paul, Uuid::new_v4()).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND, "{}", unknown.text());
    assert_eq!(unknown.error_message(), "Passerelle introuvable");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_connectors_administrators_check_a_gateway() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let before = gateway_view(&paul, id).await["scopesCheckedAt"].clone();
    slack.grant_scopes(Some(&WITHOUT_IM_READ));
    let lea = api.sign_in(user::LEA).await;
    assert_eq!(check(&lea, id).await.status, StatusCode::FORBIDDEN);
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["missingScopes"], json!([]));
    assert_eq!(view["scopesCheckedAt"], before);
}
