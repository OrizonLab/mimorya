//! Invitation links (ADR-027): a new account chooses its own password
//! through a one-time link, valid seven days; a new link replaces the former
//! one; a disabled account loses its link; only company-wide account
//! administrators issue them, and the token reaches neither the audit nor a
//! second use.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{tenant, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::{Value, json};
use support::accounts::{INVITED_PASSWORD, accept, directory_entry, invite_account};
use support::mail::token_of;
use support::{TestApi, owner_url};

const UNUSABLE: &str =
    "Ce lien d’invitation n’est plus valable : demandez-en un nouveau à votre administrateur";

async fn inspect(api: &TestApi, link: &str) -> support::TestResponse {
    api.anonymous()
        .post("/auth/invitation", json!({ "token": token_of(link) }))
        .await
}

fn link(invitation: &Value) -> String {
    invitation["link"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_invitation_activates_the_account_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "zoe.leroy").await;
    let link = link(&invitation);

    let shown = inspect(&api, &link).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let shown = shown.json();
    assert_eq!(shown["email"], account.email);
    assert_eq!(shown["displayName"], "Nouveau Collègue");
    assert_eq!(shown["company"], "Néo Maintenance (démo)");
    assert_eq!(shown["expiresAt"], invitation["expiresAt"]);
    let listed = directory_entry(&paul, account.id).await;
    assert_eq!(listed["status"], "invited");
    assert_eq!(listed["invitationExpiresAt"], invitation["expiresAt"]);
    // No password yet: no sign-in.
    let refused = api.try_login(&account.email, INVITED_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);

    let short = accept(&api, &link, "court-11car").await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST);
    let accepted = accept(&api, &link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let newcomer = api.session_from(&accepted);
    let me = newcomer.get("/auth/me").await.json();
    assert_eq!(me["user"]["mustChangePassword"], false);
    let listed = directory_entry(&paul, account.id).await;
    assert_eq!(listed["status"], "active");
    assert_eq!(listed["invitationExpiresAt"], Value::Null);
    api.sign_in_with(&account.email, INVITED_PASSWORD).await;

    // Once only.
    for used in [
        inspect(&api, &link).await,
        accept(&api, &link, "autre-phrase-de-passe").await,
    ] {
        assert_eq!(used.status, StatusCode::NOT_FOUND);
        assert_eq!(used.error_message(), UNUSABLE);
    }
    let again = paul
        .post(
            &format!("/admin/users/{}/invitation", account.id),
            json!({}),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<(String, Value)> =
        sqlx::query_as("select action, metadata from audit_log where resource_id = $1 order by id")
            .bind(account.id.to_string())
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    drop(tx);
    let actions: Vec<&str> = audited.iter().map(|(a, _)| a.as_str()).collect();
    assert_eq!(
        actions,
        ["user.created", "user.invited", "user.invitation_accepted"]
    );
    let token = token_of(&link);
    assert!(audited.iter().all(|(_, m)| !m.to_string().contains(&token)));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_link_replaces_the_former_one_and_links_expire() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, first) = invite_account(&paul, "remi.blanc").await;
    let resend = format!("/admin/users/{}/invitation", account.id);
    let second = paul.post(&resend, json!({})).await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.text());
    assert_eq!(second.headers["cache-control"], "no-store");
    let second = link(&second.json()["invitation"]);
    assert_eq!(
        inspect(&api, &link(&first)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(inspect(&api, &second).await.status, StatusCode::OK);

    // An invited account has no password to reset: its link is resent.
    let reset = paul
        .post(&format!("/admin/users/{}/password", account.id), json!({}))
        .await;
    assert_eq!(reset.status, StatusCode::CONFLICT);

    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update user_invitations set expires_at = now() - interval '1 second'")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(inspect(&api, &second).await.status, StatusCode::NOT_FOUND);
    let late = accept(&api, &second, INVITED_PASSWORD).await;
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    let third = paul.post(&resend, json!({})).await.json();
    let accepted = accept(&api, &link(&third["invitation"]), INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_disabled_account_loses_its_link() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (account, invitation) = invite_account(&paul, "ines.morel").await;
    let url = format!("/admin/users/{}", account.id);
    let disabled = paul.patch(&url, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    let refused = accept(&api, &link(&invitation), INVITED_PASSWORD).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);

    // Enabled again, it is still waiting for its password, with no link left.
    let enabled = paul.patch(&url, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK);
    let listed = directory_entry(&paul, account.id).await;
    assert_eq!(listed["status"], "invited");
    assert_eq!(listed["invitationExpiresAt"], Value::Null);
    assert_eq!(
        inspect(&api, &link(&invitation)).await.status,
        StatusCode::NOT_FOUND
    );
    let resent = paul
        .post(
            &format!("/admin/users/{}/invitation", account.id),
            json!({}),
        )
        .await;
    assert_eq!(resent.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_s_links_do_not_work() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (_, invitation) = invite_account(&paul, "invitation.suspendue").await;
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let suspend = |status: &'static str| {
        sqlx::query("update tenants set status = $1 where id = $2")
            .bind(status)
            .bind(tenant::ID)
            .execute(&owner)
    };
    suspend("suspended").await.unwrap();
    let refused = inspect(&api, &link(&invitation)).await;
    let refused_accept = accept(&api, &link(&invitation), INVITED_PASSWORD).await;
    suspend("active").await.unwrap();
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert_eq!(refused_accept.status, StatusCode::NOT_FOUND);
    assert_eq!(
        inspect(&api, &link(&invitation)).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_company_wide_account_administrators_invite() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (account, _) = invite_account(&paul, "invitation.droits").await;
    let resend = format!("/admin/users/{}/invitation", account.id);
    assert_eq!(
        marc.post(&resend, json!({})).await.status,
        StatusCode::FORBIDDEN
    );
    for unknown in [uuid::Uuid::new_v4(), OTHER_USER_ID] {
        let refused = paul
            .post(&format!("/admin/users/{unknown}/invitation"), json!({}))
            .await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND);
    }
    // An unknown or malformed token gets the same answer as a used one.
    for token in ["inconnu", &"x".repeat(43)] {
        let refused = api
            .anonymous()
            .post("/auth/invitation", json!({ "token": token }))
            .await;
        assert_eq!(refused.status, StatusCode::NOT_FOUND);
        assert_eq!(refused.error_message(), UNUSABLE);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn invitation_links_are_rate_limited_per_client() {
    let api = TestApi::start_with(&[("LOGIN_RATE_LIMIT_PER_MINUTE", "2")]).await;
    let statuses = [
        api.anonymous()
            .post("/auth/invitation", json!({ "token": "a" }))
            .await
            .status,
        api.anonymous()
            .post("/auth/invitation", json!({ "token": "b" }))
            .await
            .status,
        api.anonymous()
            .post(
                "/auth/invitation/accept",
                json!({ "token": "c", "password": INVITED_PASSWORD }),
            )
            .await
            .status,
    ];
    assert_eq!(
        statuses,
        [
            StatusCode::NOT_FOUND,
            StatusCode::NOT_FOUND,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );
}
