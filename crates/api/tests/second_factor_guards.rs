//! What keeps the second factor from being got round (ADR-200): codes
//! refused throttled per account, however many sign-ins, and past a
//! hundred in a row only a recovery code; a reset link asks for it too; a
//! server without its master key enrols nothing, its recovery codes still
//! signing in; an account without a password has its second factor at its
//! provider; one account's codes prove nothing for another; a session's
//! attempts (password to enrol, codes) count as a sign-in's. The export
//! says it is on, never its secret; the account's end takes it.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_core::second_factor::{DIGITS, code_at, read_base32, step_at};
use serde_json::json;
use support::erasure::rows_naming;
use support::limits::forget_counts;
use support::owner::owner_runs;
use support::resets::{RESET_PASSWORD, complete_reset, new_reset_token};
use support::second_factor::{
    KEYED, challenge_of, enrol, kept_factor, not_its_code, opens_a_session, password_step,
    second_step, traced,
};
use support::{PASSWORD, TestApi, email};

/// A recovery code nobody was given.
const NOBODY_S: &str = "AAAA-BBBB-CCCC-DDDD";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn codes_refused_are_throttled_per_account_then_only_a_recovery_code_passes() {
    let api = TestApi::start_with(&KEYED).await;
    let emma = api.sign_in(user::EMMA).await;
    let (_, mut app, codes) = enrol(&emma).await;
    let address = email(user::EMMA);

    let challenge = password_step(&api, &address, PASSWORD).await;
    for _ in 0..5 {
        let refused = second_step(&api, &challenge, NOBODY_S).await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{}",
            refused.text()
        );
    }
    // The sixth waits, the right code too, and in another sign-in alike.
    let throttled = second_step(&api, &challenge, &app.next_code()).await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    let other = password_step(&api, &address, PASSWORD).await;
    let throttled = second_step(&api, &other, &codes[0]).await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(kept_factor(user::EMMA).await, Some((true, 5)));

    // A hundred refused in a row, the window over: the app's codes no
    // longer pass, a recovery code does, and ends the series.
    forget_counts().await;
    owner_runs(
        "update second_factors set failed_attempts = 100 where user_id = $1",
        &[user::EMMA],
    )
    .await;
    let locked = second_step(&api, &other, &app.next_code()).await;
    assert_eq!(locked.status, StatusCode::UNAUTHORIZED);
    assert!(
        locked
            .error_message()
            .starts_with("Trop de codes refusés à la suite")
    );
    assert_eq!(kept_factor(user::EMMA).await, Some((true, 100)));
    let failed = traced(&api, tenant::ID, user::EMMA, "auth.second_factor_failed").await;
    assert_eq!(
        failed.last().unwrap().1.as_deref(),
        Some("Connexion : trop de codes refusés à la suite")
    );
    let recovered = second_step(&api, &other, &codes[1]).await;
    assert_eq!(recovered.status, StatusCode::OK, "{}", recovered.text());
    assert_eq!(kept_factor(user::EMMA).await, Some((true, 0)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reset_link_asks_for_the_second_factor_before_any_session() {
    let api = TestApi::start_with_mailer_and(&KEYED).await;
    let julien = api.sign_in(user::JULIEN).await;
    let (_, mut app, _) = enrol(&julien).await;
    let address = email(user::JULIEN);

    let token = new_reset_token(&api, &address).await;
    let completed = complete_reset(&api, &token, RESET_PASSWORD).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.text());
    assert_eq!(completed.json(), json!({ "next": "second_factor" }));
    assert!(!opens_a_session(&completed));
    // The former sessions ended all the same.
    assert_eq!(
        julien.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    let challenge = challenge_of(&completed);
    let refused = second_step(&api, &challenge, NOBODY_S).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    let signed_in = second_step(&api, &challenge, &app.next_code()).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let session = api.session_from(&signed_in);
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);
    // The new password, too, asks for it.
    password_step(&api, &address, RESET_PASSWORD).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn without_its_master_key_a_server_enrols_nothing_and_takes_recovery_codes() {
    let keyed = TestApi::start_with(&KEYED).await;
    let enrolled = keyed.sign_in(user::LEA).await;
    let (_, mut app, codes) = enrol(&enrolled).await;

    // The same database, a server without the key: her session goes on.
    let api = TestApi::start().await;
    let lea = api.as_user(&enrolled.session);
    let status = lea.get("/account/second-factor").await.json();
    assert_eq!(status["available"], false);
    assert_eq!(status["enabled"], true);
    let refused = lea
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(refused.error_message().contains("MIMORYA_MASTER_KEY"));

    // The app's codes cannot be checked here: said so, not counted.
    let challenge = password_step(&api, &email(user::LEA), PASSWORD).await;
    // More often than the limit: none was checked, none counts.
    let code = app.next_code();
    for _ in 0..6 {
        let unreadable = second_step(&api, &challenge, &code).await;
        assert_eq!(unreadable.status, StatusCode::UNAUTHORIZED);
        assert!(unreadable.error_message().contains("clé maîtresse"));
    }
    assert_eq!(kept_factor(user::LEA).await, Some((true, 0)));
    let recovered = second_step(&api, &challenge, &codes[1]).await;
    assert_eq!(recovered.status, StatusCode::OK, "{}", recovered.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_without_a_password_has_its_second_factor_at_its_provider() {
    let api = TestApi::start_with(&KEYED).await;
    let isabelle = api.sign_in(user::ISABELLE).await;
    owner_runs(
        "update users set password_hash = null where id = $1",
        &[user::ISABELLE],
    )
    .await;
    let status = isabelle.get("/account/second-factor").await.json();
    assert_eq!(status["applicable"], false);
    let refused = isabelle
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(refused.error_message().contains("fournisseur d’identité"));
    assert_eq!(kept_factor(user::ISABELLE).await, None);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn one_account_s_codes_prove_nothing_for_another() {
    let api = TestApi::start_with(&KEYED).await;
    let (_, mut marc_app, _) = enrol(&api.sign_in(user::MARC).await).await;
    let (_, mut claire_app, claire_codes) = enrol(&api.sign_in(user::CLAIRE).await).await;

    let challenge = password_step(&api, &email(user::MARC), PASSWORD).await;
    for code in [claire_codes[0].clone(), claire_app.next_code()] {
        let refused = second_step(&api, &challenge, &code).await;
        assert_eq!(
            refused.status,
            StatusCode::UNAUTHORIZED,
            "{}",
            refused.text()
        );
    }
    // Counted against Marc; Claire's codes untouched, her app's code still
    // hers to use.
    assert_eq!(kept_factor(user::MARC).await, Some((true, 2)));
    assert_eq!(kept_factor(user::CLAIRE).await, Some((true, 0)));
    let claire_challenge = password_step(&api, &email(user::CLAIRE), PASSWORD).await;
    let hers = second_step(&api, &claire_challenge, &claire_codes[0]).await;
    assert_eq!(hers.status, StatusCode::OK, "{}", hers.text());

    let signed_in = second_step(&api, &challenge, &marc_app.next_code()).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let me = api.session_from(&signed_in).get("/auth/me").await.json();
    assert_eq!(me["user"]["id"], user::MARC.to_string());
    assert_eq!(
        traced(&api, tenant::ID, user::CLAIRE, "auth.second_factor_failed")
            .await
            .len(),
        0
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_export_says_it_is_on_and_the_account_s_end_takes_it() {
    let api = TestApi::start_with(&KEYED).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let (shown, _, codes) = enrol(&lucas).await;

    let export = lucas.get("/account/export").await;
    assert_eq!(export.status, StatusCode::OK, "{}", export.text());
    let factor = &export.json()["secondFactor"];
    assert_eq!(factor["enabled"], true);
    assert!(factor["enabledAt"].is_string());
    assert_eq!(factor["recoveryCodesLeft"], 10);
    // Never its secret nor a code.
    let text = export.text();
    assert!(!text.contains(shown["secret"].as_str().unwrap()));
    assert!(codes.iter().all(|code| !text.contains(code.as_str())));

    // A sign-in waiting for it ends with the account's activity.
    password_step(&api, &email(user::LUCAS), PASSWORD).await;
    assert_eq!(challenges_of(user::LUCAS).await, 1);
    let paul = api.sign_in(user::PAUL).await;
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", user::LUCAS),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(challenges_of(user::LUCAS).await, 0);

    let erased = paul
        .post(
            &format!("/admin/users/{}/erasure", user::LUCAS),
            json!({ "confirmEmail": email(user::LUCAS) }),
        )
        .await;
    assert_eq!(erased.status, StatusCode::OK, "{}", erased.text());
    assert_eq!(kept_factor(user::LUCAS).await, None);
    let left: Vec<_> = (rows_naming(user::LUCAS).await.into_iter())
        .filter(|(_, count)| *count > 0)
        .collect();
    assert!(left.is_empty(), "rows still naming Lucas: {left:?}");
}

/// The sign-ins of `person` waiting for their second factor.
async fn challenges_of(person: uuid::Uuid) -> i64 {
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    let count = sqlx::query_scalar("select count(*) from login_challenges where user_id = $1")
        .bind(person)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_session_s_attempts_count_against_the_account_as_a_sign_in_s() {
    let api = TestApi::start_with(&KEYED).await;
    let hugo = api.sign_in(user::HUGO).await;
    let address = email(user::HUGO);
    let enrolment = "/account/second-factor/enrolment";

    // A wrong password to enrol counts as a failed login: after ten, the
    // right one waits, and so does a sign-in.
    for _ in 0..10 {
        let refused = hugo
            .post(enrolment, json!({ "password": "pas-le-bon" }))
            .await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
    let throttled = hugo.post(enrolment, json!({ "password": PASSWORD })).await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        api.try_login(&address, PASSWORD).await.status,
        StatusCode::TOO_MANY_REQUESTS
    );

    // A wrong first code counts as a refused code, traced.
    forget_counts().await;
    let started = hugo.post(enrolment, json!({ "password": PASSWORD })).await;
    let secret = read_base32(started.json()["secret"].as_str().unwrap()).unwrap();
    let wrong = not_its_code(&secret);
    let confirmation = "/account/second-factor/confirmation";
    for _ in 0..5 {
        let refused = hugo.post(confirmation, json!({ "code": wrong })).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
    let right = format!(
        "{:06}",
        code_at(&secret, step_at(Utc::now().timestamp()), DIGITS)
    );
    let throttled = hugo.post(confirmation, json!({ "code": right })).await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    let failed = traced(&api, tenant::ID, user::HUGO, "auth.second_factor_failed").await;
    assert_eq!(failed.len(), 5);
    assert!(
        failed
            .iter()
            .all(|entry| entry.1.as_deref() == Some("Activation : code refusé"))
    );

    // New recovery codes and turning it off share the sign-in's limit.
    forget_counts().await;
    let (_, _, codes) = enrol(&hugo).await;
    for (route, times) in [("recovery-codes", 3), ("deactivation", 2)] {
        for _ in 0..times {
            let refused = hugo
                .post(
                    &format!("/account/second-factor/{route}"),
                    json!({ "code": NOBODY_S }),
                )
                .await;
            assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
        }
    }
    let throttled = hugo
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    let challenge = password_step(&api, &address, PASSWORD).await;
    let throttled = second_step(&api, &challenge, &codes[0]).await;
    assert_eq!(throttled.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(kept_factor(user::HUGO).await, Some((true, 5)));
}
