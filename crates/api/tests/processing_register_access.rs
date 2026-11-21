//! Who reads the record of processing activities (ADR-217): who holds
//! `compliance:read` on the whole company, a data protection officer
//! without the rest of the administration among them; not who manages its
//! settings or reads its audit log, not a member, not an administrator of a
//! single team, and the permission is never granted on a unit.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use serde_json::json;
use support::TestApi;
use support::accounts::{activate, create_account, create_partial_admin, holder_of};

const REGISTER: &str = "/admin/processing-register";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_holds_compliance_read_on_the_whole_company_reads_it() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    assert_eq!(paul.get(REGISTER).await.status, StatusCode::OK);

    let officer = holder_of(&api, &paul, "dpo.registre", &["compliance:read"]).await;
    let read = officer.get(REGISTER).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    assert!(!read.json()["activities"].as_array().unwrap().is_empty());
    // Nothing else of the administration with it.
    assert_eq!(
        officer.get("/admin/retention").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        officer.get("/admin/llm").await.status,
        StatusCode::FORBIDDEN
    );

    let others = holder_of(
        &api,
        &paul,
        "reglages.registre",
        &[
            "settings:manage",
            "audit:read",
            "connector:manage",
            "retention:manage",
        ],
    )
    .await;
    let refused = others.get(REGISTER).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(
        refused.text().contains("compliance:read"),
        "{}",
        refused.text()
    );

    let member = api.sign_in(user::MARC).await;
    assert_eq!(member.get(REGISTER).await.status, StatusCode::FORBIDDEN);

    // The platform's administrator role, held on one team only.
    let mut team_admin = create_partial_admin(&api, &paul, "equipe.registre").await;
    let team_admin = activate(&api, &mut team_admin).await;
    assert_eq!(team_admin.get(REGISTER).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_permission_is_granted_on_the_whole_company_only() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let role = paul
        .post(
            "/admin/roles",
            json!({
                "name": "Délégué à la protection des données",
                "description": "Lit le registre des traitements",
                "permissions": ["compliance:read"],
            }),
        )
        .await;
    assert_eq!(role.status, StatusCode::CREATED, "{}", role.text());
    let account = create_account(&paul, "dpo.equipe").await;
    let on_unit = json!({
        "roleId": role.json()["id"],
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    });
    let granted = paul
        .post(&format!("/admin/users/{}/assignments", account.id), on_unit)
        .await;
    assert_eq!(
        granted.status,
        StatusCode::BAD_REQUEST,
        "{}",
        granted.text()
    );
    assert!(
        granted.error_message().contains("toute l’entreprise"),
        "{}",
        granted.text()
    );
}
