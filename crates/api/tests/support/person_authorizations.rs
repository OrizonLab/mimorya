//! Each person's OAuth authorization of an MCP server in the tests
//! (ADR-174): one kept as the platform keeps it, its token issued by the
//! simulated authorization server and sealed for that person, and what the
//! database and the audit keep of them.

use mimorya_api::services::connectors::person_access_sealed_as;
use mimorya_connectors::testing::{CLIENT_ID, FakeAuthorizationServer};
use mimorya_core::demo::tenant;
use mimorya_db::repositories::connector_person_authorizations::{
    NewPersonAuthorization, count_person_authorizations, put_person_authorization,
};
use serde_json::Value;
use uuid::Uuid;

use super::TestApi;

/// Keeps `user`'s authorization of the connector, as if they had given it
/// under its declaration (`resource`): the access token issued, which does
/// not expire.
pub async fn person_authorized(
    api: &TestApi,
    id: &str,
    user: Uuid,
    authority: &FakeAuthorizationServer,
    resource: &str,
) -> String {
    let connector: Uuid = id.parse().expect("id");
    let token = authority.issue(resource);
    let services = &api.app.services;
    let cipher = services.config.secrets.clone().expect("master key");
    let sealed = cipher
        .seal(
            tenant::ID,
            &person_access_sealed_as(connector, user),
            &token,
        )
        .expect("sealed");
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    put_person_authorization(
        &mut tx,
        NewPersonAuthorization {
            tenant_id: tenant::ID,
            connector_id: connector,
            user_id: user,
            issuer: &authority.issuer,
            client_id: CLIENT_ID,
            resource,
            scopes: "",
            access_token: &sealed,
            access_expires_at: None,
            refresh_token: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    token
}

/// How many people's authorizations the connector keeps.
pub async fn person_count(api: &TestApi, id: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let count = count_person_authorizations(&mut tx, id.parse().expect("id"))
        .await
        .unwrap();
    tx.commit().await.unwrap();
    count
}

/// The audited revocations of the connector's people's authorizations:
/// outcome, reason and counts.
pub async fn person_revocations(api: &TestApi, id: &str) -> Vec<(String, Option<String>, Value)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select outcome, reason, metadata from audit_log where resource_id = $1 \
         and action = 'connector.oauth_person_authorizations_revoked' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}

/// The metadata of the connector's last audit entry `action`.
pub async fn last_metadata(api: &TestApi, id: &str, action: &str) -> Value {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let metadata = sqlx::query_scalar(
        "select metadata from audit_log where resource_id = $1 and action = $2 \
         order by id desc limit 1",
    )
    .bind(id)
    .bind(action)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    metadata
}

/// Where a person's own authorization sends the browser back.
pub const PERSON_AUTHORIZED: &str = "/account?serveur=ok";
pub const PERSON_FAILED: &str = "/account?serveur=echec";

/// A server protected by `authority`, declared with OAuth for each person.
pub async fn declared_per_person(
    admin: &super::Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (mimorya_connectors::testing::ProtectedMcp, String) {
    let (server, id) = super::connector_oauth::declared(admin, authority, prefix).await;
    let each = admin
        .put(
            &format!("/admin/connectors/{id}/oauth/holder"),
            serde_json::json!({ "perPerson": true }),
        )
        .await;
    assert_eq!(each.status, axum::http::StatusCode::OK, "{}", each.text());
    (server, id)
}

/// The person's start towards the authorization server: its answer.
pub async fn try_person_start(person: &super::Client<'_>, id: &str) -> super::TestResponse {
    person
        .post(
            &format!("/account/connector-authorizations/{id}"),
            serde_json::json!({}),
        )
        .await
}

/// The authorization server's address and the person's attempt cookie.
pub async fn person_start(person: &super::Client<'_>, id: &str) -> (String, String) {
    let started = try_person_start(person, id).await;
    assert_eq!(
        started.status,
        axum::http::StatusCode::OK,
        "{}",
        started.text()
    );
    let cookie =
        super::sso::set_cookie(&started, super::connector_oauth::COOKIE).expect("attempt cookie");
    let pair = cookie.split(';').next().unwrap().to_string();
    let url = started.json()["redirectUrl"].as_str().unwrap().to_string();
    (url, pair)
}

/// From the person's start to the return, the authorization server agreeing.
pub async fn person_authorize(
    person: &super::Client<'_>,
    authority: &FakeAuthorizationServer,
    id: &str,
) -> super::TestResponse {
    let (url, cookie) = person_start(person, id).await;
    let back = super::connector_oauth::api_path(&authority.sent_back(&url).await);
    super::connector_oauth::come_back(person, &back, Some(&cookie)).await
}

/// The access token the person's own authorization keeps, opened as the
/// server opens it.
pub async fn person_kept(api: &TestApi, id: &str, user: Uuid) -> Option<String> {
    let connector: Uuid = id.parse().expect("id");
    let services = &api.app.services;
    let mut tx = services.db.begin(tenant::ID).await.unwrap();
    let kept = mimorya_db::repositories::connector_person_authorizations::person_access_token(
        &mut tx, connector, user,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let cipher = services.config.secrets.clone().expect("master key");
    kept.map(|kept| {
        cipher
            .open(
                tenant::ID,
                &person_access_sealed_as(connector, user),
                &kept.sealed,
            )
            .expect("sealed for the person")
    })
}

/// The audit entries `action` of this connector by `user`: outcome and
/// reason.
pub async fn entries_by(
    api: &TestApi,
    id: Option<&str>,
    user: Uuid,
    action: &str,
) -> Vec<(String, Option<String>)> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let rows = sqlx::query_as(
        "select outcome, reason from audit_log where actor_user_id = $1 and action = $2 \
         and ($3::text is null or resource_id = $3) order by id",
    )
    .bind(user)
    .bind(action)
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    rows
}
