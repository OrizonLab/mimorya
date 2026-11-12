//! What keeps the company's requirement from being got round (ADR-201): it
//! is read and set only by who manages the settings company-wide, not of
//! one team; an invited account the company counts among its
//! administrators, even of one team, enrols before its first session, as it
//! accepts its invitation; a sign-in waiting for a code cannot enrol another
//! app in its place, nor open a session, and the app already enrolled still
//! proves it. What a sign-in waits for is told to the browser holding its
//! challenge alone, without spending a login.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, activate, create_partial_admin, invite_account};
use support::llm::MASTER_KEY;
use support::second_factor::{
    enrol, enrolment_step, kept_factor, opens_a_session, password_step, second_step,
};
use support::{Client, PASSWORD, TestApi, TestResponse, email};
use uuid::Uuid;

const KEYED: [(&str, &str); 1] = [("MIMORYA_MASTER_KEY", MASTER_KEY)];
const POLICY: &str = "/admin/second-factor";

async fn require(admin: &Client<'_>, requirement: &str) -> TestResponse {
    admin
        .put(POLICY, json!({ "requirement": requirement }))
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

fn link(invitation: &Value) -> &str {
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_of_one_team_neither_reads_nor_sets_it() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.second.facteur").await;
    let partial = activate(&api, &mut partial).await;
    assert_eq!(
        require(&paul, "administrators").await.status,
        StatusCode::OK
    );

    assert_eq!(partial.get(POLICY).await.status, StatusCode::FORBIDDEN);
    for requirement in ["optional", "everyone"] {
        let refused = require(&partial, requirement).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
    // What the company requires stays as set.
    assert_eq!(
        paul.get(POLICY).await.json()["requirement"],
        "administrators"
    );
    assert_eq!(require(&paul, "optional").await.status, StatusCode::OK);
}

/// `GET /auth/second-factor`, with the challenge `challenge` in its cookie.
async fn waiting_for(api: &TestApi, challenge: Option<&str>) -> TestResponse {
    let mut request = axum::http::Request::get("/auth/second-factor");
    if let Some(challenge) = challenge {
        request = request.header("cookie", format!("mimorya_challenge={challenge}"));
    }
    api.send(request.body(axum::body::Body::empty()).unwrap())
        .await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn what_a_sign_in_waits_for_is_told_without_spending_a_login() {
    // Three logins a minute: the sign-in, the password step and the code.
    let mut env = KEYED.to_vec();
    env.push(("LOGIN_RATE_LIMIT_PER_MINUTE", "3"));
    let api = TestApi::start_with(&env).await;
    let lea = api.sign_in(user::LEA).await;
    let (_, mut app, codes) = enrol(&lea).await;

    let challenge = password_step(&api, &email(user::LEA), PASSWORD).await;
    // Asked again and again, as a page reloaded: no login spent.
    for _ in 0..5 {
        let told = waiting_for(&api, Some(&challenge)).await;
        assert_eq!(told.status, StatusCode::OK, "{}", told.text());
        assert_eq!(told.json(), json!({ "next": "second_factor" }));
    }
    // Without a challenge, or with one nobody was given, nothing is told.
    assert_eq!(
        waiting_for(&api, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let unknown = waiting_for(&api, Some("inconnu")).await;
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert!(unknown.json().get("next").is_none());

    // The code still has its login left.
    let signed_in = second_step(&api, &challenge, &app.next_code()).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    // Completed, the challenge is told no more.
    assert_eq!(
        waiting_for(&api, Some(&challenge)).await.status,
        StatusCode::UNAUTHORIZED
    );
    let session = api.session_from(&signed_in);
    let off = session
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
}
