//! What a company requires of its password accounts (ADR-201): set by
//! those who manage its settings, where the server can keep a second
//! factor; required of its administrators (a team lead who validates and
//! deposits documents is none) or of everyone, at the next sign-in with a
//! password, through a reset or an accepted invitation too. The account
//! enrols before any session opens, then can no longer turn it off.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_core::second_factor::{DIGITS, code_at, read_base32, step_at};
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, invite_account};
use support::llm::MASTER_KEY;
use support::mail::link_in;
use support::resets::{RESET_PASSWORD, complete_reset, new_reset_token};
use support::second_factor::{
    challenge_of, enrolment_step, not_its_code, opens_a_session, password_step, second_step, traced,
};
use support::{Client, PASSWORD, TestApi, TestResponse, email};

const KEYED: [(&str, &str); 1] = [("MIMORYA_MASTER_KEY", MASTER_KEY)];
const POLICY: &str = "/admin/second-factor";

async fn require(admin: &Client<'_>, requirement: &str) -> TestResponse {
    admin
        .put(POLICY, json!({ "requirement": requirement }))
        .await
}

/// The sign-in's answer, the browser holding the challenge.
async fn waiting(api: &TestApi, challenge: &str) -> Value {
    let request = axum::http::Request::get("/auth/second-factor")
        .header("cookie", format!("mimorya_challenge={challenge}"))
        .body(axum::body::Body::empty())
        .unwrap();
    api.send(request).await.json()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_requires_it_of_its_administrators_who_enrol_at_sign_in() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let shown = paul.get(POLICY).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    assert_eq!(
        shown.json(),
        json!({ "requirement": "optional", "secretsAvailable": true })
    );
    // Only those who manage the settings, company-wide.
    let lucas = api.sign_in(user::LUCAS).await;
    assert_eq!(lucas.get(POLICY).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        require(&lucas, "everyone").await.status,
        StatusCode::FORBIDDEN
    );
    assert!(require(&paul, "tous").await.status.is_client_error());

    let before = requirement_entries(&api).await.len();
    let set = require(&paul, "administrators").await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(
        require(&paul, "administrators").await.status,
        StatusCode::OK
    );
    // Traced once: the second time changed nothing.
    let audited = requirement_entries(&api).await;
    assert_eq!(audited.len(), before + 1);
    assert_eq!(
        audited.last().unwrap(),
        &json!({ "requirement": "administrators", "previous": "optional" })
    );

    // A team lead validates and deposits documents: no administration.
    let marc = api.try_login(&email(user::MARC), PASSWORD).await;
    assert_eq!(marc.json(), json!({ "next": "signed_in" }));

    // Paul administers: no session before he enrols.
    let response = api.try_login(&email(user::PAUL), PASSWORD).await;
    assert_eq!(
        response.json(),
        json!({ "next": "second_factor_enrolment" })
    );
    assert!(!opens_a_session(&response));
    let challenge = challenge_of(&response);
    assert_eq!(
        waiting(&api, &challenge).await,
        json!({ "next": "second_factor_enrolment" })
    );
    // Nothing to prove yet: the code route refuses this challenge.
    let early = second_step(&api, &challenge, "123456").await;
    assert_eq!(early.status, StatusCode::UNAUTHORIZED);
    let started =
        enrolment_step(&api, &challenge, "/auth/second-factor/enrolment", json!({})).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let account = email(user::PAUL).replace('@', "%40");
    assert!(started.json()["uri"].as_str().unwrap().contains(&account));
    let secret = read_base32(started.json()["secret"].as_str().unwrap()).unwrap();
    let confirmation = "/auth/second-factor/enrolment/confirmation";
    let wrong = enrolment_step(
        &api,
        &challenge,
        confirmation,
        json!({ "code": not_its_code(&secret) }),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert!(!opens_a_session(&wrong));
    let failed = traced(&api, tenant::ID, user::PAUL, "auth.second_factor_failed").await;
    assert_eq!(
        failed.last().unwrap().1.as_deref(),
        Some("Activation à la connexion : code refusé")
    );
    let code = format!(
        "{:06}",
        code_at(&secret, step_at(Utc::now().timestamp()), DIGITS)
    );
    let enrolled = enrolment_step(&api, &challenge, confirmation, json!({ "code": code })).await;
    assert_eq!(enrolled.status, StatusCode::OK, "{}", enrolled.text());
    assert_eq!(enrolled.json()["codes"].as_array().unwrap().len(), 10);
    assert!(opens_a_session(&enrolled));
    let session = api.session_from(&enrolled);
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);
    // The session opened before it ended; the challenge served once.
    assert_eq!(paul.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
    let again = enrolment_step(&api, &challenge, confirmation, json!({ "code": code })).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    let signed_in = traced(&api, tenant::ID, user::PAUL, "auth.login").await;
    assert_eq!(
        signed_in.last().unwrap().2,
        json!({ "method": "password", "secondFactor": "enrolled" })
    );
    let enabled = traced(
        &api,
        tenant::ID,
        user::PAUL,
        "account.second_factor_enabled",
    )
    .await;
    assert_eq!(enabled.last().unwrap().2, json!({ "atSignIn": true }));

    // Required, it stays, whatever the attempts; the next sign-in proves it.
    let status = session.get("/account/second-factor").await.json();
    assert_eq!(
        (status["required"].clone(), status["enabled"].clone()),
        (json!(true), json!(true))
    );
    for _ in 0..6 {
        let refused = session
            .post(
                "/account/second-factor/deactivation",
                json!({ "code": "AAAA-BBBB-CCCC-DDDD" }),
            )
            .await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    }
    password_step(&api, &email(user::PAUL), PASSWORD).await;

    // No longer required, it can be turned off again.
    assert_eq!(require(&session, "optional").await.status, StatusCode::OK);
    let codes = enrolled.json()["codes"].clone();
    let off = session
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
}

/// What the requirement's changes traced, oldest first.
async fn requirement_entries(api: &TestApi) -> Vec<Value> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let entries = sqlx::query_scalar(
        "select metadata from audit_log \
         where action = 'settings.second_factor_requirement_configured' order by id",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    entries
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn required_of_everyone_a_reset_or_an_invitation_enrols_first() {
    let api = TestApi::start_with_mailer_and(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let (invited, _) = invite_account(&paul, "invite.second.facteur").await;
    assert_eq!(require(&paul, "everyone").await.status, StatusCode::OK);

    let lucas = api.try_login(&email(user::LUCAS), PASSWORD).await;
    assert_eq!(lucas.json(), json!({ "next": "second_factor_enrolment" }));

    let token = new_reset_token(&api, &email(user::JULIEN)).await;
    let reset = complete_reset(&api, &token, RESET_PASSWORD).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(reset.json(), json!({ "next": "second_factor_enrolment" }));
    assert!(!opens_a_session(&reset));

    let sent = api.mail.sent();
    let mail = sent
        .iter()
        .rev()
        .find(|m| m.to == invited.email)
        .expect("invitation");
    let accepted = accept(&api, &link_in(mail), INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    assert_eq!(
        accepted.json(),
        json!({ "next": "second_factor_enrolment" })
    );
    assert!(!opens_a_session(&accepted));

    // Back to no one's: the password alone opens a session again.
    assert_eq!(require(&paul, "optional").await.status, StatusCode::OK);
    let lucas = api.try_login(&email(user::LUCAS), PASSWORD).await;
    assert_eq!(lucas.json(), json!({ "next": "signed_in" }));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_a_master_key_nothing_can_be_required_or_enrolled() {
    let keyed = TestApi::start_with(&KEYED).await;
    let paul = keyed.sign_in(user::PAUL).await;
    assert_eq!(require(&paul, "everyone").await.status, StatusCode::OK);

    // The same database, a server without the key.
    let api = TestApi::start().await;
    let admin = api.as_user(&paul.session);
    let shown = admin.get(POLICY).await.json();
    assert_eq!(shown["secretsAvailable"], false);
    let refused = require(&admin, "administrators").await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(refused.error_message().contains("MIMORYA_MASTER_KEY"));
    // The sign-in waits, and says why it cannot go on.
    let challenge = password_step_for_enrolment(&api, user::EMMA).await;
    let started =
        enrolment_step(&api, &challenge, "/auth/second-factor/enrolment", json!({})).await;
    assert_eq!(started.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(started.error_message().contains("MIMORYA_MASTER_KEY"));
    assert_eq!(require(&admin, "optional").await.status, StatusCode::OK);
}

async fn password_step_for_enrolment(api: &TestApi, person: uuid::Uuid) -> String {
    let response = api.try_login(&email(person), PASSWORD).await;
    assert_eq!(
        response.json(),
        json!({ "next": "second_factor_enrolment" })
    );
    challenge_of(&response)
}
