//! The second factor of the accounts that sign in with a password
//! (ADR-200): enrolled from « Mon compte » with an authenticator app, the
//! password typed again first, asked
//! for after the password before any session opens, each code accepted
//! once; recovery codes, each once, renewed with a current code; turned off
//! with a current code. Every step is traced, never a code.

mod support;

use axum::http::StatusCode;
use chrono::Utc;
use mimorya_core::demo::{tenant, user};
use mimorya_core::second_factor::{DIGITS, code_at, read_base32, step_at};
use serde_json::json;
use support::llm::MASTER_KEY;
use support::second_factor::{
    challenge_of, enrol, kept_factor, opens_a_session, password_step, second_step, traced,
};
use support::{PASSWORD, TestApi, email};

async fn api() -> TestApi {
    TestApi::start_with(&[("MIMORYA_MASTER_KEY", MASTER_KEY)]).await
}

/// Six digits the app of `secret` shows at no step near now.
fn not_its_code(secret: &[u8]) -> String {
    let now = step_at(Utc::now().timestamp());
    let near: Vec<u32> = (now - 2..=now + 2)
        .map(|step| code_at(secret, step, DIGITS))
        .collect();
    let code = (0..).find(|code| !near.contains(code)).unwrap();
    format!("{code:06}")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_enrolled_account_signs_in_with_its_app_once_per_code() {
    let api = api().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let status = lucas.get("/account/second-factor").await;
    assert_eq!(status.status, StatusCode::OK, "{}", status.text());
    assert_eq!(
        status.json(),
        json!({
            "applicable": true,
            "available": true,
            "enabled": false,
            "enabledAt": null,
            "recoveryCodesLeft": 0,
        })
    );

    // A session alone does not enrol: the password is typed again, and a
    // wrong one counts as a failed login.
    let refused = lucas
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": "pas-le-bon" }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(refused.error_message(), "Mot de passe incorrect");
    assert_eq!(kept_factor(user::LUCAS).await, None);

    // A first code that is not the app's leaves the enrolment waiting.
    let started = lucas
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let secret = read_base32(started.json()["secret"].as_str().unwrap()).unwrap();
    let refused = lucas
        .post(
            "/account/second-factor/confirmation",
            json!({ "code": not_its_code(&secret) }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(kept_factor(user::LUCAS).await, Some((false, 0)));

    let (shown, mut app, codes) = enrol(&lucas).await;
    let uri = shown["uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/Mimorya:lucas.petit%40neo-maintenance.example?"));
    assert!(uri.contains(&format!("secret={}", shown["secret"].as_str().unwrap())));
    assert!(uri.contains("&issuer=Mimorya&algorithm=SHA1&digits=6&period=30"));
    assert_eq!(codes.len(), 10);
    assert!(
        codes
            .iter()
            .all(|code| code.len() == 19 && code.matches('-').count() == 3)
    );
    let status = lucas.get("/account/second-factor").await.json();
    assert_eq!(status["enabled"], true);
    assert!(status["enabledAt"].is_string());
    assert_eq!(status["recoveryCodesLeft"], 10);
    // Enrolled once: a new enrolment would replace it unseen.
    let again = lucas
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());
    let enabled = traced(
        &api,
        tenant::ID,
        user::LUCAS,
        "account.second_factor_enabled",
    )
    .await;
    assert_eq!(enabled.len(), 1);

    // The password alone opens no session: the challenge's cookie is no
    // session either.
    let address = email(user::LUCAS);
    let response = api.try_login(&address, PASSWORD).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.text());
    assert_eq!(response.json(), json!({ "next": "second_factor" }));
    assert!(!opens_a_session(&response));
    let cookie = response.headers["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("Path=/api/auth/second-factor;"));
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    let challenge = challenge_of(&response);
    let as_session = api.as_user(&challenge).get("/auth/me").await;
    assert_eq!(as_session.status, StatusCode::UNAUTHORIZED);

    let code = app.next_code();
    let completed = second_step(&api, &challenge, &code).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.text());
    assert_eq!(completed.json(), json!({ "next": "signed_in" }));
    assert!(opens_a_session(&completed));
    let session = api.session_from(&completed);
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);
    let signed_in = traced(&api, tenant::ID, user::LUCAS, "auth.login").await;
    let last = signed_in.last().unwrap();
    assert_eq!(last.0, "success");
    assert_eq!(
        last.2,
        json!({ "method": "password", "secondFactor": "app" })
    );

    // The challenge served once; the code too, in any sign-in.
    let reused = second_step(&api, &challenge, &codes[0]).await;
    assert_eq!(reused.status, StatusCode::UNAUTHORIZED, "{}", reused.text());
    assert!(reused.error_message().contains("expiré"));
    let challenge = password_step(&api, &address, PASSWORD).await;
    let replayed = second_step(&api, &challenge, &app.last_code()).await;
    assert_eq!(replayed.status, StatusCode::UNAUTHORIZED);
    assert_eq!(replayed.error_message(), "Code incorrect ou déjà utilisé");
    assert!(!opens_a_session(&replayed));
    assert_eq!(kept_factor(user::LUCAS).await, Some((true, 1)));
    let failed = traced(&api, tenant::ID, user::LUCAS, "auth.second_factor_failed").await;
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, "denied");
    assert_eq!(failed[0].1.as_deref(), Some("Connexion : code refusé"));
    // A recovery code passes, and the refusals in a row end.
    let completed = second_step(&api, &challenge, &codes[1]).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.text());
    assert_eq!(kept_factor(user::LUCAS).await, Some((true, 0)));

    // No code written in what the account's sign-ins traced.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let leaked: i64 = sqlx::query_scalar(
        "select count(*) from audit_log where actor_user_id = $2 \
         and (metadata::text like '%' || $1 || '%' or coalesce(reason, '') like '%' || $1 || '%')",
    )
    .bind(&code)
    .bind(user::LUCAS)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(leaked, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_recovery_code_signs_in_once_and_new_ones_replace_them() {
    let api = api().await;
    let nadia = api.sign_in(user::NADIA).await;
    let (_, mut app, codes) = enrol(&nadia).await;
    let address = email(user::NADIA);

    // As copied: lower case, without its dashes.
    let typed = codes[3].replace('-', "").to_lowercase();
    let challenge = password_step(&api, &address, PASSWORD).await;
    let completed = second_step(&api, &challenge, &typed).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.text());
    let used = traced(&api, tenant::ID, user::NADIA, "auth.recovery_code_used").await;
    assert_eq!(used.len(), 1);
    assert_eq!(used[0].2, json!({ "place": "Connexion", "codesLeft": 9 }));
    let session = api.session_from(&completed);
    let status = session.get("/account/second-factor").await.json();
    assert_eq!(status["recoveryCodesLeft"], 9);

    let challenge = password_step(&api, &address, PASSWORD).await;
    let again = second_step(&api, &challenge, &codes[3]).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED, "{}", again.text());

    // New codes, proven by a current one, which ends the refusals in a
    // row: the former ones stop working.
    assert_eq!(kept_factor(user::NADIA).await, Some((true, 1)));
    let renewed = session
        .post(
            "/account/second-factor/recovery-codes",
            json!({ "code": app.next_code() }),
        )
        .await;
    assert_eq!(renewed.status, StatusCode::OK, "{}", renewed.text());
    let fresh: Vec<String> = renewed.json()["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|code| code.as_str().unwrap().to_string())
        .collect();
    assert_eq!(fresh.len(), 10);
    assert_eq!(kept_factor(user::NADIA).await, Some((true, 0)));
    assert!(fresh.iter().all(|code| !codes.contains(code)));
    assert_eq!(
        traced(
            &api,
            tenant::ID,
            user::NADIA,
            "account.recovery_codes_renewed"
        )
        .await
        .len(),
        1
    );
    let former = second_step(&api, &challenge, &codes[0]).await;
    assert_eq!(former.status, StatusCode::UNAUTHORIZED);
    let current = second_step(&api, &challenge, &fresh[0]).await;
    assert_eq!(current.status, StatusCode::OK, "{}", current.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn turning_it_off_takes_a_current_code() {
    let api = api().await;
    let hugo = api.sign_in(user::HUGO).await;
    let (_, _, codes) = enrol(&hugo).await;

    for (code, status) in [
        ("bonjour", StatusCode::BAD_REQUEST),
        ("AAAA-BBBB-CCCC-DDDD", StatusCode::FORBIDDEN),
    ] {
        let refused = hugo
            .post(
                "/account/second-factor/deactivation",
                json!({ "code": code }),
            )
            .await;
        assert_eq!(refused.status, status, "{}", refused.text());
    }
    let failed = traced(&api, tenant::ID, user::HUGO, "auth.second_factor_failed").await;
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].1.as_deref(), Some("Désactivation : code refusé"));
    assert_eq!(kept_factor(user::HUGO).await, Some((true, 1)));

    let off = hugo
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
    assert_eq!(kept_factor(user::HUGO).await, None);
    let disabled = traced(
        &api,
        tenant::ID,
        user::HUGO,
        "account.second_factor_disabled",
    )
    .await;
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].2, json!({ "proof": "recovery" }));
    // Its codes went with it; the password alone opens a session again.
    let again = hugo
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[1] }),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.text());
    let response = api.try_login(&email(user::HUGO), PASSWORD).await;
    assert_eq!(response.json(), json!({ "next": "signed_in" }));
    assert!(opens_a_session(&response));
}
