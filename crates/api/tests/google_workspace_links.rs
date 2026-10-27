//! A person links their own Google Workspace account at their company's
//! client (ADR-121): Gmail and Calendar permissions asked, offline access
//! and consent asked Google's way, their domain offered; only an address
//! Google verified in the account's Workspace domain, theirs, is linked;
//! the link renews what was granted, and each of the person's links, at
//! Microsoft and at Google, keeps its own token.

mod support;

use std::collections::HashMap;

use axum::http::StatusCode;
use mimorya_api::services::account_links::PersonAccounts;
use mimorya_core::account_links::{
    CALENDAR_EVENTS, CALENDAR_READONLY, GMAIL_READONLY, GMAIL_SEND, GOOGLE_WORKSPACE, MICROSOFT365,
};
use mimorya_core::demo::{tenant, user};
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use serde_json::{Value, json};
use support::account_links::{
    FAILED, LINKED, LINKS, OTHER_ACCOUNT, come_back, google_person, insert_google_workspace,
    insert_microsoft365, kept_refresh_token, link, link_entries, links, microsoft_person, start,
    try_start,
};
use support::identity_provider::{MockIdentityProvider, Person};
use support::refusals::latest_entry;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{Client, TestApi, email};

/// The person's link at the connector of that type, as listed to them.
async fn listed(person: &Client<'_>, connector_type: &str) -> Value {
    links(person)
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["type"] == connector_type)
        .cloned()
        .unwrap_or(Value::Null)
}

fn domain_of(address: &str) -> String {
    address.rsplit_once('@').unwrap().1.to_string()
}

