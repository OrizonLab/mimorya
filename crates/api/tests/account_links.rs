//! A person links their own Microsoft 365 account at their company's
//! application (ADR-116): the delegated permissions asked, their address as
//! a hint; the return keeps the refresh token sealed for this connector and
//! person, never in clear nor in the audit; another account than theirs is
//! never linked; the return must be the starter's own; the link is theirs
//! to remove, and goes with its connector or their disabled account.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, tenant, user};
use serde_json::json;
use support::account_links::{
    FAILED, LINKED, LINKS, OTHER_ACCOUNT, come_back, insert_microsoft365, kept_refresh_token, link,
    link_entries, links, microsoft_person, start, try_start,
};
use support::accounts::{activate, assignment_of, create_account};
use support::identity_provider::{MockIdentityProvider, Tamper};
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, audit_text_since, location, set_cookie};
use support::{TestApi, email};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_own_mailbox_and_removes_it() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let connector = insert_microsoft365(&api, &idp, true).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    assert_eq!(
        links(&claire).await,
        json!([{
            "connectorId": connector, "name": "Microsoft 365", "type": "microsoft365",
            "available": true, "linked": null,
        }])
    );

    // Delegated permissions only, offline access among them, and her
    // address as a hint.
    let (url, cookie) = start(&claire, connector).await;
    let asked = url::Url::parse(&url).unwrap();
    let query: std::collections::HashMap<_, _> = asked.query_pairs().into_owned().collect();
    let scopes: Vec<&str> = query["scope"].split(' ').collect();
    for scope in ["openid", "offline_access", "Mail.Read", "Calendars.Read"] {
        assert!(scopes.contains(&scope), "{scopes:?}");
    }
    assert_eq!(query["login_hint"], address);
    assert!(query["redirect_uri"].ends_with("/api/account/links/callback"));

    let since = latest_entry(&api).await;
    idp.sign_in_as(microsoft_person("objet-claire", &address.to_uppercase()));
    let back = come_back(&claire, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(location(&back), LINKED);
    let cleared = set_cookie(&back, "mimorya_link").expect("attempt cleared");
    assert!(cleared.contains("Max-Age=0"));
    let listed = links(&claire).await;
    assert_eq!(listed[0]["linked"]["address"], json!(address));
    let granted = listed[0]["linked"]["scopes"].as_array().unwrap();
    assert!(granted.contains(&json!("Mail.Read")), "{granted:?}");

    // Sealed for this connector and person, never kept nor audited in clear.
    let refresh = MockIdentityProvider::refresh_token_of("sujet-objet-claire");
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        Some(refresh.clone())
    );
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let in_clear: bool = sqlx::query_scalar(
        "select exists(select 1 from account_links \
         where position(convert_to($1, 'UTF8') in refresh_ciphertext) > 0)",
    )
    .bind(&refresh)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(!in_clear);
    let entries = link_entries(&api, since).await;
    assert_eq!(entries.len(), 1, "{entries:?}");
    let (action, outcome, by, _, metadata) = &entries[0];
    assert_eq!(
        (action.as_str(), outcome.as_str()),
        ("account.link.created", "success")
    );
    assert_eq!(
        (*by, metadata),
        (Some(user::CLAIRE), &json!({ "replaced": false }))
    );
    let audited = audit_text_since(&api, tenant::ID, since).await;
    assert!(!audited.contains(&refresh) && !audited.contains(&address));

    // Linked again: the link is replaced, not doubled.
    let again = link(&claire, &idp, connector).await;
    assert_eq!(location(&again), LINKED);
    assert_eq!(links(&claire).await.as_array().unwrap().len(), 1);

    // Hers to remove, and then gone.
    let since = latest_entry(&api).await;
    let path = format!("{LINKS}/{connector}");
    let removed = claire.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(links(&claire).await[0]["linked"], json!(null));
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    let entries = link_entries(&api, since).await;
    assert_eq!(entries[0].0, "account.link.removed");
    let twice = claire.delete(&path).await;
    assert_eq!(twice.status, StatusCode::NOT_FOUND, "{}", twice.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_account_than_the_persons_own_is_never_linked() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let connector = insert_microsoft365(&api, &idp, true).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let since = latest_entry(&api).await;

    // A colleague's account she may open, or one the directory vouches for
    // no address of.
    idp.sign_in_as(microsoft_person("objet-paul", &email(user::PAUL)));
    assert_eq!(
        location(&link(&claire, &idp, connector).await),
        OTHER_ACCOUNT
    );
    idp.sign_in_as(microsoft_person("objet-claire", "cmartin"));
    assert_eq!(location(&link(&claire, &idp, connector).await), FAILED);
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    let entries = link_entries(&api, since).await;
    let refused: Vec<_> = entries
        .iter()
        .map(|(action, outcome, by, reason, metadata)| {
            (
                action.as_str(),
                outcome.as_str(),
                *by,
                reason.clone(),
                metadata.clone(),
            )
        })
        .collect();
    assert_eq!(
        refused,
        [
            (
                "account.link.created",
                "denied",
                Some(user::CLAIRE),
                Some("Compte Microsoft d’une autre adresse".to_string()),
                json!({ "otherAccount": true })
            ),
            (
                "account.link.created",
                "denied",
                Some(user::CLAIRE),
                Some("Jeton sans compte vérifié de l’annuaire".to_string()),
                json!({ "otherAccount": false })
            ),
        ]
    );

    // Without offline access, nothing could be read while she is away.
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    idp.tamper(Tamper {
        withhold_refresh_token: true,
        ..Tamper::default()
    });
    assert_eq!(location(&link(&claire, &idp, connector).await), FAILED);
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_return_links_only_for_the_person_who_started_it() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let connector = insert_microsoft365(&api, &idp, true).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let nadia = api.sign_in(user::NADIA).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    let (url, cookie) = start(&claire, connector).await;
    let back = idp.authorize(&url);

    // Another person's session carrying her attempt; no attempt; another
    // attempt's state; the provider's refusal.
    assert_eq!(
        location(&come_back(&nadia, &back, Some(&cookie)).await),
        FAILED
    );
    assert_eq!(location(&come_back(&claire, &back, None).await), FAILED);
    let (_, other) = start(&claire, connector).await;
    assert_eq!(
        location(&come_back(&claire, &back, Some(&other)).await),
        FAILED
    );
    let (url, cookie) = start(&claire, connector).await;
    let state = support::sso::state_of(&url);
    let denied = format!("{LINKS}/callback?error=access_denied&state={state}");
    assert_eq!(
        location(&come_back(&claire, &denied, Some(&cookie)).await),
        FAILED
    );
    assert_eq!(idp.exchanges(), 0);
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    assert_eq!(kept_refresh_token(&api, connector, user::NADIA).await, None);

    // Signed out, nothing is linked: the sign-in page.
    let anonymous = api.anonymous();
    let signed_out = anonymous.get_with(&back, &[("cookie", &cookie)]).await;
    assert_eq!(location(&signed_out), "/login");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_may_use_connectors_links_at_an_enabled_application() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = insert_microsoft365(&api, &idp, true).await;

    // Its right taken away while at Microsoft (the base role): the return
    // links nothing, and nothing more is offered or started.
    let mut account = create_account(&paul, "sans.connecteurs").await;
    let newcomer = activate(&api, &mut account).await;
    let (url, cookie) = start(&newcomer, connector).await;
    let base = assignment_of(&paul, account.id, role::MEMBER).await;
    let revoked = paul.delete(&format!("/admin/assignments/{base}")).await;
    assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.text());
    idp.sign_in_as(microsoft_person("objet-nouveau", &account.email));
    let back = come_back(&newcomer, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(location(&back), FAILED);
    assert_eq!(kept_refresh_token(&api, connector, account.id).await, None);
    assert_eq!(links(&newcomer).await, json!([]));
    let refused = try_start(&newcomer, connector).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());

    // Disabled: no link starts; one made before stays hers to remove.
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let path = format!("/admin/connectors/{connector}");
    let disabled = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let listed = links(&claire).await;
    assert_eq!(listed[0]["available"], json!(false));
    assert_eq!(listed[0]["linked"]["address"], json!(email(user::CLAIRE)));
    let refused = try_start(&claire, connector).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    // An unknown connector reads the same (another company's: `account_link_returns`).
    let unknown = try_start(&claire, uuid::Uuid::new_v4()).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND, "{}", unknown.text());
    let removed = claire.delete(&format!("{LINKS}/{connector}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn links_go_with_their_connector_and_a_disabled_account() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let connector = insert_microsoft365(&api, &idp, true).await;
    for (who, object) in [(user::CLAIRE, "objet-claire"), (user::NADIA, "objet-nadia")] {
        let person = api.sign_in(who).await;
        idp.sign_in_as(microsoft_person(object, &email(who)));
        assert_eq!(location(&link(&person, &idp, connector).await), LINKED);
    }

    // Nadia leaves: her link goes, Claire's stays.
    let account = format!("/admin/users/{}", user::NADIA);
    let left = paul.patch(&account, json!({ "status": "disabled" })).await;
    assert_eq!(left.status, StatusCode::OK, "{}", left.text());
    assert_eq!(kept_refresh_token(&api, connector, user::NADIA).await, None);
    assert!(
        kept_refresh_token(&api, connector, user::CLAIRE)
            .await
            .is_some()
    );
    let back = paul.patch(&account, json!({ "status": "active" })).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());

    // The application removed, every link goes with it.
    let removed = paul.delete(&format!("/admin/connectors/{connector}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
}
