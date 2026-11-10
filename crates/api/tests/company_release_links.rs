//! A suspended company's release (ADR-193) takes the accounts its people
//! linked at Google and revokes their grants there (ADR-194): its deletion
//! is refused until then, the operator is told how each went, the platform
//! audits it, never a token. The accounts linked at Microsoft, which
//! revokes no single grant, stay until the company goes.

mod support;

use mimorya_api::services::provisioning::{
    CompanyDeletion, CompanyStatusChange, delete_company, release_company,
};
use mimorya_core::demo::{tenant, user};
use mimorya_core::{DomainError, ErrorCode};
use serde_json::Value;
use support::account_links::{
    LINKED, insert_google_workspace, insert_microsoft365, kept_refresh_token, link,
    microsoft_person,
};
use support::identity_provider::MockIdentityProvider;
use support::link_revocations::{
    last_entry, link_google, no_token_audited, remove_connectors, revocations,
};
use support::retention::owner;
use support::sso::{SSO_DEPLOYMENT, location};
use support::suspension::{DemoActiveOnDrop, change};
use support::{TestApi, email};

fn domain(error: anyhow::Error) -> DomainError {
    error.downcast::<DomainError>().expect("domain error")
}

/// What the release audited of itself.
async fn released_entry(api: &TestApi) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entry = sqlx::query_scalar(
        "select metadata from audit_log where action = 'tenant.released' \
         and actor_type = 'system' order by id desc limit 1",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    entry
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_released_company_s_google_grants_are_revoked_at_google() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let _active = DemoActiveOnDrop;
    let idp = MockIdentityProvider::start().await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let microsoft = insert_microsoft365(&api, &idp, true).await;
    let claire = link_google(&api, &idp, google, user::CLAIRE, "g-claire").await;
    link_google(&api, &idp, google, user::MARC, "g-marc").await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, microsoft).await), LINKED);
    let services = &api.app.services;

    // Suspended, its deletion is refused while its Google grants stand.
    change(&api, tenant::SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    let owner = owner().await;
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.to_release, 2);
    let refused = delete_company(&owner, tenant::SLUG, Some(tenant::SLUG)).await;
    let refused = domain(refused.unwrap_err());
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(
        refused.message.contains("comptes Google reliés"),
        "{}",
        refused.message
    );

    // Released: each revoked at Google before the operator is answered.
    let since = last_entry(&api).await;
    let released = release_company(services, tenant::SLUG).await.unwrap();
    assert!(released.servers.is_empty() && released.bots.is_empty());
    let links: Vec<(&str, usize, usize, usize)> = (released.links.iter())
        .map(|l| (l.name.as_str(), l.revoked, l.kept, l.failed))
        .collect();
    assert_eq!(links, [("Google Workspace", 2, 0, 0)]);
    let mut revoked = idp.revoked();
    revoked.sort();
    assert_eq!(
        revoked,
        [
            "jeton-de-renouvellement-g-claire",
            "jeton-de-renouvellement-g-marc"
        ]
    );
    for who in [user::CLAIRE, user::MARC] {
        assert_eq!(kept_refresh_token(&api, google, who).await, None);
    }
    // Microsoft's stays: it revokes no single grant, and it goes with the
    // company.
    assert!(
        kept_refresh_token(&api, microsoft, user::CLAIRE)
            .await
            .is_some()
    );
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.to_release, 0);

    // Audited by the platform, counted, never a token.
    assert_eq!(released_entry(&api).await["accountLinks"], 2);
    let audited = revocations(&api, since).await;
    assert_eq!(audited.len(), 1);
    let (by, outcome, connector, _, counts) = &audited[0];
    assert_eq!((by, outcome.as_str()), (&None, "success"));
    assert_eq!(connector.as_deref(), Some(google.to_string().as_str()));
    assert_eq!(
        *counts,
        serde_json::json!({ "revoked": 2, "kept": 0, "failed": 0 })
    );
    no_token_audited(&api).await;

    // Nothing is left for a second release.
    let again = release_company(services, tenant::SLUG).await.unwrap();
    assert!(again.links.is_empty());
    change(&api, tenant::SLUG, CompanyStatusChange::Reactivate)
        .await
        .unwrap();
    remove_connectors(&api, &[google, microsoft]).await;
    owner.close().await;
}
