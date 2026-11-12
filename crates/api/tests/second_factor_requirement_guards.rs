//! What keeps the company's requirement from being got round (ADR-201): an
//! invited account the company counts among its administrators, even of one
//! team, enrols before its first session, as it accepts its invitation; a
//! sign-in waiting for a code cannot enrol another app in its place, nor
//! open a session, and the app already enrolled still proves it.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use serde_json::json;
use support::accounts::{INVITED_PASSWORD, accept, invite_account};
use support::llm::MASTER_KEY;
use support::second_factor::{
    enrol, enrolment_step, kept_factor, opens_a_session, password_step, second_step,
};
use support::{Client, PASSWORD, TestApi, TestResponse, email};
use uuid::Uuid;

const KEYED: [(&str, &str); 1] = [("MIMORYA_MASTER_KEY", MASTER_KEY)];

async fn require(admin: &Client<'_>, requirement: &str) -> TestResponse {
    admin
        .put(
            "/admin/second-factor",
            json!({ "requirement": requirement }),
        )
        .await
}

/// The technical administrator's role on the north team only, written
/// directly, as if granted before the company-wide rule existed.
async fn administrator_of_one_team(api: &TestApi, user_id: Uuid) {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    create_role_assignment(
        &mut tx,
        NewRoleAssignment {
            tenant_id: tenant::ID,
            user_id,
            role_id: role::TENANT_ADMIN,
            scope: AssignmentScope::subtree(unit::TEAM_NORTH),
            granted_by_user_id: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn link(invitation: &serde_json::Value) -> &str {
    invitation["link"].as_str().expect("link without a relay")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invited_administrator_enrols_before_its_first_session() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let (administrator, invitation) = invite_account(&paul, "administrateur.invite").await;
    let (_, welcome) = invite_account(&paul, "collegue.invite").await;
    // Its rights count before it accepts, whatever their scope.
    administrator_of_one_team(&api, administrator.id).await;
    assert_eq!(
        require(&paul, "administrators").await.status,
        StatusCode::OK
    );

    let accepted = accept(&api, link(&invitation), INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    assert_eq!(
        accepted.json(),
        json!({ "next": "second_factor_enrolment" })
    );
    assert!(!opens_a_session(&accepted));
    // Its password chosen, the next sign-in waits for the enrolment too.
    let again = api.try_login(&administrator.email, INVITED_PASSWORD).await;
    assert_eq!(again.json(), json!({ "next": "second_factor_enrolment" }));
    assert!(!opens_a_session(&again));

    // Who administers nothing goes straight in.
    let welcomed = accept(&api, link(&welcome), INVITED_PASSWORD).await;
    assert_eq!(welcomed.json(), json!({ "next": "signed_in" }));
    assert!(opens_a_session(&welcomed));
    assert_eq!(require(&paul, "optional").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sign_in_waiting_for_a_code_cannot_enrol_another_app() {
    let api = TestApi::start_with(&KEYED).await;
    let hugo = api.sign_in(user::HUGO).await;
    let (_, mut app, codes) = enrol(&hugo).await;

    // Whoever knows the password holds a challenge to prove, not to enrol.
    let challenge = password_step(&api, &email(user::HUGO), PASSWORD).await;
    let started =
        enrolment_step(&api, &challenge, "/auth/second-factor/enrolment", json!({})).await;
    assert_eq!(started.status, StatusCode::UNAUTHORIZED);
    assert!(started.json().get("secret").is_none());
    assert!(!opens_a_session(&started));
    let confirmed = enrolment_step(
        &api,
        &challenge,
        "/auth/second-factor/enrolment/confirmation",
        json!({ "code": "123456" }),
    )
    .await;
    assert_eq!(confirmed.status, StatusCode::UNAUTHORIZED);
    assert!(!opens_a_session(&confirmed));
    // The second factor kept as it was, nothing counted against it.
    assert_eq!(kept_factor(user::HUGO).await, Some((true, 0)));

    // The challenge still waits for the app enrolled, which proves it.
    let signed_in = second_step(&api, &challenge, &app.next_code()).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    assert!(opens_a_session(&signed_in));
    let session = api.session_from(&signed_in);
    let off = session
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
}
