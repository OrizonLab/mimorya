//! New invitation links given by the operator (`mimorya-admin invite`,
//! ADR-033): only to an account still waiting for its password, in an active
//! company; the new link replaces the former one.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::provisioning::reinvite;
use mimorya_core::demo::user;
use mimorya_core::{DomainError, ErrorCode};
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::{TestApi, WEB_ORIGIN, email, owner_url};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_gets_no_new_link() {
    let api = TestApi::start().await;
    let created = create(&api, "suspendue", "admin@suspendue.example").await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query("update tenants set status = 'suspended' where id = $1")
        .bind(created.tenant_id)
        .execute(&owner)
        .await
        .unwrap();
    let error = reinvite(&api.app.services.db, WEB_ORIGIN, "admin@suspendue.example")
        .await
        .err()
        .expect("refusal");
    let refusal = error.downcast::<DomainError>().expect("domain error");
    assert_eq!(refusal.code, ErrorCode::Conflict);
    assert_eq!(
        refusal.message,
        "L’entreprise de ce compte est suspendue : pas d’invitation"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_an_account_waiting_for_its_password_gets_a_new_link() {
    let api = TestApi::start().await;
    let db = &api.app.services.db;
    let created = create(&api, "nouveau-lien", "admin@nouveau-lien.example").await;

    let renewed = reinvite(db, WEB_ORIGIN, " ADMIN@Nouveau-Lien.example ")
        .await
        .expect("new link");
    assert_ne!(renewed.link, created.invitation.link);
    let former = accept(&api, &created.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(former.status, StatusCode::NOT_FOUND, "{}", former.text());
    let accepted = accept(&api, &renewed.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());

    for (address, code) in [
        (
            "admin@nouveau-lien.example".to_string(),
            ErrorCode::Conflict,
        ),
        (email(user::LUCAS), ErrorCode::Conflict),
        (
            "inconnu@nouveau-lien.example".to_string(),
            ErrorCode::NotFound,
        ),
    ] {
        let error = reinvite(db, WEB_ORIGIN, &address)
            .await
            .err()
            .expect("refusal");
        let refusal = error.downcast::<DomainError>().expect("domain error");
        assert_eq!(refusal.code, code, "{address}");
    }
}
