//! A person links their Teams identity to their account from the link the
//! company's Teams bot sends them (ADR-138): previewed, then confirmed in
//! their own session once Teams, asked through the conversation the bot
//! met them in, says its member is the Entra object the link was sent to,
//! of the company's directory, with the account's own address. Another
//! address, another directory or another person links nothing, audited
//! with the reason; a link opened by someone else links nothing; nor an
//! expired link, a link opened in another company's session, a paused bot,
//! or someone who may not talk with the agent. Once linked, writing to the bot is answered
//! that the agent is not talked with in Teams yet.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_connectors::testing::{FakeTeams, TEAMS_DIRECTORY, conversation_of, member_id};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::gateways::GATEWAYS;
use support::manager_approvals::without;
use support::refusals::{entries_since, latest_entry};
use support::slack_companies::other_administrator;
use support::sso::SSO_DEPLOYMENT;
use support::teams::{LEA_TEAMS, MARC_TEAMS, declare_teams, link_code, write};
use support::{Client, TestApi, TestResponse, email};

const LINK: &str = "/account/gateways/link";

async fn confirm(client: &Client<'_>, code: &str) -> TestResponse {
    client.post(LINK, json!({ "code": code })).await
}

/// The bot as `client` lists it in their account.
async fn listed(client: &Client<'_>) -> Value {
    let listed = client.get("/account/gateways").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["gateways"][0].clone()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_teams_identity_from_the_bot_s_link() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = declare_teams(&paul).await;
    teams.add_person(MARC_TEAMS.0, MARC_TEAMS.1, &email(user::MARC));
    let code = link_code(&api, &teams, id, "5000", MARC_TEAMS).await;
    let since = latest_entry(&api).await;

    // Shown before it is confirmed: the bot, and whom Teams names.
    let preview = marc
        .post(&format!("{LINK}/preview"), json!({ "code": code }))
        .await;
    assert_eq!(preview.status, StatusCode::OK, "{}", preview.text());
    assert_eq!(preview.json()["externalId"], MARC_TEAMS.0);
    let confirmed = confirm(&marc, &code).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
    let own = listed(&marc).await;
    assert_eq!(own["linked"]["externalId"], MARC_TEAMS.0, "{own}");
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.identity_linked"), "{audited}");
    assert!(!audited.contains(&email(user::MARC)), "{audited}");
    assert_eq!(
        paul.get(GATEWAYS).await.json()["gateways"][0]["identities"],
        1
    );

    // Linked: writing to the bot is answered where to talk with the agent.
    write(&api, &teams, id, "5001", MARC_TEAMS, "Bonjour").await;
    let said = teams.sent().pop().unwrap();
    assert_eq!(said.conversation, conversation_of(MARC_TEAMS.0));
    assert!(
        said.text
            .starts_with("Votre compte Teams est relié à votre compte."),
        "{}",
        said.text
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn teams_naming_another_address_directory_or_person_links_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = declare_teams(&paul).await;
    let member = |aad: &str, tenant: &str, address: &str| {
        json!({
            "id": member_id(MARC_TEAMS.0),
            "name": MARC_TEAMS.1,
            "aadObjectId": aad,
            "email": address,
            "userPrincipalName": address,
            "tenantId": tenant,
        })
    };
    let since = latest_entry(&api).await;
    for (activity, profile, said, reason) in [
        (
            "6000",
            member(MARC_TEAMS.0, TEAMS_DIRECTORY, &email(user::LEA)),
            "L’adresse de ce compte Teams n’est pas celle de votre compte",
            "other_address",
        ),
        (
            "6001",
            member(
                MARC_TEAMS.0,
                "1a2b3c4d-5e6f-4789-8abc-def012345678",
                &email(user::MARC),
            ),
            "Ce compte Teams n’est pas de l’annuaire Microsoft de votre entreprise",
            "other_directory",
        ),
        (
            "6002",
            member(LEA_TEAMS.0, TEAMS_DIRECTORY, &email(user::MARC)),
            "Teams ne dit plus que ce compte est celui à qui le lien a été envoyé",
            "other_person",
        ),
    ] {
        teams.set_member(MARC_TEAMS.0, profile);
        let code = link_code(&api, &teams, id, activity, MARC_TEAMS).await;
        let refused = confirm(&marc, &code).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(
            refused.error_message().starts_with(said),
            "{}",
            refused.text()
        );
        let audited = entries_since(&api, since).await;
        assert!(audited.contains(reason), "{audited}");
    }
    assert_eq!(listed(&marc).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_opened_by_someone_else_links_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let id = declare_teams(&paul).await;
    // Sent to Marc, opened by Léa: Teams names Marc's address.
    teams.add_person(MARC_TEAMS.0, MARC_TEAMS.1, &email(user::MARC));
    let code = link_code(&api, &teams, id, "7000", MARC_TEAMS).await;
    let refused = confirm(&lea, &code).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert_eq!(listed(&lea).await["linked"], Value::Null);
    // Neither a code that does not open, nor once the bot is paused.
    let forged = confirm(&lea, "pas-un-code").await;
    assert_eq!(forged.status, StatusCode::NOT_FOUND, "{}", forged.text());
    let marc = api.sign_in(user::MARC).await;
    let paused = paul
        .patch(&format!("{GATEWAYS}/{id}"), json!({ "enabled": false }))
        .await;
    assert_eq!(paused.status, StatusCode::OK, "{}", paused.text());
    let conflict = confirm(&marc, &code).await;
    assert_eq!(conflict.status, StatusCode::CONFLICT, "{}", conflict.text());
    assert!(
        conflict
            .error_message()
            .starts_with("La passerelle Teams de votre entreprise est désactivée"),
        "{}",
        conflict.text()
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn who_may_not_talk_with_the_agent_or_whom_teams_cannot_name_links_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, lea) = (api.sign_in(user::PAUL).await, api.sign_in(user::LEA).await);
    let id = declare_teams(&paul).await;
    teams.add_person(LEA_TEAMS.0, LEA_TEAMS.1, &email(user::LEA));
    let code = link_code(&api, &teams, id, "8000", LEA_TEAMS).await;
    let policy = without(&paul, user::LEA, "agent:use").await;
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    // Teams unreachable when it is confirmed: nothing linked, said so.
    teams.unavailable(true);
    let unreached = confirm(&lea, &code).await;
    teams.unavailable(false);
    assert_eq!(
        unreached.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        unreached.text()
    );
    assert_eq!(listed(&lea).await["linked"], Value::Null);
    // Reachable again: linked.
    let confirmed = confirm(&lea, &code).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
    assert_eq!(listed(&lea).await["linked"]["externalId"], LEA_TEAMS.0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_expired_link_or_another_company_s_session_links_nothing() {
    let teams = FakeTeams::start().await;
    let api = TestApi::start_with_teams(&SSO_DEPLOYMENT, &teams.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let id = declare_teams(&paul).await;
    teams.add_person(MARC_TEAMS.0, MARC_TEAMS.1, &email(user::MARC));
    let code = link_code(&api, &teams, id, "9000", MARC_TEAMS).await;
    let gateways = &api.app.services.gateways;

    // Sixteen minutes later.
    let identity = mimorya_api::services::AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: uuid::Uuid::nil(),
        must_change_password: false,
    };
    let own = api.app.services.access.context(identity).await.unwrap();
    let later = Utc::now() + Duration::minutes(16);
    let expired = gateways.confirm_link(&own, &code, later).await;
    let expired = expired.err().unwrap().to_string();
    assert!(expired.starts_with("Ce lien a expiré"), "{expired}");
    // In another company, as if it were none.
    let (_, jeanne) = other_administrator(&api, "acme-teams-links").await;
    let elsewhere = gateways.preview_link(&jeanne, &code, Utc::now()).await;
    let elsewhere = elsewhere.err().unwrap().to_string();
    assert!(
        elsewhere.starts_with("Lien de liaison invalide"),
        "{elsewhere}"
    );
    assert!(
        gateways
            .confirm_link(&jeanne, &code, Utc::now())
            .await
            .is_err()
    );
    // Still Marc's to use, now.
    let confirmed = confirm(&marc, &code).await;
    assert_eq!(confirmed.status, StatusCode::OK, "{}", confirmed.text());
}
