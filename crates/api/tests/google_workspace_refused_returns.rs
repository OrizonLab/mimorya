//! A return from Google refused once its code was redeemed leaves a grant
//! no link keeps (ADR-194): revoked at Google, as a link let go of is,
//! unless the person has a link to the same account at the connector
//! already, which the same grant serves: then it is kept, said so in the
//! audit. Another account's grant is revoked all the same.

mod support;

use mimorya_core::demo::user;
use serde_json::json;
use support::account_links::{
    FAILED, OTHER_ACCOUNT, come_back, google_person, insert_google_workspace, kept_refresh_token,
    link, start,
};
use support::identity_provider::MockIdentityProvider;
use support::link_revocations::{
    domain_of, last_entry, link_google, no_token_audited, remove_connectors, revocations,
};
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_with_another_google_account_has_its_fresh_grant_revoked() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    let domain = domain_of(&email(user::CLAIRE));
    let since = last_entry(&api).await;

    // A colleague's account chosen at Google: refused, its grant revoked,
    // hers kept.
    let other = format!("nadia.colleague@{domain}");
    idp.sign_in_as(google_person("g-autre", &other, Some(&domain)));
    assert_eq!(location(&link(&claire, &idp, google).await), OTHER_ACCOUNT);
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-autre"]);
    assert_eq!(
        kept_refresh_token(&api, google, user::CLAIRE)
            .await
            .as_deref(),
        Some("jeton-de-renouvellement-g-claire")
    );
    assert_eq!(
        revocations(&api, since).await,
        [(
            Some(user::CLAIRE),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": 1, "kept": 0, "failed": 0 }),
        )]
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refused_return_of_the_account_a_link_keeps_keeps_its_grant() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    let since = last_entry(&api).await;

    // Her own account again, its Workspace domain not named this time:
    // refused once the code was redeemed.
    idp.sign_in_as(google_person("g-claire", &email(user::CLAIRE), None));
    let (url, cookie) = start(&claire, google).await;
    let returned = come_back(&claire, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(location(&returned), FAILED);

    // Her link serves the same grant: nothing is sent to Google.
    assert!(
        kept_refresh_token(&api, google, user::CLAIRE)
            .await
            .is_some()
    );
    assert!(idp.revoked().is_empty());
    assert_eq!(
        revocations(&api, since).await,
        [(
            Some(user::CLAIRE),
            "success".to_string(),
            Some(google.to_string()),
            None,
            json!({ "revoked": 0, "kept": 1, "failed": 0 }),
        )]
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}
