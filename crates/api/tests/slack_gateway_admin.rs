//! The company's Slack gateway, declared by its connector administrators
//! (ADR-125): its bot's token checked with Slack before anything is kept,
//! the workspace and bot user Slack names kept, its token and signing
//! secret sealed, never returned nor audited; one per company; the address
//! to give Slack shown; its token replaced only by one of the same
//! workspace; deleted with its people's identities. Reserved to the
//! connectors' administrators.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{
    BOT_TOKEN, BOT_USER, FakeSlack, OTHER_TOKEN, SIGNING_SECRET, WORKSPACE, WORKSPACE_NAME,
};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::gateways::{DECLARE, GATEWAYS, declare_slack, gateway_view, remove_gateways};
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, WEB_ORIGIN};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_slack_application_once_checked_with_slack() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    remove_gateways(&paul).await;
    let since = latest_entry(&api).await;

    // Written as Slack issues them, or refused before Slack is asked.
    for (token, secret, said) in [
        ("xoxp-1-2-personne", SIGNING_SECRET, "xoxb-"),
        (BOT_TOKEN, "court", "Secret de signature"),
    ] {
        let refused = paul
            .post(
                DECLARE,
                json!({ "name": "Slack", "botToken": token, "signingSecret": secret }),
            )
            .await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(refused.error_message().contains(said), "{}", refused.text());
    }
    // A token Slack refuses: nothing is kept.
    let unknown = "xoxb-fictif-jeton-inconnu-de-slack";
    let refused = paul
        .post(
            DECLARE,
            json!({ "name": "Slack", "botToken": unknown, "signingSecret": SIGNING_SECRET }),
        )
        .await;
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
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));
    // An application Slack gave too few permissions: said for the administrator.
    slack.refuse_with(Some("missing_scope"));
    let refused = paul
        .post(
            DECLARE,
            json!({ "name": "Slack", "botToken": BOT_TOKEN, "signingSecret": SIGNING_SECRET }),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused.error_message().contains("users:read.email"),
        "{}",
        refused.text()
    );
    slack.refuse_with(None);
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));

    let id = declare_slack(&paul).await;
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["type"], "slack");
    assert_eq!(view["name"], "Slack Néo");
    assert_eq!(view["enabled"], true);
    assert_eq!(view["workspaceId"], WORKSPACE);
    assert_eq!(view["workspaceName"], WORKSPACE_NAME);
    assert_eq!(view["botUserId"], BOT_USER);
    assert_eq!(view["secretsStored"], true);
    assert_eq!(view["identities"], 0);
    assert_eq!(
        view["eventsUrl"],
        format!("{WEB_ORIGIN}/api/gateways/slack/{id}")
    );
    let listed = paul.get(GATEWAYS).await;
    let listing = listed.text();
    assert!(!listing.contains(BOT_TOKEN) && !listing.contains(SIGNING_SECRET));
    let scopes = &listed.json()["slackScopes"];
    assert_eq!(
        scopes,
        &json!(["chat:write", "im:history", "users:read", "users:read.email"])
    );
    assert_eq!(listed.json()["slackEvents"], json!(["message.im"]));

    // One per company.
    let again = paul
        .post(
            DECLARE,
            json!({ "name": "Autre", "botToken": BOT_TOKEN, "signingSecret": SIGNING_SECRET }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());

    // Audited, its secrets never.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.created"), "{audited}");
    assert!(audited.contains(WORKSPACE), "{audited}");
    assert!(!audited.contains(BOT_TOKEN) && !audited.contains(SIGNING_SECRET));
    assert!(!audited.contains(unknown));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_gateway_changes_within_its_workspace_and_goes_with_its_identities() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let path = format!("{GATEWAYS}/{id}");
    let since = latest_entry(&api).await;

    let changed = paul
        .patch(
            &path,
            json!({ "name": "Slack de l’atelier", "enabled": false }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["name"], "Slack de l’atelier");
    assert_eq!(view["enabled"], false);

    // Another workspace's application: the identities are this one's.
    let other = paul.patch(&path, json!({ "botToken": OTHER_TOKEN })).await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST, "{}", other.text());
    assert!(
        other.error_message().contains("autre espace de travail"),
        "{}",
        other.text()
    );
    // The same application's, and a new secret.
    let secret = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6";
    let renewed = paul
        .patch(
            &path,
            json!({ "botToken": BOT_TOKEN, "signingSecret": secret, "enabled": true }),
        )
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.updated"), "{audited}");
    assert!(audited.contains("\"botTokenChanged\":true"), "{audited}");
    assert!(
        !audited.contains(secret) && !audited.contains(BOT_TOKEN),
        "{audited}"
    );

    // Deleted, with what its people linked.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into gateway_identities (tenant_id, gateway_id, user_id, external_id, address) \
         values ($1, $2, $3, 'U0LEA', 'lea@neo-maintenance.example')",
    )
    .bind(tenant::ID)
    .bind(id)
    .bind(user::LEA)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(gateway_view(&paul, id).await["identities"], 1);
    let deleted = paul.delete(&path).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let left: i64 = sqlx::query_scalar("select count(*) from gateway_identities")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(left, 0);
    let secrets: i64 =
        sqlx::query_scalar("select count(*) from secrets where name like 'gateway.%'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(secrets, 0);
    // Answered as absent once gone.
    let gone = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND, "{}", gone.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_connectors_administrators_reach_the_gateways() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_slack(&paul).await;
    let lea = api.sign_in(user::LEA).await;
    assert_eq!(lea.get(GATEWAYS).await.status, StatusCode::FORBIDDEN);
    let declared = lea
        .post(
            DECLARE,
            json!({ "name": "Slack", "botToken": BOT_TOKEN, "signingSecret": SIGNING_SECRET }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::FORBIDDEN);
    let path = format!("{GATEWAYS}/{id}");
    let changed = lea.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(changed.status, StatusCode::FORBIDDEN);
    assert_eq!(lea.delete(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(gateway_view(&paul, id).await["enabled"], true);
}
