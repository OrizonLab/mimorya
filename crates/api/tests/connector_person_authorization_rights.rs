//! Who authorizes an MCP server for themself, and when (ADR-174): only the
//! person who started it finishes it, an administrator presenting their
//! attempt included, and a person never finishes an administrator's; only
//! a person who may use connectors, on an enabled server each person
//! authorizes, starts one; a return after they lost that right, after the
//! server's declaration changed or after it was disabled keeps nothing. The
//! refusals are audited, never a token.

mod support;

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::TestApi;
use support::accounts::{activate, assignment_of, create_account};
use support::connector_oauth::{FAILED, api_path, come_back, declared, kept_tokens, start};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{
    PERSON_FAILED, declared_per_person, entries_by, person_kept, person_start, try_person_start,
};
use support::sso::location;

const AUTHORIZED: &str = "connector.person_oauth_authorized";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_person_who_started_it_finishes_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lea = api.sign_in(user::LEA).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "aqui").await;

    // Marc's attempt, presented by Léa, then by an administrator: refused.
    let (url, cookie) = person_start(&marc, &id).await;
    let back = api_path(&authority.sent_back(&url).await);
    for other in [&lea, &paul] {
        assert_eq!(
            location(&come_back(other, &back, Some(&cookie)).await),
            PERSON_FAILED
        );
    }
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    assert!(person_kept(&api, &id, user::LEA).await.is_none());
    assert!(person_kept(&api, &id, user::PAUL).await.is_none());
    for other in [user::LEA, user::PAUL] {
        let refused = entries_by(&api, None, other, AUTHORIZED).await;
        let reason = refused
            .last()
            .expect("audited")
            .1
            .clone()
            .unwrap_or_default();
        assert_eq!(reason, "Tentative d’une autre personne");
    }

    // A return without any attempt, from someone who administers nothing:
    // refused as theirs, back to their account.
    let lost = come_back(&marc, &back, None).await;
    assert_eq!(location(&lost), PERSON_FAILED);
    let refused = entries_by(&api, None, user::MARC, AUTHORIZED).await;
    let reason = refused
        .last()
        .expect("audited")
        .1
        .clone()
        .unwrap_or_default();
    assert_eq!(reason, "Tentative inconnue ou expirée");

    // An administrator's attempt on a server the company authorizes is
    // never finished as a person's.
    let (_, company) = declared(&paul, &authority, "aquient").await;
    let (url, cookie) = start(&paul, &company).await;
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&marc, &back, Some(&cookie)).await),
        FAILED
    );
    assert!(kept_tokens(&api, &company).await.is_none());
    assert!(person_kept(&api, &company, user::MARC).await.is_none());
    for connector in [id, company] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{connector}"))
                .await
                .status,
            StatusCode::OK
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_authorizes_only_what_they_may_use_while_they_may() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "droitssoi").await;

    // A server the company authorizes, or none at all.
    let (_, company) = declared(&paul, &authority, "droitsent").await;
    let refused = try_person_start(&marc, &company).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    let absent = try_person_start(&marc, &uuid::Uuid::new_v4().to_string()).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND, "{}", absent.text());

    // Someone who may not use connectors: refused, nothing listed. A new
    // account is a member: its role withdrawn first.
    let mut account = create_account(&paul, "sans.connecteurs").await;
    let without = activate(&api, &mut account).await;
    let member = assignment_of(&paul, account.id, role::MEMBER).await;
    let withdrawn = paul.delete(&format!("/admin/assignments/{member}")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    let refused = try_person_start(&without, &id).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    let listed = without
        .get("/account/connector-authorizations")
        .await
        .json();
    assert_eq!(listed["servers"], json!([]));
    // Given the right, then losing it before the return: nothing kept.
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::MEMBER, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    let (url, cookie) = person_start(&without, &id).await;
    let assignment = assignment_of(&paul, account.id, role::MEMBER).await;
    let withdrawn = paul
        .delete(&format!("/admin/assignments/{assignment}"))
        .await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&without, &back, Some(&cookie)).await),
        PERSON_FAILED
    );
    assert!(person_kept(&api, &id, account.id).await.is_none());
    let reason = entries_by(&api, Some(&id), account.id, AUTHORIZED).await;
    assert_eq!(
        reason.last().unwrap().1.as_deref(),
        Some("Connecteurs non autorisés")
    );

    // The declaration changed during the round trip: nothing kept.
    let (url, cookie) = person_start(&marc, &id).await;
    let holder = format!("/admin/connectors/{id}/oauth/holder");
    let changed = paul.put(&holder, json!({ "perPerson": false })).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&marc, &back, Some(&cookie)).await),
        PERSON_FAILED
    );
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    let reason = entries_by(&api, Some(&id), user::MARC, AUTHORIZED).await;
    assert!(
        reason
            .last()
            .unwrap()
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("changée")
    );

    // Disabled during the round trip, then disabled: nothing kept, none
    // started.
    let back_to_each = paul.put(&holder, json!({ "perPerson": true })).await;
    assert_eq!(back_to_each.status, StatusCode::OK);
    let (url, cookie) = person_start(&marc, &id).await;
    let instance = format!("/admin/connectors/{id}");
    let off = paul.patch(&instance, json!({ "enabled": false })).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    let back = api_path(&authority.sent_back(&url).await);
    assert_eq!(
        location(&come_back(&marc, &back, Some(&cookie)).await),
        PERSON_FAILED
    );
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    let reason = entries_by(&api, Some(&id), user::MARC, AUTHORIZED).await;
    assert!(
        reason
            .last()
            .unwrap()
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("désactivé")
    );
    let refused = try_person_start(&marc, &id).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    for connector in [id, company] {
        assert_eq!(
            paul.delete(&format!("/admin/connectors/{connector}"))
                .await
                .status,
            StatusCode::OK
        );
    }
}
