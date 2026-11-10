//! A Google grant let go of while something else is under way (ADR-194):
//! the request that let it go abandoned while Google keeps the revocation
//! waiting, the revocation going on to its bound and its audit all the
//! same; and a link returning from Google while its connector is being
//! deleted, never kept behind the deletion's back, the grant its code
//! brought revoked.

mod support;

use std::time::Duration;

use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::account_links::take_account_links_of_connector;
use mimorya_db::repositories::connectors::delete_connector_instance;
use support::account_links::{
    FAILED, LINKS, come_back, google_person, insert_google_workspace, kept_refresh_token, start,
};
use support::identity_provider::MockIdentityProvider;
use support::link_revocations::{
    domain_of, last_entry, link_google, no_token_audited, remove_connectors, revocations,
};
use support::locks::wait_for_waiters;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};
use tokio::time::{Instant, sleep, timeout};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_removal_its_request_abandons_is_revoked_and_audited_all_the_same() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    let since = last_entry(&api).await;

    // Google keeps the revocation waiting; the request is given up.
    idp.hold_revocations();
    let path = format!("{LINKS}/{google}");
    let given_up = timeout(Duration::from_secs(1), claire.delete(&path)).await;
    assert!(given_up.is_err(), "answered before the revocation's bound");
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);

    // Asked all the same, its bound reached, and audited without a token.
    let deadline = Instant::now() + Duration::from_secs(30);
    let audited = loop {
        let audited = revocations(&api, since).await;
        if !audited.is_empty() || Instant::now() > deadline {
            break audited;
        }
        sleep(Duration::from_millis(250)).await;
    };
    assert_eq!(idp.revocations_asked(), 1);
    let (by, outcome, _, reason, counts) = audited.first().expect("audited");
    assert_eq!(*by, Some(user::CLAIRE));
    assert_eq!(outcome, "failure");
    assert!(
        reason.as_deref().unwrap_or_default().contains("à temps"),
        "{reason:?}"
    );
    assert_eq!(
        *counts,
        serde_json::json!({ "revoked": 0, "kept": 0, "failed": 1 })
    );
    no_token_audited(&api).await;
    remove_connectors(&api, &[google]).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_returning_while_its_connector_is_deleted_is_never_kept() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    idp.sign_in_as(google_person(
        "g-claire",
        &address,
        Some(&domain_of(&address)),
    ));
    let (url, cookie) = start(&claire, google).await;
    let back = idp.authorize(&url);

    // The administration deletes the connector: its links taken to be
    // revoked, the connector not gone yet.
    let mut deleting = api.app.services.db.begin(tenant::ID).await.unwrap();
    deleting.lock_administration().await.unwrap();
    let taken = take_account_links_of_connector(&mut deleting, google)
        .await
        .unwrap();
    assert!(taken.is_empty());
    // Claire comes back meanwhile: her link waits behind the deletion,
    // then finds the connector gone.
    let returned = come_back(&claire, &back, Some(&cookie));
    let deleted = async {
        wait_for_waiters(&api, 1).await;
        assert!(
            delete_connector_instance(&mut deleting, google)
                .await
                .unwrap()
        );
        deleting.commit().await.unwrap();
    };
    let (returned, ()) = tokio::join!(returned, deleted);
    assert_eq!(location(&returned), FAILED);
    assert_eq!(kept_refresh_token(&api, google, user::CLAIRE).await, None);
    // The grant its code brought, kept by no link, revoked at Google.
    assert_eq!(idp.revoked(), ["jeton-de-renouvellement-g-claire"]);
}
