//! What a suspended company declared at other services, withdrawn there
//! by the operator before it is deleted (ADR-193): every OAuth
//! authorization of its MCP servers, the company's and its people's,
//! revoked at the authorization server that granted it, and its Telegram
//! bot withdrawn with its webhook. An active company keeps them; its
//! deletion is refused while they are declared. Audited by the `system`
//! actor, never with a token; a second release finds nothing.

mod support;

use mimorya_api::services::provisioning::{
    CompanyDeletion, CompanyStatusChange, delete_company, release_company,
};
use mimorya_connectors::testing::{FakeAuthorizationServer, FakeTelegram, TELEGRAM_BOT};
use mimorya_core::demo::{tenant, user};
use mimorya_core::{DomainError, ErrorCode};
use serde_json::Value;
use support::TestApi;
use support::companies::rows_of;
use support::connector_oauth::{AUTHORIZED, authorize, declared, kept_tokens};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_count, person_kept,
};
use support::retention::owner;
use support::sso::location;
use support::suspension::{DemoActiveOnDrop, change};
use support::telegram::declare_telegram;

fn domain(error: anyhow::Error) -> DomainError {
    error.downcast::<DomainError>().expect("domain error")
}

/// The audit log of the demo company, every column of every entry as text.
async fn audit_text(api: &TestApi) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let text: Option<String> =
        sqlx::query_scalar("select string_agg(row_to_json(a)::text, ' ') from audit_log a")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    text.unwrap_or_default()
}

/// What the release audited of itself.
async fn released_entries(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entries = sqlx::query_scalar(
        "select metadata from audit_log where action = 'tenant.released' \
         and actor_type = 'system' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    entries
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_s_grants_and_bots_are_withdrawn_at_their_services() {
    let telegram = FakeTelegram::start().await;
    let api = TestApi::start_with_telegram(&PRIVATE_DEPLOYMENT, &telegram.url).await;
    let _active = DemoActiveOnDrop;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (company_server, company_id) = declared(&paul, &authority, "liberee").await;
    let authorized = authorize(&paul, &authority, &company_id).await;
    assert_eq!(location(&authorized), AUTHORIZED);
    let (access, refresh) = kept_tokens(&api, &company_id).await.expect("kept");
    let refresh = refresh.expect("renewable");
    let (each_server, each_id) = declared_per_person(&paul, &authority, "chacun").await;
    let his = person_authorize(&marc, &authority, &each_id).await;
    assert_eq!(location(&his), PERSON_AUTHORIZED);
    let marcs = person_kept(&api, &each_id, user::MARC).await.expect("kept");
    declare_telegram(&paul).await;
    assert!(telegram.webhook(TELEGRAM_BOT).is_some());
    let services = &api.app.services;

    // An active company keeps them, as an unknown one has none.
    let active = domain(release_company(services, tenant::SLUG).await.unwrap_err());
    assert_eq!(active.code, ErrorCode::Conflict);
    assert!(active.message.starts_with("Seule une entreprise suspendue"));
    let unknown = domain(release_company(services, "nulle-part").await.unwrap_err());
    assert_eq!(unknown.code, ErrorCode::NotFound);
    assert!(kept_tokens(&api, &company_id).await.is_some());
    assert!(telegram.webhook(TELEGRAM_BOT).is_some());

    // Suspended, its deletion is refused while they are declared.
    change(&api, tenant::SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    let owner = owner().await;
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.to_release, 3);
    let outside = shown.held_outside();
    for what in [
        "autorisations OAuth de serveurs MCP données pour l’entreprise",
        "autorisations OAuth de serveurs MCP données par ses personnes",
        "passerelles de conversation (Slack, Telegram, Teams)",
    ] {
        assert!(
            outside
                .iter()
                .any(|(said, count)| *said == what && *count > 0)
        );
    }
    let refused = delete_company(&owner, tenant::SLUG, Some(tenant::SLUG)).await;
    let refused = domain(refused.unwrap_err());
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(
        refused.message.contains("release-company"),
        "{}",
        refused.message
    );
    assert_eq!(rows_of(&owner, tenant::ID).await, shown.tables);

    let released = release_company(services, tenant::SLUG).await.unwrap();
    assert_eq!(released.tenant_id, tenant::ID);
    let mut servers: Vec<(&str, Option<bool>, usize, usize)> = (released.servers.iter())
        .map(|s| {
            let company = s.company.as_ref().map(Result::is_ok);
            (
                s.name.as_str(),
                company,
                s.persons_revoked,
                s.persons_failed,
            )
        })
        .collect();
    servers.sort();
    assert_eq!(
        servers,
        [
            ("Fichiers chacun", None, 1, 0),
            ("Fichiers liberee", Some(true), 0, 0),
        ]
    );
    assert_eq!(released.bots.len(), 1);
    assert_eq!(released.bots[0].name, "Telegram Néo");
    assert_eq!(released.bots[0].webhook, Ok(()));
    // Revoked there, the company's refresh token asked for; gone here.
    let revoked = authority.requests();
    assert!((revoked.iter()).any(|r| r.endpoint == "revoke" && r.params["token"] == refresh));
    assert!(!authority.accepts(&access, &company_server.url));
    assert!(!authority.accepts(&marcs, &each_server.url));
    assert_eq!(kept_tokens(&api, &company_id).await, None);
    assert_eq!(person_kept(&api, &each_id, user::MARC).await, None);
    assert_eq!(person_count(&api, &each_id).await, 0);
    assert_eq!(telegram.webhook(TELEGRAM_BOT), None);
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.to_release, 0);
    // Audited by the platform, never with a token.
    let entries = released_entries(&api).await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["companyAuthorizations"], 1);
    assert_eq!(entries[0]["personAuthorizations"], 1);
    assert_eq!(entries[0]["telegramBots"], 1);
    let audited = audit_text(&api).await;
    for token in [&access, &refresh, &marcs] {
        assert!(!audited.contains(token.as_str()));
    }

    // Nothing is left for a second release.
    let again = release_company(services, tenant::SLUG).await.unwrap();
    assert!(again.servers.is_empty() && again.bots.is_empty());

    change(&api, tenant::SLUG, CompanyStatusChange::Reactivate)
        .await
        .unwrap();
    for id in [&company_id, &each_id] {
        let removed = paul.delete(&format!("/admin/connectors/{id}")).await;
        assert_eq!(
            removed.status,
            axum::http::StatusCode::OK,
            "{}",
            removed.text()
        );
    }
    owner.close().await;
}
