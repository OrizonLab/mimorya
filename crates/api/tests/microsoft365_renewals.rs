//! A linked account's access renewed (ADR-117): a refresh token the
//! provider rotates is sealed in the link; one it no longer honours removes
//! the link, audited, and the person is told to link again; a token Graph
//! refuses is renewed at the next call.

mod support;

use mimorya_connectors::testing::FakeGraph;
use mimorya_core::demo::user;
use serde_json::json;
use support::account_links::{
    LINKED, insert_microsoft365_reading, kept_refresh_token, link, link_entries, links,
    microsoft_person,
};
use support::identity_provider::MockIdentityProvider;
use support::mailbox::{told, tool_result};
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_the_provider_no_longer_honours_is_removed() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let since = latest_entry(&api).await;

    idp.revoke_refresh_tokens();
    let refused = tool_result(&api, &claire, "m365_mail_search", json!({})).await;
    assert!(told(&refused).contains("relie de nouveau"), "{refused}");
    assert!(graph.requests().is_empty());
    assert_eq!(links(&claire).await[0]["linked"], json!(null));
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    let entries = link_entries(&api, since).await;
    let revoked: Vec<_> = entries
        .iter()
        .filter(|(action, ..)| action == "account.link.revoked")
        .collect();
    assert_eq!(revoked.len(), 1, "{entries:?}");
    assert!(!format!("{entries:?}").contains("jeton-de-renouvellement"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_rotated_token_is_kept_and_a_refused_one_renewed() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    link(&claire, &idp, connector).await;
    idp.rotate_refresh_tokens();

    let read = tool_result(&api, &claire, "m365_mail_search", json!({})).await;
    assert_eq!(read["nombre"], 3, "{read}");
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE)
            .await
            .as_deref(),
        Some("jeton-de-renouvellement-sujet-objet-claire-1")
    );

    // Revoked before it expired: Graph refuses it, the next call renews it.
    graph.refuse("acces-sujet-objet-claire-1");
    let refused = tool_result(&api, &claire, "m365_mail_search", json!({})).await;
    assert!(told(&refused).contains("réessayez"), "{refused}");
    let renewed = tool_result(&api, &claire, "m365_mail_search", json!({})).await;
    assert_eq!(renewed["nombre"], 3, "{renewed}");
    assert_eq!(
        graph.requests().last().unwrap().token,
        "acces-sujet-objet-claire-2"
    );
    assert_eq!(idp.renewals().len(), 2);
    // Renewed from the rotated token, itself rotated.
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE)
            .await
            .as_deref(),
        Some("jeton-de-renouvellement-sujet-objet-claire-2")
    );
}
