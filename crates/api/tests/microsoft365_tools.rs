//! The Microsoft 365 tools in a run (ADR-117): they read the mailbox and
//! calendar of the person the run serves, with an access token renewed
//! from that person's link and kept for the next calls; never another
//! person's, never without a link; the token is neither shown nor kept in
//! the journal; nor in a skill's evaluation, which is no one's request.
//! An administrator's policy keeps `connector:use`.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedStep, call, last_tool_results};
use mimorya_api::services::skill_evaluations::NO_LINKED_ACCOUNT;
use mimorya_connectors::microsoft365::MICROSOFT365_TOOLS;
use mimorya_connectors::testing::FakeGraph;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::account_links::{LINKED, insert_microsoft365_reading, link, microsoft_person};
use support::identity_provider::MockIdentityProvider;
use support::mailbox::{told, tool_result};
use support::skill_evaluations::{forget, launched, play, verdict, write_case};
use support::skill_proposals::SKILLS;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_reads_its_persons_mailbox_with_a_renewed_token() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    for name in MICROSOFT365_TOOLS {
        assert!(tools.tools.iter().any(|t| t.info().name == name), "{name}");
    }
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);

    let found = tool_result(
        &api,
        &claire,
        "m365_mail_search",
        json!({ "query": "facture" }),
    )
    .await;
    assert_eq!(found["nombre"], 1, "{found}");
    assert_eq!(
        found["emails"][0]["objet"],
        "Facture de mars — sujet-objet-claire"
    );
    let asked = graph.requests();
    assert_eq!(asked[0].token, "acces-sujet-objet-claire-1");
    // Renewed with what she granted, read and offline access among it.
    let renewals = idp.renewals();
    assert_eq!(renewals.len(), 1);
    for scope in ["offline_access", "Mail.Read", "Calendars.Read"] {
        assert!(renewals[0].split(' ').any(|s| s == scope), "{renewals:?}");
    }

    // The next run uses the token kept.
    let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
    let events = tool_result(&api, &claire, "m365_calendar_events", week).await;
    assert_eq!(events["nombre"], 2, "{events}");
    assert_eq!(graph.requests()[1].token, "acces-sujet-objet-claire-1");
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
        written.contains("m365_mail_search"),
        "the journal keeps the call"
    );
    assert!(!written.contains("acces-sujet"));
    assert!(!written.contains("jeton-de-renouvellement"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_person_reads_their_own_account_only() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let marc = api.sign_in(user::MARC).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    link(&claire, &idp, connector).await;
    idp.sign_in_as(microsoft_person("objet-marc", &email(user::MARC)));
    link(&marc, &idp, connector).await;

    let theirs = tool_result(&api, &marc, "m365_mail_search", json!({})).await;
    for found in theirs["emails"].as_array().unwrap() {
        assert!(
            found["objet"]
                .as_str()
                .unwrap()
                .ends_with("sujet-objet-marc"),
            "{found}"
        );
    }
    // Claire's e-mail, named in Marc's run, is read in Marc's mailbox.
    let hers = json!({ "id": "sujet-objet-claire/m2=" });
    let refused = tool_result(&api, &marc, "m365_mail_read", hers).await;
    assert!(told(&refused).contains("Introuvable"), "{refused}");
    for asked in graph.requests() {
        assert!(
            asked.token.starts_with("acces-sujet-objet-marc-"),
            "{}",
            asked.token
        );
    }

    // Without a link, nothing reaches Graph.
    let julien = api.sign_in(user::JULIEN).await;
    let before = graph.requests().len();
    let none = tool_result(&api, &julien, "m365_mail_search", json!({})).await;
    assert!(
        told(&none).contains("Aucun compte Microsoft 365 relié"),
        "{none}"
    );
    assert_eq!(graph.requests().len(), before);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_policy_keeps_the_connectors_permission() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let path = format!("/admin/connectors/{connector}");
    let mapped = paul
        .patch(
            &path,
            json!({ "toolPolicies": { "m365_mail_read": { "risk": "write" } } }),
        )
        .await;
    assert_eq!(mapped.status, StatusCode::OK, "{}", mapped.text());
    // Written outside the API: permissions without connector:use.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "update connector_instances set tool_policies = $2, updated_at = now() where id = $1",
    )
    .bind(connector)
    .bind(json!({ "m365_mail_read": { "permissions": ["intervention:read"] } }))
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
        tools.overrides["m365_mail_read"].permissions,
        Some(vec![
            "intervention:read".to_string(),
            "connector:use".to_string()
        ])
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_evaluation_reads_no_ones_account() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let paul = api.sign_in(user::PAUL).await;
    // Whom it runs for linked theirs: still not read.
    idp.sign_in_as(microsoft_person("objet-paul", &email(user::PAUL)));
    assert_eq!(location(&link(&paul, &idp, connector).await), LINKED);
    let created = paul
        .post(
            SKILLS,
            json!({ "name": "courrier-evalue", "description": "Relire le courrier.", "content": "1. Lire." }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let skill: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    write_case(&paul, skill, "Courrier", "Qu’ai-je reçu ?").await;
    api.script(vec![
        ScriptedStep::calls("", vec![call("m365_mail_search", json!({}))]),
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
    assert!(graph.requests().is_empty());
    forget(&api, skill).await;
    paul.delete(&format!("{SKILLS}/{skill}")).await;
}
