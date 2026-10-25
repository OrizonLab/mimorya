//! Companies created by the operator (`mimorya-admin`, ADR-033): a real
//! company starts with its root unit, the platform roles and one invited
//! administrator, who manages the workspace without any business access and
//! reaches nothing of another company; a slug or an address already taken
//! leaves nothing behind; an invalid request changes nothing.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::provisioning::{NewCompany, create_company};
use mimorya_core::demo::{tenant, user};
use mimorya_core::{DomainError, ErrorCode};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::{company, create};
use support::mail::token_of;
use support::{TestApi, WEB_ORIGIN, email};

async fn refused(api: &TestApi, input: NewCompany) -> DomainError {
    let error = create_company(&api.app.services.db, WEB_ORIGIN, input)
        .await
        .err()
        .expect("refusal");
    error.downcast::<DomainError>().expect("domain error")
}

fn texts(list: &Value, key: &str) -> Vec<String> {
    let mut texts: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[key].as_str().unwrap().to_string())
        .collect();
    texts.sort();
    texts
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_company_starts_with_its_invited_administrator_only() {
    let api = TestApi::start().await;
    let created = create(&api, "acme", " Jeanne.Martin@Acme.example ").await;
    let link = &created.invitation.link;
    assert!(
        link.starts_with(&format!("{WEB_ORIGIN}/invitation#")),
        "{link}"
    );

    let shown = api
        .anonymous()
        .post("/auth/invitation", json!({ "token": token_of(link) }))
        .await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let shown = shown.json();
    assert_eq!(shown["company"], "Acme Industrie");
    assert_eq!(shown["email"], "jeanne.martin@acme.example");
    assert_eq!(shown["displayName"], "Jeanne Martin");
    let early = api
        .try_login("jeanne.martin@acme.example", INVITED_PASSWORD)
        .await;
    assert_eq!(early.status, StatusCode::UNAUTHORIZED);

    let accepted = accept(&api, link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.session_from(&accepted);
    let me = jeanne.get("/auth/me").await.json();
    assert_eq!(me["user"]["id"], json!(created.admin_id));
    assert_eq!(me["tenant"]["id"], json!(created.tenant_id));
    assert_eq!(me["tenant"]["name"], "Acme Industrie");
    assert_eq!(me["tenant"]["demo"], false);
    let permissions: Vec<&str> = me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    for granted in ["agent:use", "user:manage", "role:manage", "settings:manage"] {
        assert!(permissions.contains(&granted), "{granted}: {permissions:?}");
    }
    // The workspace's administration gives no business data, and the base
    // role (memories, missions) covers the administrator's own data only.
    for withheld in [
        "intervention:read",
        "planning:read",
        "memory:write",
        "document:write",
    ] {
        assert!(!permissions.contains(&withheld), "{withheld}");
    }
    let mut grants: Vec<&str> = me["grants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g.as_str().unwrap())
        .collect();
    grants.sort();
    assert_eq!(
        grants,
        [
            "Administrateur technique — toute l’entreprise",
            "Collaborateur — ses propres données",
        ]
    );

    let organization = jeanne.get("/admin/organization").await.json();
    assert_eq!(organization["units"].as_array().unwrap().len(), 1);
    assert_eq!(organization["units"][0]["type"], "company");
    assert_eq!(organization["units"][0]["name"], "Acme Industrie");
    assert_eq!(
        texts(&organization["users"], "displayName"),
        ["Jeanne Martin"]
    );
    let roles = jeanne.get("/admin/roles").await.json();
    assert_eq!(
        texts(&roles["roles"], "key"),
        ["knowledge_manager", "member", "tenant_admin"]
    );
    assert!(
        roles["roles"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["isSystem"] == true)
    );
    let users = jeanne.get("/admin/users").await.json();
    assert_eq!(
        texts(&users["users"], "email"),
        ["jeanne.martin@acme.example"]
    );

    let audit = jeanne.get("/admin/audit").await;
    let system: Vec<String> = audit.json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["actorType"] == "system")
        .map(|e| e["action"].as_str().unwrap().to_string())
        .collect();
    for action in [
        "tenant.created",
        "user.created",
        "role.assigned",
        "user.invited",
    ] {
        assert!(system.iter().any(|a| a == action), "{action}: {system:?}");
    }
    assert!(!audit.text().contains(&token_of(link)));

    // Nothing of the demo company is reachable, nor shows it.
    let foreign = jeanne
        .patch(
            &format!("/admin/users/{}", user::LUCAS),
            json!({ "displayName": "Repris" }),
        )
        .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND, "{}", foreign.text());
    let paul = api.sign_in(user::PAUL).await;
    let demo_users = paul.get("/admin/users").await.json();
    assert!(!texts(&demo_users["users"], "email").contains(&"jeanne.martin@acme.example".into()));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_slug_or_an_address_already_taken_leaves_nothing_behind() {
    let api = TestApi::start().await;
    let slug = refused(&api, company(tenant::SLUG, "nouvelle@acme.example")).await;
    assert_eq!(slug.code, ErrorCode::Conflict);
    assert_eq!(
        slug.message,
        "Ce sigle est déjà celui d’une autre entreprise"
    );

    for taken in [email(user::PAUL), email(user::PAUL).to_uppercase()] {
        let address = refused(&api, company("adresse-prise", &taken)).await;
        assert_eq!(address.code, ErrorCode::Conflict, "{taken}");
        assert_eq!(address.message, "Cette adresse e-mail est déjà utilisée");
    }
    // The company refused with the address was not kept: its slug is free.
    create(&api, "adresse-prise", "libre@acme.example").await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn invalid_requests_are_refused_before_any_change() {
    let api = TestApi::start().await;
    let mut short_name = company("nom-court", "court@acme.example");
    short_name.name = " A ".into();
    let mut blank_admin = company("admin-vide", "vide@acme.example");
    blank_admin.admin_name = "   ".into();
    let invalid = [
        company("Acme Corp", "majuscules@acme.example"),
        company("-acme", "tiret@acme.example"),
        company("adresse-invalide", "pas-une-adresse"),
        short_name,
        blank_admin,
    ];
    for input in invalid {
        let slug = input.slug.clone();
        let error = refused(&api, input).await;
        assert_eq!(
            error.code,
            ErrorCode::Validation,
            "{slug}: {}",
            error.message
        );
    }
    // None of them left an account behind.
    for address in [
        "majuscules@acme.example",
        "court@acme.example",
        "vide@acme.example",
    ] {
        create(&api, &address.replace("@acme.example", "-ok"), address).await;
    }
}
