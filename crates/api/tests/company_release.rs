//! What a suspended company declared at other services, withdrawn there
//! by the operator before it is deleted (ADR-193): every OAuth
//! authorization of its MCP servers, the company's and its people's,
//! revoked at the authorization server that granted it, and its Telegram
//! bot withdrawn with its webhook; its Slack and Teams applications stay. An active
//! company keeps them; without the master key, nothing is taken; its
//! deletion is refused while they are declared. Audited by the `system`
//! actor, never with a token; a second release finds nothing.

mod support;

use mimorya_api::services::provisioning::{
    CompanyDeletion, CompanyStatusChange, delete_company, release_company, release_company_with,
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

/// The bots the platform withdrew, as audited.
async fn bot_entries(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entries = sqlx::query_scalar(
        "select metadata from audit_log where action = 'gateway.deleted' \
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
    // Its Slack and Teams applications live in its own workspace: nothing
    // of them is the platform's to withdraw, nor do they hold the deletion
    // back.
    let applications: Vec<uuid::Uuid> = sqlx::query_scalar(
        "insert into gateways (tenant_id, kind, name, workspace_id, workspace_name, bot_user_id) \
         values ($1, 'slack', 'Slack Néo', 'T0LIBRE', 'Néo', 'U0LIBRE'), \
                ($1, 'teams', 'Teams Néo', '11111111-2222-4333-8444-555555555555', \
                 '11111111-2222-4333-8444-555555555555', '66666666-7777-4888-9999-000000000000') \
         returning id",
    )
    .bind(tenant::ID)
    .fetch_all(&owner)
    .await
    .unwrap();
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.to_release, 3);
    let outside = shown.held_outside();
    for what in [
        "autorisations OAuth de serveurs MCP données pour l’entreprise",
        "autorisations OAuth de serveurs MCP données par ses personnes",
        "passerelles de conversation",
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
    // Without the master key, nothing taken could be revoked: nothing is.
    let mut keyless = services.config.clone();
    keyless.secrets = None;
    let keyless = release_company_with(&services.db, &keyless, tenant::SLUG).await;
    let keyless = domain(keyless.unwrap_err());
    assert_eq!(keyless.code, ErrorCode::Validation);
    assert!(keyless.message.starts_with("MIMORYA_MASTER_KEY"));
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
    // The Slack and Teams applications stay, untouched.
    let mut gateways: Vec<uuid::Uuid> =
        sqlx::query_scalar("select id from gateways where tenant_id = $1")
            .bind(tenant::ID)
            .fetch_all(&owner)
            .await
            .unwrap();
    gateways.sort();
    let mut kept = applications.clone();
    kept.sort();
    assert_eq!(gateways, kept);
    // Audited by the platform, never with a token: the bot as its removal is.
    let bot = bot_entries(&api).await;
    assert_eq!(bot.len(), 1);
    assert_eq!(bot[0]["name"], "Telegram Néo");
    assert_eq!(bot[0]["type"], "telegram");
    assert_eq!(bot[0]["webhookWithdrawn"], true);
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
    sqlx::query("delete from gateways where id = any($1)")
        .bind(&applications)
        .execute(&owner)
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
