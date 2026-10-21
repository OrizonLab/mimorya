//! Declaring the company's identity providers (`settings:manage`, ADR-063):
//! checked before anything is written, the provider reached first; the
//! client secret sealed, never returned nor audited; changes and removals
//! audited; reserved to the administrators of the settings.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::identity_provider::{CLIENT_SECRET, MockIdentityProvider};
use support::llm::MASTER_KEY;
use support::refusals::latest_entry;
use support::sso::{DOMAIN, PROVIDERS, SSO_DEPLOYMENT, declaration, declare, remove};
use support::{TestApi, WEB_ORIGIN};

/// The audit entries of identity providers after `since`, in order:
/// action, metadata.
async fn audited(api: &TestApi, since: i64) -> Vec<(String, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_as(
        "select action, metadata from audit_log \
         where id > $1 and action like 'identity_provider.%' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_client_secret_is_sealed_and_never_comes_back() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let id = declare(&paul, declaration(&idp)).await;
    let path = format!("{PROVIDERS}/{id}");

    let view = paul.get(PROVIDERS).await;
    assert_eq!(view.status, StatusCode::OK);
    assert!(!view.text().contains(CLIENT_SECRET));
    let view = view.json();
    assert_eq!(view["secretsAvailable"], true);
    assert_eq!(
        view["redirectUri"],
        format!("{WEB_ORIGIN}/api/auth/sso/callback")
    );
    let provider = &view["providers"][0];
    assert_eq!(provider["kind"], "oidc");
    assert_eq!(provider["issuer"], idp.issuer.as_str());
    assert_eq!(provider["domains"], json!([DOMAIN]));
    assert_eq!(provider["enabled"], true);
    assert_eq!(provider["secretStored"], true);

    let changed = paul
        .patch(
            &path,
            json!({
                "name": "Comptes Néo",
                "clientSecret": "nouveau-secret-0123456789",
                "domains": ["Neo-Maintenance.example", "neo.example"],
            }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let provider = paul.get(PROVIDERS).await.json()["providers"][0].clone();
    assert_eq!(provider["name"], "Comptes Néo");
    assert_eq!(provider["domains"], json!([DOMAIN, "neo.example"]));

    // Sealed in the company's secrets, never in clear.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let stored: Vec<Vec<u8>> = sqlx::query_scalar("select ciphertext from secrets")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(!stored.is_empty());
    for sealed in &stored {
        assert!(!sealed.windows(6).any(|w| w == b"secret"));
    }

    remove(&paul, &id).await;
    assert_eq!(paul.get(PROVIDERS).await.json()["providers"], json!([]));
    let audit = audited(&api, since).await;
    let actions: Vec<&str> = audit.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        [
            "identity_provider.created",
            "identity_provider.updated",
            "identity_provider.deleted"
        ]
    );
    assert_eq!(
        audit[0].1["clientId"],
        support::identity_provider::CLIENT_ID
    );
    assert_eq!(audit[1].1["secretReplaced"], true);
    for (_, metadata) in &audit {
        let text = metadata.to_string();
        assert!(
            !text.contains(CLIENT_SECRET) && !text.contains("nouveau-secret"),
            "{text}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_declaration_is_checked_before_anything_is_written() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let base = declaration(&idp);
    let with = |changes: Value| {
        let mut body = base.clone();
        for (key, value) in changes.as_object().unwrap() {
            if value.is_null() {
                body.as_object_mut().unwrap().remove(key);
            } else {
                body[key] = value.clone();
            }
        }
        body
    };
    for (body, message) in [
        (with(json!({ "kind": "saml" })), "Type de fournisseur"),
        (
            with(json!({ "kind": "microsoft", "issuer": null })),
            "indiquez l’identifiant de l’annuaire",
        ),
        (
            with(
                json!({ "kind": "microsoft", "issuer": null, "directoryId": "contoso.onmicrosoft.com" }),
            ),
            "GUID",
        ),
        (with(json!({ "kind": "google" })), "Google : ni annuaire"),
        (
            with(json!({ "issuer": "https://accounts.google.com" })),
            "celui de Google Workspace, choisissez ce type",
        ),
        (with(json!({ "name": null })), "Donnez un nom"),
        (with(json!({ "domains": [] })), "de 1 à 20 domaines"),
        (with(json!({ "domains": ["neo"] })), "n’est pas un domaine"),
        (with(json!({ "clientSecret": "" })), "secret client"),
        (with(json!({ "clientId": "a b" })), "identifiant client"),
        (
            with(json!({ "issuer": "http://127.0.0.1:9/inexistant" })),
            "Fournisseur injoignable",
        ),
    ] {
        let refused = paul.post(PROVIDERS, body).await;
        assert_eq!(
            refused.status,
            StatusCode::BAD_REQUEST,
            "{}",
            refused.text()
        );
        assert!(
            refused.error_message().contains(message),
            "{message}: {}",
            refused.error_message()
        );
    }
    assert_eq!(paul.get(PROVIDERS).await.json()["providers"], json!([]));

    // The same client twice, or a domain another provider covers.
    let id = declare(&paul, base.clone()).await;
    let twice = paul.post(PROVIDERS, base.clone()).await;
    assert_eq!(twice.status, StatusCode::CONFLICT, "{}", twice.text());
    let other_client = with(json!({ "clientId": "autre-client" }));
    let covered = paul.post(PROVIDERS, other_client).await;
    assert_eq!(covered.status, StatusCode::CONFLICT, "{}", covered.text());
    assert!(covered.error_message().contains(DOMAIN));
    assert_eq!(
        paul.get(PROVIDERS).await.json()["providers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    remove(&paul, &id).await;

    // An address in clear, or of the private network, on a server that
    // does not allow it.
    let public = TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await;
    let admin = public.sign_in(user::PAUL).await;
    let refused = admin.post(PROVIDERS, base).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(refused.error_message().contains("Adresse de l’émetteur"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_administrator_of_the_settings_manages_providers() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let id = declare(&paul, declaration(&idp)).await;
    let path = format!("{PROVIDERS}/{id}");

    assert_eq!(marc.get(PROVIDERS).await.status, StatusCode::FORBIDDEN);
    let created = marc.post(PROVIDERS, declaration(&idp)).await;
    assert_eq!(created.status, StatusCode::FORBIDDEN);
    let changed = marc.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(changed.status, StatusCode::FORBIDDEN);
    assert_eq!(marc.delete(&path).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        paul.get(PROVIDERS).await.json()["providers"][0]["enabled"],
        true
    );
    // Strict bodies: kind, issuer and client do not change.
    let fixed = paul
        .patch(&path, json!({ "issuer": "https://ailleurs.example" }))
        .await;
    assert_eq!(fixed.status, StatusCode::BAD_REQUEST, "{}", fixed.text());
    remove(&paul, &id).await;
    let absent = paul.delete(&path).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
}
