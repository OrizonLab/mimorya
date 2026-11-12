//! An account's second factor reset by an administrator (ADR-202): the
//! second factor and its recovery codes go, the sign-ins waiting for a code
//! end, every session of the account closes, and its next sign-in opens a
//! session, or first an enrolment where the company requires one. Traced.
//! Only who manages accounts company-wide resets one, never its own: an
//! administrator of one team, a collaborator, another company's account or
//! one without a second factor change nothing. The directory shows account
//! managers alone whose second factor is on. A sign-in crossing a reset,
//! made deterministic with `support::locks`, keeps no session past it.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::accounts::{activate, create_partial_admin};
use support::locks::{hold_rows, release_after, wait_for_waiters};
use support::second_factor::{
    KEYED, enrol, kept_factor, opens_a_session, password_step, require, second_step, traced,
};
use support::{Client, PASSWORD, TestApi, email, owner_url};
use uuid::Uuid;

fn reset_path(user_id: Uuid) -> String {
    format!("/admin/users/{user_id}/second-factor")
}

/// What the directory says of `user_id`'s second factor to `reader`.
async fn shown(reader: &Client<'_>, user_id: Uuid) -> Option<Value> {
    let directory = reader.get("/admin/users").await.json();
    let row = directory["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == user_id.to_string())
        .cloned()
        .expect("listed");
    row.get("secondFactor").cloned()
}

async fn recovery_codes_of(user_id: Uuid) -> i64 {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let count =
        sqlx::query_scalar("select count(*) from second_factor_recovery_codes where user_id = $1")
            .bind(user_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    owner.close().await;
    count
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_administrator_resets_it_and_every_session_of_the_account_ends() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let nadia = api.sign_in(user::NADIA).await;
    let (_, mut app, _) = enrol(&nadia).await;
    assert_eq!(shown(&paul, user::NADIA).await, Some(json!(true)));
    assert_eq!(shown(&paul, user::PAUL).await, Some(json!(false)));
    // A sign-in waiting for its code.
    let waiting = password_step(&api, &email(user::NADIA), PASSWORD).await;

    let reset = paul.delete(&reset_path(user::NADIA)).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(nadia.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
    let late = second_step(&api, &waiting, &app.next_code()).await;
    assert_eq!(late.status, StatusCode::UNAUTHORIZED);
    assert_eq!(kept_factor(user::NADIA).await, None);
    assert_eq!(recovery_codes_of(user::NADIA).await, 0);
    assert_eq!(shown(&paul, user::NADIA).await, Some(json!(false)));
    let entries = traced(&api, tenant::ID, user::PAUL, "user.second_factor_reset").await;
    assert_eq!(
        entries.last().unwrap().2,
        json!({ "sessionsClosed": 1, "signInsEnded": 1 })
    );

    // Not required: the password alone opens a session again.
    let again = api.try_login(&email(user::NADIA), PASSWORD).await;
    assert_eq!(again.json(), json!({ "next": "signed_in" }));
    // Nothing left to reset: refused, nothing traced.
    let twice = paul.delete(&reset_path(user::NADIA)).await;
    assert_eq!(twice.status, StatusCode::CONFLICT, "{}", twice.text());
    let after = traced(&api, tenant::ID, user::PAUL, "user.second_factor_reset").await;
    assert_eq!(after.len(), entries.len());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn required_by_the_company_the_next_sign_in_enrols_again() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let hugo = api.sign_in(user::HUGO).await;
    enrol(&hugo).await;
    assert_eq!(require(&paul, "everyone").await.status, StatusCode::OK);

    let reset = paul.delete(&reset_path(user::HUGO)).await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    let next = api.try_login(&email(user::HUGO), PASSWORD).await;
    assert_eq!(next.json(), json!({ "next": "second_factor_enrolment" }));
    assert_eq!(require(&paul, "optional").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_who_manages_accounts_company_wide_resets_another_s() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let (_, _, codes) = enrol(&claire).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let mut partial = create_partial_admin(&api, &paul, "admin.equipe.reinitialisation").await;
    let partial = activate(&api, &mut partial).await;

    // Neither a collaborator nor an administrator of one team.
    for client in [&lucas, &partial] {
        let refused = client.delete(&reset_path(user::CLAIRE)).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    }
    // A collaborator reads the directory, not whose second factor is on.
    assert_eq!(shown(&lucas, user::CLAIRE).await, None);
    // Never one's own, as a password.
    let own = paul.delete(&reset_path(user::PAUL)).await;
    assert_eq!(own.status, StatusCode::FORBIDDEN);
    // Another company's account, or none: not found.
    for unknown in [OTHER_USER_ID, Uuid::new_v4()] {
        let absent = paul.delete(&reset_path(unknown)).await;
        assert_eq!(absent.status, StatusCode::NOT_FOUND, "{}", absent.text());
    }
    // An enrolment that waits is no second factor: kept as it was.
    let emma = api.sign_in(user::EMMA).await;
    let started = emma
        .post(
            "/account/second-factor/enrolment",
            json!({ "password": PASSWORD }),
        )
        .await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let pending = paul.delete(&reset_path(user::EMMA)).await;
    assert_eq!(pending.status, StatusCode::CONFLICT);
    assert_eq!(kept_factor(user::EMMA).await, Some((false, 0)));
    assert_eq!(emma.get("/auth/me").await.status, StatusCode::OK);

    // Claire's second factor, codes and session stay.
    assert_eq!(kept_factor(user::CLAIRE).await, Some((true, 0)));
    assert_eq!(recovery_codes_of(user::CLAIRE).await, 10);
    assert_eq!(claire.get("/auth/me").await.status, StatusCode::OK);
    let off = claire
        .post(
            "/account/second-factor/deactivation",
            json!({ "code": codes[0] }),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_sign_in_crossing_a_reset_keeps_no_session_past_it() {
    let api = TestApi::start_with(&KEYED).await;
    let paul = api.sign_in(user::PAUL).await;

    // The code queued first: its session opens, then the reset ends it.
    let (_, mut app, _) = enrol(&api.sign_in(user::LEA).await).await;
    let challenge = password_step(&api, &email(user::LEA), PASSWORD).await;
    let (code, path) = (app.next_code(), reset_path(user::LEA));
    let hold = hold_rows(&api, "users", &[user::LEA]).await;
    let (signed_in, reset, ()) = tokio::join!(
        second_step(&api, &challenge, &code),
        async {
            wait_for_waiters(&api, 1).await;
            paul.delete(&path).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let crossed = api.session_from(&signed_in);
    assert_eq!(
        crossed.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    // The reset queued first: the code behind it finds no sign-in waiting.
    let (_, mut app, _) = enrol(&api.sign_in(user::ISABELLE).await).await;
    let challenge = password_step(&api, &email(user::ISABELLE), PASSWORD).await;
    let (code, path) = (app.next_code(), reset_path(user::ISABELLE));
    let hold = hold_rows(&api, "users", &[user::ISABELLE]).await;
    let (reset, late, ()) = tokio::join!(
        paul.delete(&path),
        async {
            wait_for_waiters(&api, 1).await;
            second_step(&api, &challenge, &code).await
        },
        release_after(&api, 2, hold),
    );
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    assert_eq!(late.status, StatusCode::UNAUTHORIZED, "{}", late.text());
    assert!(!opens_a_session(&late));
    assert_eq!(kept_factor(user::ISABELLE).await, None);
}
