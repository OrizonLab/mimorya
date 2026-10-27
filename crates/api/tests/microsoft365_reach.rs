//! Whose Microsoft 365 account a run's tools reach, and with which token
//! (ADR-117): a sub-agent reads its run's person's; a token kept serves only
//! the link it was renewed from, never once the link is removed or its
//! application disabled; a renewal or a revocation read before a link was
//! made again leaves the new link alone, and is not said to have removed it.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_api::services::account_links::PersonAccounts;
use mimorya_connectors::testing::FakeGraph;
use mimorya_core::account_links::{MICROSOFT365, sealed_as};
use mimorya_core::demo::{tenant, user};
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use mimorya_db::repositories::account_links::{
    delete_revoked_account_link, get_account_link, renew_account_link,
};
use serde_json::json;
use support::account_links::{
    LINKED, LINKS, insert_microsoft365_reading, kept_refresh_token, link, microsoft_person,
};
use support::identity_provider::MockIdentityProvider;
use support::mailbox::{told, tool_result};
use support::recovery::conversation_of;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};

const TOKEN: &str = "acces-sujet-objet-claire";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sub_agent_reads_its_runs_persons_mailbox() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    api.script(vec![
        ScriptedStep::calls(
            "",
            vec![call(
                "delegate_task",
                json!({ "profile": "research", "task": "Lister les factures reçues" }),
            )],
        ),
        ScriptedStep::calls(
            "",
            vec![call("m365_mail_search", json!({ "query": "facture" }))],
        ),
        ScriptedStep::dynamic(|request| {
            let results = last_tool_results(request);
            assert!(
                results[0]
                    .content
                    .contains("Facture de mars — sujet-objet-claire"),
                "{results:?}"
            );
            ScriptedReply::text("Une facture : celle de mars.")
        }),
        ScriptedStep::text("Vous avez reçu la facture de mars."),
    ]);
    let conversation = conversation_of(&claire).await;
    let sent = claire
        .post(
            &format!("/conversations/{conversation}/messages"),
            json!({ "content": "Quelles factures ai-je reçues ?" }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    let asked = graph.requests();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].token, format!("{TOKEN}-1"));
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_kept_token_serves_only_the_link_it_was_renewed_from() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    link(&claire, &idp, connector).await;
    let services = &api.app.services;
    let accounts = PersonAccounts::new(services.account_links.clone(), tenant::ID, user::CLAIRE);
    let first = format!("{TOKEN}-1");
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[]).await,
        Ok(first.clone())
    );
    assert_eq!(accounts.access_token(MICROSOFT365, &[]).await, Ok(first));
    assert_eq!(idp.renewals().len(), 1);

    // Removed by the person: the token kept no longer serves.
    let removed = claire.delete(&format!("{LINKS}/{connector}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[]).await,
        Err(AccountAccessError::NotLinked)
    );
    let none = tool_result(&api, &claire, "m365_mail_search", json!({})).await;
    assert!(
        told(&none).contains("Aucun compte Microsoft 365 relié"),
        "{none}"
    );
    assert!(graph.requests().is_empty());

    // Made again: renewed anew.
    link(&claire, &idp, connector).await;
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[]).await,
        Ok(format!("{TOKEN}-2"))
    );

    // The application disabled: nothing is served.
    let paul = api.sign_in(user::PAUL).await;
    let disabled = paul
        .patch(
            &format!("/admin/connectors/{connector}"),
            json!({ "enabled": false }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    match accounts.access_token(MICROSOFT365, &[]).await {
        Err(AccountAccessError::Unavailable(why)) => assert!(why.contains("n’est pas active")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_late_renewal_or_revocation_leaves_a_link_made_again_alone() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    link(&claire, &idp, connector).await;
    let services = &api.app.services;
    let read = |api: &TestApi| {
        let db = api.app.services.db.clone();
        async move {
            let mut tx = db.begin(tenant::ID).await.unwrap();
            let (link, _) = get_account_link(&mut tx, connector, user::CLAIRE)
                .await
                .unwrap()
                .unwrap();
            tx.commit().await.unwrap();
            link
        }
    };
    let before = read(&api).await;
    link(&claire, &idp, connector).await;
    let cipher = services.config.secrets.clone().expect("master key");
    let late = cipher
        .seal(
            tenant::ID,
            &sealed_as(connector, user::CLAIRE),
            "jeton-tardif",
        )
        .unwrap();

    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let renewed = renew_account_link(&mut tx, before.id, before.refreshed_at, &late).await;
    assert_eq!(renewed.unwrap(), None);
    let removed = delete_revoked_account_link(&mut tx, before.id, before.refreshed_at).await;
    assert!(!removed.unwrap());
    tx.commit().await.unwrap();
    let kept = MockIdentityProvider::refresh_token_of("sujet-objet-claire");
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        Some(kept)
    );

    // As read now, both apply.
    let now = read(&api).await;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let renewed = renew_account_link(&mut tx, now.id, now.refreshed_at, &late).await;
    let version = renewed.unwrap().expect("renewed");
    assert!(version > now.refreshed_at);
    assert!(
        delete_revoked_account_link(&mut tx, now.id, version)
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_made_again_during_its_revocation_is_not_said_removed() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    link(&claire, &idp, connector).await;
    idp.revoke_refresh_tokens();
    let gate = idp.hold_renewals();
    let accounts = PersonAccounts::new(
        api.app.services.account_links.clone(),
        tenant::ID,
        user::CLAIRE,
    );
    let asked = tokio::spawn(async move { accounts.access_token(MICROSOFT365, &[]).await });
    gate.reached().await;
    // Made again while Microsoft answered for the old one.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update account_links set refreshed_at = now() where user_id = $1")
        .bind(user::CLAIRE)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    gate.release();
    match asked.await.unwrap() {
        Err(AccountAccessError::Unavailable(why)) => {
            assert!(why.contains("vient d’être refaite"), "{why}")
        }
        other => panic!("{other:?}"),
    }
    assert!(
        kept_refresh_token(&api, connector, user::CLAIRE)
            .await
            .is_some()
    );
}