/// The query of the address the browser is sent to.
fn asked(url: &str) -> HashMap<String, String> {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_workspace_account() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let connector = insert_google_workspace(&api, &idp, true, false).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    assert_eq!(
        listed(&claire, GOOGLE_WORKSPACE).await,
        json!({
            "connectorId": connector, "name": "Google Workspace", "type": "google_workspace",
            "available": true, "writeTools": false, "linked": null,
        })
    );

    // Gmail and Calendar read, offline access and consent asked Google's
    // way, her domain offered, her address as a hint.
    let (url, cookie) = start(&claire, connector).await;
    let query = asked(&url);
    let scopes: Vec<&str> = query["scope"].split(' ').collect();
    assert_eq!(
        scopes,
        [
            "openid",
            "email",
            "profile",
            GMAIL_READONLY,
            CALENDAR_READONLY
        ]
    );
    assert_eq!(query["access_type"], "offline");
    assert_eq!(query["prompt"], "consent");
    assert_eq!(query["hd"], domain_of(&address));
    assert_eq!(query["login_hint"], address);

    let since = latest_entry(&api).await;
    let back = come_back(&claire, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(location(&back), LINKED);
    let linked = listed(&claire, GOOGLE_WORKSPACE).await["linked"].clone();
    assert_eq!(linked["address"], json!(address));
    assert_eq!(linked["can"], json!([GMAIL_READONLY, CALENDAR_READONLY]));
    assert_eq!(linked["toGrant"], json!([]));
    let subject = format!("sujet-{address}");
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        Some(MockIdentityProvider::refresh_token_of(&subject))
    );
    let entries = link_entries(&api, since).await;
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(
        (entries[0].0.as_str(), entries[0].1.as_str()),
        ("account.link.created", "success")
    );

    // Renewed with what she granted: Google is named no permission.
    let accounts = PersonAccounts::new(
        api.app.services.account_links.clone(),
        tenant::ID,
        user::CLAIRE,
    );
    let token = format!("acces-{subject}-1");
    assert_eq!(
        accounts
            .access_token(GOOGLE_WORKSPACE, &[GMAIL_READONLY])
            .await,
        Ok(token.clone())
    );
    assert_eq!(idp.renewals(), [""]);
    assert_eq!(
        accounts.access_token(GOOGLE_WORKSPACE, &[GMAIL_SEND]).await,
        Err(AccountAccessError::NotGranted(vec![GMAIL_SEND.into()]))
    );
    // No application at Microsoft: said of Microsoft's.
    match accounts.access_token(MICROSOFT365, &[]).await {
        Err(AccountAccessError::Unavailable(why)) => assert_eq!(
            why,
            "L’application Microsoft 365 de l’entreprise n’est pas active"
        ),
        other => panic!("{other:?}"),
    }

    // Its tools let write: sending and events asked, until linked again.
    let paul = api.sign_in(user::PAUL).await;
    let path = format!("/admin/connectors/{connector}");
    let let_write = paul.patch(&path, json!({ "writeTools": true })).await;
    assert_eq!(let_write.status, StatusCode::OK, "{}", let_write.text());
    let linked = listed(&claire, GOOGLE_WORKSPACE).await["linked"].clone();
    assert_eq!(linked["toGrant"], json!([GMAIL_SEND, CALENDAR_EVENTS]));

    // Disabled: no new link, and nothing asked of the one she keeps.
    let disabled = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let refused = try_start(&claire, connector).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());
    let kept = listed(&claire, GOOGLE_WORKSPACE).await;
    assert_eq!(kept["available"], json!(false));
    assert_eq!(kept["linked"]["toGrant"], json!([]));
    assert_eq!(kept["linked"]["can"], json!([]));

    // Enabled again: linked again, she grants them.
    let enabled = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let (url, _) = start(&claire, connector).await;
    let scopes = asked(&url)["scope"].clone();
    assert!(
        scopes.ends_with(&format!("{GMAIL_SEND} {CALENDAR_EVENTS}")),
        "{scopes}"
    );
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let linked = listed(&claire, GOOGLE_WORKSPACE).await["linked"].clone();
    assert_eq!(linked["toGrant"], json!([]));
    assert_eq!(
        linked["can"],
        json!([
            GMAIL_READONLY,
            CALENDAR_READONLY,
            GMAIL_SEND,
            CALENDAR_EVENTS
        ])
    );
    let removed = paul.delete(&path).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_persons_own_workspace_account_is_linked() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let connector = insert_google_workspace(&api, &idp, true, false).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    let domain = domain_of(&address);
    let since = latest_entry(&api).await;

    // A colleague's account.
    let paul = email(user::PAUL);
    idp.sign_in_as(google_person("g-paul", &paul, Some(&domain_of(&paul))));
    assert_eq!(
        location(&link(&claire, &idp, connector).await),
        OTHER_ACCOUNT
    );
    // Her address, but a consumer account, one in another Workspace domain,
    // or one whose address Google did not verify.
    let unverified = Person {
        email_verified: Some(false),
        ..google_person("g-3", &address, Some(&domain))
    };
    for person in [
        google_person("g-1", &address, None),
        google_person("g-2", &address, Some("autre.example")),
        unverified,
    ] {
        idp.sign_in_as(person);
        assert_eq!(location(&link(&claire, &idp, connector).await), FAILED);
    }
    assert_eq!(
        kept_refresh_token(&api, connector, user::CLAIRE).await,
        None
    );
    let reasons: Vec<_> = link_entries(&api, since)
        .await
        .into_iter()
        .map(|(action, outcome, _, reason, _)| (action, outcome, reason.unwrap_or_default()))
        .collect();
    let denied = |reason: &str| {
        (
            "account.link.created".to_string(),
            "denied".to_string(),
            reason.to_string(),
        )
    };
    let unidentified = denied("Jeton sans compte vérifié du domaine Workspace");
    assert_eq!(
        reasons,
        [
            denied("Compte Google d’une autre adresse"),
            unidentified.clone(),
            unidentified.clone(),
            unidentified
        ]
    );

    // Her own, in her domain, whatever its case.
    idp.sign_in_as(google_person(
        "g-claire",
        &address.to_uppercase(),
        Some(&domain.to_uppercase()),
    ));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let linked = listed(&claire, GOOGLE_WORKSPACE).await["linked"].clone();
    assert_eq!(linked["address"], json!(address));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_link_of_a_person_keeps_its_own_token() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let microsoft = insert_microsoft365(&api, &idp, true).await;
    let google = insert_google_workspace(&api, &idp, true, false).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    idp.sign_in_as(microsoft_person("objet-claire", &address));
    assert_eq!(location(&link(&claire, &idp, microsoft).await), LINKED);
    idp.sign_in_as(google_person(
        "g-claire",
        &address,
        Some(&domain_of(&address)),
    ));
    assert_eq!(location(&link(&claire, &idp, google).await), LINKED);
    let names: Vec<Value> = links(&claire)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].clone())
        .collect();
    assert_eq!(names, [json!("Google Workspace"), json!("Microsoft 365")]);

    let accounts = PersonAccounts::new(
        api.app.services.account_links.clone(),
        tenant::ID,
        user::CLAIRE,
    );
    let at_microsoft = Ok("acces-sujet-objet-claire-1".to_string());
    let at_google = Ok("acces-g-claire-2".to_string());
    assert_eq!(accounts.access_token(MICROSOFT365, &[]).await, at_microsoft);
    assert_eq!(
        accounts.access_token(GOOGLE_WORKSPACE, &[]).await,
        at_google
    );
    // Each kept for its own link.
    assert_eq!(accounts.access_token(MICROSOFT365, &[]).await, at_microsoft);
    assert_eq!(
        accounts.access_token(GOOGLE_WORKSPACE, &[]).await,
        at_google
    );
    let renewals = idp.renewals();
    assert_eq!(renewals.len(), 2, "{renewals:?}");
    // Microsoft is named what she granted; Google nothing.
    assert!(renewals[0].contains("Mail.Read"), "{renewals:?}");
    assert_eq!(renewals[1], "");

    // A token refused: hers are renewed at each provider.
    accounts.rejected(GOOGLE_WORKSPACE).await;
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[]).await,
        Ok("acces-sujet-objet-claire-3".to_string())
    );
    assert_eq!(
        accounts.access_token(GOOGLE_WORKSPACE, &[]).await,
        Ok("acces-g-claire-4".to_string())
    );

    // Her Google link removed: her Microsoft one still serves.
    let removed = claire.delete(&format!("{LINKS}/{google}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(
        accounts.access_token(GOOGLE_WORKSPACE, &[]).await,
        Err(AccountAccessError::NotLinked)
    );
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[]).await,
        Ok("acces-sujet-objet-claire-3".to_string())
    );
    let paul = api.sign_in(user::PAUL).await;
    for connector in [microsoft, google] {
        let removed = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}
