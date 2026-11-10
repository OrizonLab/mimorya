//! A Google account's grant revoked at Google when the platform lets go of
//! its link (ADR-194): removed by the person, their account disabled, the
//! connector deleted; sent to the revocation endpoint Google's discovery
//! names, audited counted, never a token. A provider that refuses or names
//! no endpoint keeps the grant, said; the removal stands. Microsoft revokes
//! no single grant: nothing is sent there.

mod support;

use axum::http::{Method, StatusCode};
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::account_links::{
    LINKED, LINKS, insert_google_workspace, insert_microsoft365, kept_refresh_token, link,
    microsoft_person,
};
use support::accounts::{activate, create_account};
use support::identity_provider::MockIdentityProvider;
use support::link_revocations::{
    last_entry, link_google, link_google_as, no_token_audited, remove_connectors, revocations,
};
use support::scim::{made, patch_of, send};
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_google_link_removed_is_revoked_at_google_and_a_microsoft_one_is_not() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let microsoft = insert_microsoft365(&api, &idp, true).await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, microsoft).await), LINKED);
    let since = last_entry(&api).await;

    // Her Google link removed: its grant revoked at Google, by the time
    // she is answered.
    let removed = claire.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-claire"]);
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);
    assert_eq!(
        revocations(&api, since).await,
        [(
            Some(user::CLAIRE),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": 1, "failed": 0 }),
        )]
    );

    // Her Microsoft link removed: Microsoft revokes no single grant,
    // nothing is sent.
    let removed = claire.delete(&format!("{LINKS}/{microsoft}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(idp.revoked().len(), 1);
    assert_eq!(revocations(&api, since).await.len(), 1);
    no_token_audited(&api).await;
    remove_connectors(&api, &[microsoft, google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_grant_google_does_not_revoke_is_said_and_the_link_still_goes() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    let marc = link_google(&api, &idp, google, user::MARC, "g-marc").await;
    let since = last_entry(&api).await;

    // Google refuses: the link goes all the same, the refusal said by its
    // code alone.
    idp.refuse_revocations();
    let removed = claire.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);
    assert!(idp.revoked().is_empty());

    // No endpoint named: said too.
    idp.name_no_revocation_endpoint();
    let removed = marc.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(kept_refresh_token(&api, google, user::MARC).await, None);
    let failed = json!({ "revoked": 0, "failed": 1 });
    assert_eq!(
        revocations(&api, since).await,
        [
            (
                Some(user::CLAIRE),
                "failure".to_string(),
                Some(google.to_string()),
                Some("Google a refusé la révocation (temporarily_unavailable)".to_string()),
                failed.clone(),
            ),
            (
                Some(user::MARC),
                "failure".to_string(),
                Some(google.to_string()),
                Some(
                    "Google ne nomme aucun point de révocation : retirez l’accord dans le \
                     compte Google"
                        .to_string()
                ),
                failed,
            ),
        ]
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_and_a_deleted_connector_revoke_their_google_grants() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    link_google(&api, &idp, google, user::NADIA, "g-nadia").await;
    link_google(&api, &idp, google, user::MARC, "g-marc").await;
    let paul = api.sign_in(user::PAUL).await;
    let since = last_entry(&api).await;

    // Nadia leaves: her grant revoked, audited as let go by Paul.
    let account = format!("/admin/users/{}", user::NADIA);
    let left = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(left.status, StatusCode::OK, "{}", left.text());
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-nadia"]);
    let back = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());

    // The connector deleted: everyone else's revoked.
    remove_connectors(&api, &[google]).await;
    let mut revoked = idp.revoked();
    revoked.sort();
    assert_eq!(
        revoked,
        [
            "jeton-de-renouvellement-g-claire",
            "jeton-de-renouvellement-g-marc",
            "jeton-de-renouvellement-g-nadia",
        ]
    );
    let entries = revocations(&api, since).await;
    let by_paul = |count: u64| {
        (
            Some(user::PAUL),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": count, "failed": 0 }),
        )
    };
    assert_eq!(entries, [by_paul(1), by_paul(2)]);
    no_token_audited(&api).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_the_provider_disables_through_scim_has_its_google_grant_revoked() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let paul = api.sign_in(user::PAUL).await;
    let mut account = create_account(&paul, "scim.google").await;
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::MEMBER, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    let leaving = activate(&api, &mut account).await;
    link_google_as(&leaving, &idp, google, &account.email, "g-scim").await;
    let since = last_entry(&api).await;

    // The identity provider disables the account: its grant revoked at
    // Google, audited as let go by the platform.
    let (_, secret) = made(&paul, "Okta").await;
    let off = patch_of(json!([{ "op": "Replace", "path": "active", "value": "False" }]));
    let path = format!("/Users/{}", account.id);
    let disabled = send(&api, Some(&secret), Method::PATCH, &path, &off).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-scim"]);
    assert_eq!(kept_refresh_token(&api, google, account.id).await, None);
    assert_eq!(
        revocations(&api, since).await,
        [(
            None,
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": 1, "failed": 0 }),
        )]
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}
