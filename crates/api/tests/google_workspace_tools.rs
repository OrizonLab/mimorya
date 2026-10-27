//! The Google Workspace tools in a run (ADR-122): they read the Gmail and
//! calendar of the person the run serves, with an access token renewed from
//! that person's link and kept for the next calls; never another person's,
//! never without a link; the token is neither shown nor kept in the
//! journal; nor in a skill's evaluation, which is no one's request. An
//! administrator's policy keeps `connector:use`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call, last_tool_results};
use mimorya_api::services::skill_evaluations::NO_LINKED_ACCOUNT;
use mimorya_connectors::google_workspace::GOOGLE_WORKSPACE_TOOLS;
use mimorya_connectors::testing::FakeGoogle;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::account_links::{LINKED, google_person, insert_google_workspace_reading, link};
use support::identity_provider::MockIdentityProvider;
use support::mailbox::{told, tool_result};
use support::skill_evaluations::{forget, launched, play, verdict, write_case};
use support::skill_proposals::SKILLS;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{Client, TestApi, email};
use uuid::Uuid;

/// The person signs in at Google as their Workspace account, then links it.
async fn linked(
    person: &Client<'_>,
    idp: &MockIdentityProvider,
    connector: Uuid,
    subject: &str,
    address: &str,
) {
    let domain = address.rsplit_once('@').unwrap().1;
    idp.sign_in_as(google_person(subject, address, Some(domain)));
    assert_eq!(location(&link(person, idp, connector).await), LINKED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_reads_its_persons_gmail_and_calendar_with_a_renewed_token() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_reading(&api, &idp, &google).await;
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    for name in GOOGLE_WORKSPACE_TOOLS {
        assert!(tools.tools.iter().any(|t| t.info().name == name), "{name}");
    }
    let claire = api.sign_in(user::CLAIRE).await;
    linked(&claire, &idp, connector, "g-claire", &email(user::CLAIRE)).await;

    let found = tool_result(
        &api,
        &claire,
        "google_gmail_search",
        json!({ "query": "facture" }),
    )
    .await;
    assert_eq!(found["nombre"], 1, "{found}");
    assert_eq!(found["emails"][0]["objet"], "Facture de mars — g-claire");
    let asked = google.requests();
    assert!(
        asked.iter().all(|r| r.token == "acces-g-claire-1"),
        "{asked:?}"
    );
    // Renewed with what she granted: Google is named no permission.
    assert_eq!(idp.renewals(), [""]);

    // The next runs use the token kept.
    let read = tool_result(&api, &claire, "google_gmail_read", json!({ "id": "m2" })).await;
    assert_eq!(read["texte"], "Préparé pour g-claire.", "{read}");
    let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
    let events = tool_result(&api, &claire, "google_calendar_events", week).await;
    assert_eq!(events["nombre"], 2, "{events}");
    assert!(
        google
            .requests()
            .iter()
            .all(|r| r.token == "acces-g-claire-1")
    );
    assert_eq!(idp.renewals().len(), 1);

    // Neither the access nor the refresh token in the journal or the audit.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let written: String = sqlx::query_scalar(
        "select coalesce((select string_agg(t::text, ' ') from tool_invocations t), '') || \
         coalesce((select string_agg(a::text, ' ') from audit_log a), '') || \
         coalesce((select string_agg(e::text, ' ') from run_events e), '')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(
        written.contains("google_gmail_search"),
        "the journal keeps the call"
    );
    assert!(!written.contains("acces-g-claire"));
    assert!(!written.contains("jeton-de-renouvellement"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_person_reads_their_own_google_account_only() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_reading(&api, &idp, &google).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let marc = api.sign_in(user::MARC).await;
    linked(&claire, &idp, connector, "g-claire", &email(user::CLAIRE)).await;
    linked(&marc, &idp, connector, "g-marc", &email(user::MARC)).await;

    let theirs = tool_result(&api, &marc, "google_gmail_search", json!({})).await;
    assert_eq!(theirs["nombre"], 3, "{theirs}");
    for found in theirs["emails"].as_array().unwrap() {
        assert!(
            found["objet"].as_str().unwrap().ends_with("g-marc"),
            "{found}"
        );
    }
    assert!(
        google
            .requests()
            .iter()
            .all(|r| r.token.starts_with("acces-g-marc-")),
        "{:?}",
        google.requests()
    );

    // Without a link, nothing reaches Google.
    let julien = api.sign_in(user::JULIEN).await;
    let before = google.requests().len();
    let none = tool_result(&api, &julien, "google_gmail_search", json!({})).await;
    assert!(
        told(&none).contains("Aucun compte Google Workspace relié"),
        "{none}"
    );
    assert_eq!(google.requests().len(), before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_policy_keeps_the_connectors_permission() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_reading(&api, &idp, &google).await;
    let paul = api.sign_in(user::PAUL).await;
    let path = format!("/admin/connectors/{connector}");
    let mapped = paul
        .patch(
            &path,
            json!({ "toolPolicies": { "google_gmail_read": { "risk": "write" } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    // Written outside the API: permissions without connector:use.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = $2, updated_at = now() where id = $1",
    )
    .bind(connector)
    .bind(json!({ "google_gmail_read": { "permissions": ["intervention:read"] } }))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    assert_eq!(
        tools.overrides["google_gmail_read"].permissions,
        Some(vec![
            "intervention:read".to_string(),
            "connector:use".to_string()
        ])
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_reads_no_ones_google_account() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_reading(&api, &idp, &google).await;
    let paul = api.sign_in(user::PAUL).await;
    // Whom it runs for linked theirs: still not read.
    linked(&paul, &idp, connector, "g-paul", &email(user::PAUL)).await;
    let created = paul
        .post(
            SKILLS,
            json!({ "name": "gmail-evalue", "description": "Relire le courrier.", "content": "1. Lire." }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let skill: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    write_case(&paul, skill, "Courrier", "Qu’ai-je reçu ?").await;
    api.script(vec![
        ScriptedStep::calls("", vec![call("google_gmail_search", json!({}))]),
        ScriptedStep::text("Rien à signaler."),
        verdict(true),
    ]);
    launched(&paul, skill, 1).await;
    play(&api).await;

    let requests = api.model.as_ref().unwrap().requests();
    let result = last_tool_results(&requests[1]);
    assert!(
        result[0].content.contains(NO_LINKED_ACCOUNT),
        "{:?}",
        result
    );
    assert!(google.requests().is_empty());
    forget(&api, skill).await;
    paul.delete(&format!("{SKILLS}/{skill}")).await;
}
