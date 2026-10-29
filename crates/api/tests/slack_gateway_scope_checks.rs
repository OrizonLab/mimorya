//! What a check of a Slack gateway's permissions with Slack cannot do
//! (ADR-132): another company's gateway, whose stored token the route would
//! read and send, answers as an absent one, nothing of it read nor changed;
//! a token that cannot be read, or one Slack says is another workspace's,
//! is said, and nothing is kept nor audited as checked; nor when the token
//! is replaced while Slack is asked. Only permissions written as Slack
//! names them are kept.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::gateways::bot_token_name;
use mimorya_connectors::testing::{BOT_TOKEN, FakeSlack};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::gateways::{GATEWAYS, declare_slack, gateway_view};
use support::locks::{hold_rows, wait_for_waiters};
use support::refusals::not_found_since;
use support::refusals::{entries_of_since, entries_since, latest_entry, latest_entry_of};
use support::slack_companies::other_company;
use support::sso::SSO_DEPLOYMENT;
use support::{Client, TestApi, TestResponse};
use uuid::Uuid;

/// Asks Slack again what the gateway `id`'s token is.
async fn check(admin: &Client<'_>, id: Uuid) -> TestResponse {
    admin
        .post(&format!("{GATEWAYS}/{id}/check"), json!({}))
        .await
}

/// When the gateway `id` of `tenant_id` was last checked and changed.
async fn stamps(api: &TestApi, tenant_id: Uuid, id: Uuid) -> (Option<String>, String) {
    let mut tx = api.app.services.db.begin(tenant_id).await.unwrap();
    sqlx::query_as("select scopes_checked_at::text, updated_at::text from gateways where id = $1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

/// Runs `sql` on the demo company's gateway `id`.
async fn alter(api: &TestApi, sql: &str, id: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_company_s_gateway_is_checked_as_an_absent_one() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    declare_slack(&paul).await;
    let (acme, theirs) = other_company(&api, "acme-slack-scopes").await;
    let before = stamps(&api, acme, theirs).await;
    let (since, acme_since) = (latest_entry(&api).await, latest_entry_of(&api, acme).await);

    // Its id, which Slack is given in its events' address: as none.
    let refused = check(&paul, theirs).await;
    let absent = check(&paul, Uuid::new_v4()).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    assert_eq!(refused.text(), absent.text());
    assert_eq!(stamps(&api, acme, theirs).await, before);
    // Traced in Paul's company only.
    let traced = not_found_since(&api, since).await;
    assert!(
        traced.iter().any(|(actor, refusal)| *actor == user::PAUL
            && refusal.resource == Some(("gateway".into(), theirs.to_string()))),
        "{traced:?}"
    );
    let theirs_audited = entries_of_since(&api, acme, acme_since).await;
    assert_eq!(theirs_audited, "[]", "{theirs_audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_cannot_be_checked_is_said_and_nothing_kept() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let before = stamps(&api, tenant::ID, id).await;
    let since = latest_entry(&api).await;

    // Its token no longer stored: said, to give it again.
    alter(
        &api,
        "delete from secrets where name = 'gateway.' || $1::text || '.bot_token'",
        id,
    )
    .await;
    let unread = check(&paul, id).await;
    assert_eq!(unread.status, StatusCode::BAD_REQUEST, "{}", unread.text());
    assert!(
        unread.error_message().starts_with(
            "Le jeton du bot de cette passerelle ne peut pas être lu : donnez-le à nouveau"
        ),
        "{}",
        unread.text()
    );
    let path = format!("{GATEWAYS}/{id}");
    let given = paul.patch(&path, json!({ "botToken": BOT_TOKEN })).await;
    assert_eq!(given.status, StatusCode::OK, "{}", given.text());
    let given_at = stamps(&api, tenant::ID, id).await;
    assert_ne!(given_at, before);

    // Slack saying it is another workspace's: said, nothing kept.
    alter(
        &api,
        "update gateways set workspace_id = 'T0AILLEURS' where id = $1",
        id,
    )
    .await;
    let elsewhere = check(&paul, id).await;
    assert_eq!(
        elsewhere.status,
        StatusCode::BAD_REQUEST,
        "{}",
        elsewhere.text()
    );
    assert!(
        elsewhere
            .error_message()
            .contains("autre espace de travail"),
        "{}",
        elsewhere.text()
    );
    assert_eq!(stamps(&api, tenant::ID, id).await, given_at);
    assert_eq!(gateway_view(&paul, id).await["missingScopes"], json!([]));
    let audited: Value = serde_json::from_str(&entries_since(&api, since).await).unwrap();
    let checked = (audited.as_array().unwrap().iter())
        .filter(|entry| entry["action"] == "gateway.checked")
        .count();
    assert_eq!(checked, 0, "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_replaced_while_slack_is_asked_keeps_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let before = stamps(&api, tenant::ID, id).await;
    let since = latest_entry(&api).await;

    // The check asks Slack, then waits on the gateway, whose token another
    // change replaces meanwhile.
    let hold = hold_rows(&api, "gateways", &[id]).await;
    let replace = async {
        wait_for_waiters(&api, 1).await;
        let mut hold = hold;
        let secrets = api.app.services.gateways.secrets();
        let name = bot_token_name(id);
        secrets
            .put(&mut hold, tenant::ID, &name, "xoxb-fictif-autre-jeton-0002")
            .await
            .unwrap();
        hold.commit().await.unwrap();
    };
    let (checked, ()) = tokio::join!(check(&paul, id), replace);
    assert_eq!(checked.status, StatusCode::CONFLICT, "{}", checked.text());
    assert_eq!(
        checked.error_message(),
        "Le jeton de la passerelle a changé pendant la vérification : vérifiez de nouveau"
    );
    assert_eq!(stamps(&api, tenant::ID, id).await, before);
    let audited = entries_since(&api, since).await;
    assert!(!audited.contains("gateway.checked"), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_permissions_written_as_slack_names_them_are_kept() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    for kept in ["{}", "{chat:write,users:read.email}", "{a}"] {
        assert!(keeps(&api, id, kept).await, "{kept}");
    }
    let long = format!("{{a{}}}", "b".repeat(80));
    for refused in [
        "{NULL}",
        "{\"chat:write,users:read\"}",
        "{chat:write,NULL}",
        "{chat:write,\"\"}",
        "{\"\"}",
        "{1chat:write}",
        "{:write}",
        "{Chat:write}",
        "{\"a b\"}",
        &long,
    ] {
        assert!(!keeps(&api, id, refused).await, "{refused}");
    }
}

/// Whether the gateway `id` could keep `scopes` (an array as PostgreSQL
/// writes it); nothing kept either way.
async fn keeps(api: &TestApi, id: Uuid, scopes: &str) -> bool {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let kept = sqlx::query("update gateways set granted_scopes = $2::text[] where id = $1")
        .bind(id)
        .bind(scopes)
        .execute(&mut *tx)
        .await;
    // Dropped: rolled back.
    drop(tx);
    kept.is_ok()
}
