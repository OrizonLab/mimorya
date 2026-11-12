//! Second factor codes used at the same instant (ADR-200), made
//! deterministic with `support::locks`: the account's row held while both
//! completions queue behind it, then released. A code serves once however
//! many sign-ins carry it; a challenge opens one session; a password
//! changed while a completion waits ends its challenge; a sign-in crossing
//! an activation keeps no session past it (ADR-201).

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::user;
use mimorya_core::second_factor::{DIGITS, code_at, read_base32, step_at};
use serde_json::json;
use support::limits::windows_of;
use support::llm::MASTER_KEY;
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::second_factor::{enrol, kept_factor, opens_a_session, password_step, second_step};
use support::{PASSWORD, TestApi, TestResponse, email};

async fn api() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await
}

/// The statuses of `responses`, sorted.
fn statuses(responses: [&TestResponse; 2]) -> Vec<StatusCode> {
    let mut statuses: Vec<StatusCode> = responses.iter().map(|r| r.status).collect();
    statuses.sort();
    statuses
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_code_carried_by_two_sign_ins_opens_one_session() {
    let api = api().await;
    let (_, mut app, _) = enrol(&api.sign_in(user::EMMA).await).await;
    let address = email(user::EMMA);
    // A phished code replayed by another browser while its owner signs in.
    let (first, second) = (
        password_step(&api, &address, PASSWORD).await,
        password_step(&api, &address, PASSWORD).await,
    );
    let code = app.next_code();

    let hold = hold_rows(&api, "users", &[user::EMMA]).await;
    let (one, other, ()) = tokio::join!(
        second_step(&api, &first, &code),
        async {
            wait_for_waiters(&api, 1).await;
            second_step(&api, &second, &code).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(
        statuses([&one, &other]),
        [StatusCode::OK, StatusCode::UNAUTHORIZED]
    );
    assert_eq!(
        [&one, &other].iter().filter(|r| opens_a_session(r)).count(),
        1
    );
    let refused = if one.status == StatusCode::OK {
        &other
    } else {
        &one
    };
    assert_eq!(refused.error_message(), "Code incorrect ou déjà utilisé");
    assert_eq!(kept_factor(user::EMMA).await, Some((true, 1)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_challenge_completed_twice_opens_one_session_and_spends_one_code() {
    let api = api().await;
    let hugo = api.sign_in(user::HUGO).await;
    let (_, _, codes) = enrol(&hugo).await;
    let challenge = password_step(&api, &email(user::HUGO), PASSWORD).await;

    let hold = hold_rows(&api, "users", &[user::HUGO]).await;
    let (one, other, ()) = tokio::join!(
        second_step(&api, &challenge, &codes[0]),
        async {
            wait_for_waiters(&api, 1).await;
            second_step(&api, &challenge, &codes[1]).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(
        statuses([&one, &other]),
        [StatusCode::OK, StatusCode::UNAUTHORIZED]
    );
    let late = if one.status == StatusCode::OK {
        &other
    } else {
        &one
    };
    assert!(late.error_message().contains("expiré"), "{}", late.text());
    assert!(!opens_a_session(late));
    // The late code was right: it stays unused, and nothing counts.
    let session = api.session_from(if one.status == StatusCode::OK {
        &one
    } else {
        &other
    });
    let status = session.get("/account/second-factor").await.json();
    assert_eq!(status["recoveryCodesLeft"], 9);
    assert_eq!(kept_factor(user::HUGO).await, Some((true, 0)));
    assert!(
        windows_of("second_factor_failures")
            .await
            .iter()
            .all(|(_, hits)| *hits == 0)
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_password_changed_while_a_code_waits_ends_its_sign_in() {
    let api = api().await;
    let (_, mut app, _) = enrol(&api.sign_in(user::LEA).await).await;
    let challenge = password_step(&api, &email(user::LEA), PASSWORD).await;
    let paul = api.sign_in(user::PAUL).await;
    let reset_url = format!("/admin/users/{}/password", user::LEA);

    // The administrator's reset queued first on the account's row, the
    // code behind it.
    let hold = hold_rows(&api, "users", &[user::LEA]).await;
    let (reset, completed, ()) = tokio::join!(
        paul.post(&reset_url, json!({})),
        async {
            wait_for_waiters(&api, 1).await;
            second_step(&api, &challenge, &app.next_code()).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(completed.status, StatusCode::UNAUTHORIZED);
    assert!(completed.error_message().contains("expiré"));
    assert!(!opens_a_session(&completed));
    assert_eq!(kept_factor(user::LEA).await, Some((true, 0)));
    // The new password asks for the second factor again.
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    password_step(&api, &email(user::LEA), &temporary).await;
}

/// Starts an enrolment as `session`: the code its app shows now.
async fn enrolment_code(session: &support::Client<'_>) -> String {
    let started = session
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let secret = read_base32(started.json()["secret"].as_str().unwrap()).unwrap();
    format!(
        "{:06}",
        code_at(&secret, step_at(Utc::now().timestamp()), DIGITS)
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sign_in_crossing_an_activation_keeps_no_session_past_it() {
    let api = api().await;
    let confirm = "/account/second-factor/confirmation";

    // The activation queued first: the sign-in behind it finds the second
    // factor on, and is asked for it.
    let nadia = api.sign_in(user::NADIA).await;
    let code = enrolment_code(&nadia).await;
    let hold = hold_rows(&api, "users", &[user::NADIA]).await;
    let (activated, login, ()) = tokio::join!(
        nadia.post(confirm, json!({ "code": code })),
        async {
            wait_for_waiters(&api, 1).await;
            api.try_login(&email(user::NADIA), PASSWORD).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    assert_eq!(login.json(), json!({ "next": "second_factor" }));
    assert!(!opens_a_session(&login));

    // The sign-in queued first: its session opens, then the activation
    // ends it.
    let claire = api.sign_in(user::CLAIRE).await;
    let code = enrolment_code(&claire).await;
    let address = email(user::CLAIRE);
    let hold = hold_rows(&api, "users", &[user::CLAIRE]).await;
    let (login, activated, ()) = tokio::join!(
        api.try_login(&address, PASSWORD),
        async {
            wait_for_waiters(&api, 1).await;
            claire.post(confirm, json!({ "code": code })).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(activated.status, StatusCode::OK, "{}", activated.text());
    assert!(opens_a_session(&login));
    let crossed = api.session_from(&login);
    assert_eq!(
        crossed.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(claire.get("/auth/me").await.status, StatusCode::OK);
}
