//! The company's Microsoft Teams bot, declared by its connector
//! administrators (ADR-138): its application, directory and client secret
//! checked as written, then the secret tried with Microsoft Entra before
//! anything is kept; the secret sealed, never returned nor audited; one per
//! company; the messaging endpoint to give the Azure bot shown; its name,
//! state and secret changed (a new secret tried alike), never its
//! application nor directory; deleted. Reserved to the connectors'
//! administrators of the company.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::{FakeSlack, FakeTeams, TEAMS_APP, TEAMS_DIRECTORY, TEAMS_SECRET};
use mimorya_core::demo::user;
use serde_json::json;
use support::gateways::{GATEWAYS, declare_slack, gateway_view, remove_gateways};
use support::refusals::{entries_since, latest_entry};
use support::sso::SSO_DEPLOYMENT;
use support::teams::{DECLARE_TEAMS, declare_teams};
use support::{TestApi, WEB_ORIGIN};

fn declaration(app: &str, directory: &str, secret: &str) -> serde_json::Value {
    json!({ "name": "Teams Néo", "appId": app, "directoryId": directory, "clientSecret": secret })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_declares_its_teams_bot_once_entra_takes_its_secret() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let paul = api.sign_in(user::PAUL).await;
    remove_gateways(&paul).await;
    let since = latest_entry(&api).await;

    // Written otherwise than Microsoft writes them: refused before Entra.
    for (app, directory, secret, said) in [
        (
            "pas-un-guid",
            TEAMS_DIRECTORY,
            TEAMS_SECRET,
            "Microsoft App ID",
        ),
        (
            TEAMS_APP,
            "contoso.onmicrosoft.com",
            TEAMS_SECRET,
            "annuaire",
        ),
        (TEAMS_APP, TEAMS_DIRECTORY, "court", "Secret client"),
    ] {
        let refused = paul
            .post(DECLARE_TEAMS, declaration(app, directory, secret))
            .await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(refused.error_message().contains(said), "{}", refused.text());
    }
    assert_eq!(teams.token_requests(), 0);
    // A secret Entra refuses: nothing is kept.
    let wrong = "Abc8Q~fictif.mauvais_secret-0000000000";
    let refused = paul
        .post(
            DECLARE_TEAMS,
            declaration(TEAMS_APP, TEAMS_DIRECTORY, wrong),
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
            .starts_with("Microsoft Entra refuse l’application du bot"),
        "{}",
        refused.text()
    );
    assert_eq!(teams.token_requests(), 1);
    let listed = paul.get(GATEWAYS).await.json();
    assert_eq!(listed["gateways"], json!([]));

    // Taken: kept with its application and directory, the endpoint shown.
    let id = declare_teams(&paul).await;
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["type"], "teams");
    assert_eq!(view["workspaceId"], TEAMS_DIRECTORY);
    assert_eq!(view["botUserId"], TEAMS_APP);
    assert_eq!(
        view["eventsUrl"],
        format!("{WEB_ORIGIN}/api/gateways/teams/{id}")
    );
    assert_eq!(view["secretsStored"], true);
    assert_eq!(view["identities"], 0);
    assert_eq!(view["missingScopes"], serde_json::Value::Null);
    // One per company.
    let again = paul
        .post(
            DECLARE_TEAMS,
            declaration(TEAMS_APP, TEAMS_DIRECTORY, TEAMS_SECRET),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());
    // Never the secret, neither listed nor audited.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.created"), "{audited}");
    assert!(audited.contains(TEAMS_APP), "{audited}");
    assert!(!audited.contains(TEAMS_SECRET), "{audited}");
    assert!(!paul.get(GATEWAYS).await.text().contains(TEAMS_SECRET));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_teams_bot_changes_its_name_state_and_secret_only() {
    let (slack, teams) = (FakeSlack::start().await, FakeTeams::start().await);
    let api = TestApi::start_with_slack_and_teams(&SSO_DEPLOYMENT, &slack.url, &teams.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let id = declare_teams(&paul).await;
    let path = format!("{GATEWAYS}/{id}");
    let since = latest_entry(&api).await;

    let changed = paul
        .patch(&path, json!({ "name": "Teams de Néo", "enabled": false }))
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let view = gateway_view(&paul, id).await;
    assert_eq!(view["name"], "Teams de Néo");
    assert_eq!(view["enabled"], false);
    // A new secret, tried with Entra for the same application.
    let wrong = paul
        .patch(
            &path,
            json!({ "clientSecret": "Abc8Q~fictif.mauvais_secret-0000000000" }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST, "{}", wrong.text());
    let requests = teams.token_requests();
    let renewed = paul
        .patch(&path, json!({ "clientSecret": TEAMS_SECRET }))
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    assert_eq!(teams.token_requests(), requests + 1);
    // Neither a token nor a signing secret: a Teams bot has none.
    for body in [
        json!({ "botToken": "xoxb-x" }),
        json!({ "signingSecret": "s" }),
    ] {
        let refused = paul.patch(&path, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
    }
    let audited = entries_since(&api, since).await;
    assert!(
        audited.contains(r#""clientSecretChanged":true"#),
        "{audited}"
    );
    assert!(!audited.contains(TEAMS_SECRET), "{audited}");

    // Nor permissions to check with Slack.
    let checked = paul.post(&format!("{path}/check"), json!({})).await;
    assert_eq!(
        checked.status,
        StatusCode::BAD_REQUEST,
        "{}",
        checked.text()
    );
    assert!(
        checked.error_message().ends_with("un bot Teams n’en a pas"),
        "{}",
        checked.text()
    );

    // A Slack application has no client secret.
    let slack = declare_slack(&paul).await;
    let refused = paul
        .patch(
            &format!("{GATEWAYS}/{slack}"),
            json!({ "clientSecret": TEAMS_SECRET }),
        )
        .await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    remove_gateways(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_connectors_administrators_declare_a_teams_bot() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let refused = marc
        .post(
            DECLARE_TEAMS,
            declaration(TEAMS_APP, TEAMS_DIRECTORY, TEAMS_SECRET),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    let id = declare_teams(&paul).await;
    let changed = marc
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(changed.status, StatusCode::FORBIDDEN, "{}", changed.text());
    // Deleted by its administrators, its secret with it.
    let deleted = paul.delete(&format!("{GATEWAYS}/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    assert_eq!(paul.get(GATEWAYS).await.json()["gateways"], json!([]));
}
