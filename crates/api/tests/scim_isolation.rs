//! What a SCIM token reaches (ADR-183): its company's accounts only, never
//! another's, by identifier, address or page; nothing while its company is
//! suspended. A session proves nothing to SCIM, header or not, to read,
//! create, change or remove an account; the administration's routes keep
//! asking for the header. No account is created at an address of a domain
//! another company proved (ADR-069).

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use mimorya_core::demo::user;
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept};
use support::queue::as_owner;
use support::scim::{SCIM_JSON, TOKENS, create, entra_user, get, made, patch_of, refusal, scim_as};
use support::{TestApi, companies};

/// The addresses of every account the token `secret` lists.
async fn addresses(api: &TestApi, secret: &str) -> Vec<String> {
    let listed = get(api, Some(secret), "/Users").await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["Resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|user| user["userName"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_reaches_its_company_s_accounts_only() {
    let api = TestApi::start().await;
    let address = "jeanne.martin@acme-scim.example";
    let company = companies::create(&api, "acme-scim", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, theirs) = made(&jeanne, "Okta Acme").await;
    let (_, ours) = made(&paul, "Entra Néo").await;

    // Each administration sees and removes its own tokens only.
    let jeannes = jeanne.get(TOKENS).await.json()["tokens"][0]["id"].clone();
    let listed = paul.get(TOKENS).await.json();
    assert!(
        !listed["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == jeannes)
    );
    let removal = paul
        .delete(&format!("{TOKENS}/{}", jeannes.as_str().unwrap()))
        .await;
    assert_eq!(removal.status, StatusCode::NOT_FOUND, "{}", removal.text());
    assert_eq!(
        get(&api, Some(&theirs), "/Users").await.status,
        StatusCode::OK
    );

    // An address another company's account holds is taken here too.
    let taken = create(&api, Some(&ours), &entra_user(address, "jeanne")).await;
    assert_eq!(refusal(&taken), (StatusCode::CONFLICT, json!("uniqueness")));

    // Each token lists its company's accounts.
    assert_eq!(addresses(&api, &theirs).await, [address]);
    let neo = addresses(&api, &ours).await;
    assert!(neo.contains(&"paul.renaud@neo-maintenance.example".to_string()));
    assert!(!neo.contains(&address.to_string()));

    // Nor by identifier, nor by address, nor by the provider's identifier.
    let paul_path = format!("/Users/{}", user::PAUL);
    assert_eq!(
        get(&api, Some(&theirs), &paul_path).await.status,
        StatusCode::NOT_FOUND
    );
    let by_address = "/Users?filter=userName%20eq%20%22paul.renaud%40neo-maintenance.example%22";
    assert_eq!(
        get(&api, Some(&theirs), by_address).await.json()["totalResults"],
        0
    );
    let made_here = create(
        &api,
        Some(&ours),
        &entra_user("lucie@neo-maintenance.example", "partage"),
    )
    .await;
    assert_eq!(
        made_here.status,
        StatusCode::CREATED,
        "{}",
        made_here.text()
    );
    let by_external = "/Users?filter=externalId%20eq%20%22partage%22";
    assert_eq!(
        get(&api, Some(&theirs), by_external).await.json()["totalResults"],
        0
    );
    assert_eq!(
        get(&api, Some(&ours), by_external).await.json()["totalResults"],
        1
    );

    // The same identifier, given by another company's provider, is theirs.
    let made_there = create(
        &api,
        Some(&theirs),
        &entra_user("lucie@acme-scim.example", "partage"),
    )
    .await;
    assert_eq!(
        made_there.status,
        StatusCode::CREATED,
        "{}",
        made_there.text()
    );
    let lucie = made_there.json();
    assert!(
        addresses(&api, &theirs)
            .await
            .contains(&"lucie@acme-scim.example".to_string())
    );
    let lucie_path = format!("/Users/{}", lucie["id"].as_str().unwrap());
    assert_eq!(
        get(&api, Some(&ours), &lucie_path).await.status,
        StatusCode::NOT_FOUND
    );
    assert!(
        !addresses(&api, &ours)
            .await
            .contains(&"lucie@acme-scim.example".to_string())
    );

    // Their company suspended: their token reaches nothing, ours still does.
    let status = "update tenants set status = $2 where id = $1";
    as_owner(&status.replace("$2", "'suspended'"), company.tenant_id).await;
    let suspended = get(&api, Some(&theirs), "/Users").await;
    assert_eq!(refusal(&suspended), (StatusCode::UNAUTHORIZED, Value::Null));
    let creation = create(
        &api,
        Some(&theirs),
        &entra_user("tard@acme-scim.example", "tard"),
    )
    .await;
    assert_eq!(creation.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(&api, Some(&ours), "/Users").await.status,
        StatusCode::OK
    );
    as_owner(&status.replace("$2", "'active'"), company.tenant_id).await;
    assert_eq!(
        get(&api, Some(&theirs), "/Users").await.status,
        StatusCode::OK
    );
    assert!(
        !addresses(&api, &theirs)
            .await
            .contains(&"tard@acme-scim.example".to_string())
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn no_account_is_made_in_a_domain_another_company_proved() {
    let api = TestApi::start().await;
    let address = "jeanne.martin@acme-domaine.example";
    let company = companies::create(&api, "acme-domaine", address).await;
    let accepted = accept(&api, &company.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.sign_in_with(address, INVITED_PASSWORD).await;
    // Their domain, proved (ADR-069).
    let proved =
        "insert into verified_domains (tenant_id, domain) values ($1, 'acme-domaine.example')";
    as_owner(proved, company.tenant_id).await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, ours) = made(&paul, "Entra Néo").await;

    let refused = create(
        &api,
        Some(&ours),
        &entra_user("pirate@acme-domaine.example", "pirate"),
    )
    .await;
    assert_eq!(
        refusal(&refused),
        (StatusCode::CONFLICT, json!("uniqueness"))
    );
    assert!(
        refused
            .text()
            .contains("domaine prouvé par une autre entreprise"),
        "{}",
        refused.text()
    );
    let (_, theirs) = made(&jeanne, "Okta Acme").await;
    assert_eq!(addresses(&api, &theirs).await, [address]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_session_proves_nothing_to_scim() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, secret) = made(&paul, "Entra Néo").await;
    let before = get(&api, Some(&secret), "/Users?count=0").await.json()["totalResults"].clone();

    // Paul's session, with the header the browser's changes carry: refused,
    // nothing created.
    let read = paul.get("/scim/v2/Users").await;
    assert_eq!(read.status, StatusCode::UNAUTHORIZED, "{}", read.text());
    let sent = paul
        .post(
            "/scim/v2/Users",
            entra_user("session@neo-maintenance.example", "s"),
        )
        .await;
    assert_eq!(sent.status, StatusCode::UNAUTHORIZED, "{}", sent.text());
    let after = get(&api, Some(&secret), "/Users?count=0").await.json()["totalResults"].clone();
    assert_eq!(after, before);

    // The provider's change goes without the header; the administration's
    // routes still ask for it, even under a path that reads like SCIM's.
    let body = Some((
        SCIM_JSON,
        entra_user("sans.entete@neo-maintenance.example", "e").to_string(),
    ));
    let created = scim_as(&api, Some(&secret), Method::POST, "/Users", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    // Paul's session changes no account through SCIM either.
    let account = format!("/Users/{}", created.json()["id"].as_str().unwrap());
    let disabling = patch_of(json!([{ "op": "replace", "path": "active", "value": false }]));
    let scim_path = format!("/scim/v2{account}");
    for refused in [
        paul.patch(&scim_path, disabling).await,
        paul.put(&scim_path, json!({ "active": false })).await,
        paul.delete(&scim_path).await,
    ] {
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{}",
            refused.text()
        );
    }
    let still = get(&api, Some(&secret), &account).await;
    assert_eq!(still.json()["active"], true, "{}", still.text());
    for path in [TOKENS, "/scim/v2", "/scimv2/Users", "/admin/scim/v2/Users"] {
        let request = Request::post(path)
            .header("authorization", format!("Bearer {secret}"))
            .header("content-type", SCIM_JSON)
            .body(Body::from(json!({ "name": "Sans en-tête" }).to_string()))
            .unwrap();
        let refused = api.send(request).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            refused.error_message(),
            "En-tête de requête manquant",
            "{path}"
        );
    }
}
